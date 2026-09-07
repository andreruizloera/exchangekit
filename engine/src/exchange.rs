use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::book::Book;
use crate::error::EngineError;
use crate::settlement::{compute_payouts, unbacked, Holding, Resolution, Settlement, SHARE_PAYOUT};
use crate::types::{
    BookView, Cash, Market, Order, OrderId, OrderRequest, OrderStatus, Outcome, PlaceResult, Price,
    Qty, Side, TimeInForce, Trade, TradeId, TradeKind,
};

/// What a YES share and a NO share of one market are worth together, as a
/// price rather than a cash amount. Exactly one of the two wins and is paid
/// [`SHARE_PAYOUT`], so a bid of `p` on one outcome is an offer of
/// `PAIR_CENTS - p` on the other. Prices are whole cents and so is this, so
/// the two halves of a complementary cross always add up with nothing left
/// over to round.
const PAIR_CENTS: Price = SHARE_PAYOUT as Price;

/// One matchable opportunity for a taker order: a resting order it can
/// execute against, and at what price.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    /// The resting order on the other side of the fill.
    maker: OrderId,
    /// That order's sequence number, which is its time priority.
    seq: u64,
    /// Execution price in the taker's outcome frame.
    taker_price: Price,
    /// Execution price in the maker's outcome frame. The same number for a
    /// same-book cross; `PAIR_CENTS - taker_price` for a complementary one,
    /// which is exactly why a minted pair is self-funding.
    maker_price: Price,
    kind: TradeKind,
}

impl Candidate {
    /// True if `self` is the better of the two for a taker on `side`.
    /// Better price wins; equal prices go to the older resting order, so
    /// price-time priority holds across both books and not just within one.
    fn beats(&self, other: &Candidate, side: Side) -> bool {
        if self.taker_price != other.taker_price {
            return match side {
                Side::Buy => self.taker_price < other.taker_price,
                Side::Sell => self.taker_price > other.taker_price,
            };
        }
        self.seq < other.seq
    }
}

/// Shares held in one outcome of one market.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    /// Total shares owned.
    pub quantity: i64,
    /// Shares committed to open sell orders.
    pub locked: i64,
}

impl Position {
    pub fn available(&self) -> i64 {
        self.quantity - self.locked
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct MarketPosition {
    pub yes: Position,
    pub no: Position,
}

impl MarketPosition {
    fn get_mut(&mut self, outcome: Outcome) -> &mut Position {
        match outcome {
            Outcome::Yes => &mut self.yes,
            Outcome::No => &mut self.no,
        }
    }

    pub fn get(&self, outcome: Outcome) -> Position {
        match outcome {
            Outcome::Yes => self.yes,
            Outcome::No => self.no,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// Total play-money cash in cents.
    pub balance: Cash,
    /// Cash committed to open buy orders.
    pub locked_cash: Cash,
    /// Positions keyed by market id.
    pub positions: HashMap<String, MarketPosition>,
}

impl Account {
    pub fn available_cash(&self) -> Cash {
        self.balance - self.locked_cash
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct MarketBooks {
    yes: Book,
    no: Book,
}

impl MarketBooks {
    fn get_mut(&mut self, outcome: Outcome) -> &mut Book {
        match outcome {
            Outcome::Yes => &mut self.yes,
            Outcome::No => &mut self.no,
        }
    }

    fn get(&self, outcome: Outcome) -> &Book {
        match outcome {
            Outcome::Yes => &self.yes,
            Outcome::No => &self.no,
        }
    }
}

/// The whole exchange: markets, books, orders, accounts, trade history.
/// Purely in-memory and single-threaded; wrap in a lock for concurrent use.
/// Serializable as a snapshot for optional persistence.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Exchange {
    markets: BTreeMap<String, Market>,
    books: HashMap<String, MarketBooks>,
    orders: HashMap<OrderId, Order>,
    accounts: BTreeMap<String, Account>,
    trades: HashMap<String, Vec<Trade>>,
    /// Cents backing each market's outstanding shares. Minting a YES/NO
    /// pair adds 100 cents here; resolution pays out of it. Absent from
    /// snapshots taken before settlement existed, which load as zero.
    #[serde(default)]
    collateral: BTreeMap<String, Cash>,
    /// The settlement report of every resolved market, kept so a caller
    /// can read what happened long after the fact.
    #[serde(default)]
    settlements: BTreeMap<String, Settlement>,
    next_order_id: OrderId,
    next_trade_id: TradeId,
    next_seq: u64,
}

impl Exchange {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- setup -----------------------------------------------------------

    pub fn create_account(&mut self, id: &str, balance: Cash) -> Result<(), EngineError> {
        if self.accounts.contains_key(id) {
            return Err(EngineError::AccountExists(id.to_string()));
        }
        self.accounts.insert(
            id.to_string(),
            Account {
                id: id.to_string(),
                balance,
                locked_cash: 0,
                positions: HashMap::new(),
            },
        );
        Ok(())
    }

    pub fn create_market(
        &mut self,
        id: &str,
        question: &str,
        description: &str,
        now_ms: u64,
    ) -> Result<(), EngineError> {
        if self.markets.contains_key(id) {
            return Err(EngineError::MarketExists(id.to_string()));
        }
        self.markets.insert(
            id.to_string(),
            Market {
                id: id.to_string(),
                question: question.to_string(),
                description: description.to_string(),
                created_at: now_ms,
                resolution: None,
            },
        );
        self.books.insert(id.to_string(), MarketBooks::default());
        self.trades.insert(id.to_string(), Vec::new());
        Ok(())
    }

    /// An open market, or the error explaining why it is not usable.
    fn require_open(&self, market: &str) -> Result<(), EngineError> {
        match self.markets.get(market) {
            None => Err(EngineError::UnknownMarket(market.to_string())),
            Some(m) if m.is_resolved() => Err(EngineError::MarketResolved(market.to_string())),
            Some(_) => Ok(()),
        }
    }

    /// Mint `qty` YES/NO pairs into an account against collateral: the
    /// account pays [`SHARE_PAYOUT`] cents per pair and receives one share
    /// of each outcome, and the cents go into the market's collateral pool.
    ///
    /// This is the funded way to create shares. Exactly one share of each
    /// pair wins at settlement and is paid 100 cents, so the pool always
    /// holds what the market will owe. Pairs are the reason a settlement
    /// can report zero unbacked cash.
    pub fn mint_pair(&mut self, account: &str, market: &str, qty: Qty) -> Result<(), EngineError> {
        self.require_open(market)?;
        if qty == 0 {
            return Err(EngineError::InvalidQuantity);
        }
        let cost = SHARE_PAYOUT * qty as Cash;
        let acct = self
            .accounts
            .get_mut(account)
            .ok_or_else(|| EngineError::UnknownAccount(account.to_string()))?;
        let available = acct.available_cash();
        if cost > available {
            return Err(EngineError::InsufficientBalance {
                need: cost,
                available,
            });
        }
        acct.balance -= cost;
        let pos = acct.positions.entry(market.to_string()).or_default();
        pos.yes.quantity += qty as i64;
        pos.no.quantity += qty as i64;
        *self.collateral.entry(market.to_string()).or_insert(0) += cost;
        Ok(())
    }

    /// Redeem `qty` YES/NO pairs out of an account: the account gives up
    /// one share of each outcome per pair and gets [`SHARE_PAYOUT`] cents
    /// back, drawn from the market's collateral pool.
    ///
    /// This is the exact inverse of [`Exchange::mint_pair`] and it is what
    /// makes a pair worth 100 cents before resolution rather than only at
    /// it: holding both sides of a binary question is holding a dollar, so
    /// the exchange will hand the dollar over on demand.
    ///
    /// Shares committed to a resting sell order do not count. Cancel the
    /// order first, or the redemption would spend a share that is already
    /// promised to a buyer. The market must also hold the collateral: the
    /// engine will not release cents it never took in, which is what keeps
    /// the pool from going negative on a market whose shares were granted
    /// rather than minted.
    pub fn redeem_pair(
        &mut self,
        account: &str,
        market: &str,
        qty: Qty,
    ) -> Result<(), EngineError> {
        self.require_open(market)?;
        if qty == 0 {
            return Err(EngineError::InvalidQuantity);
        }
        let proceeds = SHARE_PAYOUT * qty as Cash;
        let held = self.collateral(market);
        if proceeds > held {
            return Err(EngineError::InsufficientCollateral {
                need: proceeds,
                available: held,
            });
        }
        let acct = self
            .accounts
            .get_mut(account)
            .ok_or_else(|| EngineError::UnknownAccount(account.to_string()))?;
        let pos = acct.positions.entry(market.to_string()).or_default();
        for outcome in [Outcome::Yes, Outcome::No] {
            let available = pos.get(outcome).available();
            if (qty as i64) > available {
                return Err(EngineError::InsufficientPosition {
                    need: qty as i64,
                    available,
                });
            }
        }
        for outcome in [Outcome::Yes, Outcome::No] {
            pos.get_mut(outcome).quantity -= qty as i64;
        }
        acct.balance += proceeds;
        *self.collateral.entry(market.to_string()).or_insert(0) -= proceeds;
        Ok(())
    }

    /// Mint shares into an account for free, with no collateral behind
    /// them. Used for seeding demo liquidity and for the one-sided
    /// inventory a game round hands out.
    ///
    /// Granted shares are real shares: they trade and they settle. What
    /// they do not carry is funding, so settling them creates cash that no
    /// collateral stood behind. [`Exchange::resolve_market`] reports that
    /// as `unbacked_cash` rather than letting it pass silently. Use
    /// [`Exchange::mint_pair`] where the money has to add up.
    pub fn grant_shares(
        &mut self,
        account: &str,
        market: &str,
        outcome: Outcome,
        qty: Qty,
    ) -> Result<(), EngineError> {
        self.require_open(market)?;
        let acct = self
            .accounts
            .get_mut(account)
            .ok_or_else(|| EngineError::UnknownAccount(account.to_string()))?;
        acct.positions
            .entry(market.to_string())
            .or_default()
            .get_mut(outcome)
            .quantity += qty as i64;
        Ok(())
    }

    // ---- trading ---------------------------------------------------------

    /// Submit a limit order. If it crosses resting orders it fills
    /// immediately at the resting prices (a marketable limit order);
    /// any remainder rests in the book at the limit price.
    ///
    /// An order can fill against either book. Buying NO at `q` is the same
    /// trade as selling YES at `PAIR_CENTS - q`, so a resting NO bid is an
    /// offer of YES and a resting NO ask is a bid for YES. A taker takes
    /// the best price available across both books, so the two outcomes of
    /// a market are one order book seen from two directions.
    ///
    /// Crossing the complementary book does not move shares between two
    /// accounts, because the accounts on the two sides want opposite
    /// outcomes. It creates or destroys them instead:
    ///
    /// - Two buyers cross: the pair is minted. They pay `p` and
    ///   `PAIR_CENTS - p`, which is exactly the collateral one pair needs,
    ///   so the mint funds itself and no unbacked cash is created.
    /// - Two sellers cross: the pair is burned and the 100 cents of
    ///   collateral behind it is released to pay them, again splitting
    ///   exactly. A burn is only offered while the market's collateral pool
    ///   can cover it; the engine will not release cents it does not hold.
    #[allow(clippy::too_many_arguments)]
    pub fn place_order(
        &mut self,
        account: &str,
        market: &str,
        outcome: Outcome,
        side: Side,
        price: Price,
        quantity: Qty,
        now_ms: u64,
    ) -> Result<PlaceResult, EngineError> {
        self.place(OrderRequest::limit(
            account, market, outcome, side, price, quantity, now_ms,
        ))
    }

    /// Submit an order with a [`TimeInForce`] other than the default.
    /// [`Exchange::place_order`] is this with an ordinary limit order.
    ///
    /// The three non-default policies are all decided before any state
    /// moves, so a refused order takes no escrow and leaves no trace in
    /// either book:
    ///
    /// - `FillOrKill` asks the books how much they could supply at the
    ///   limit, counting both of them and the pairs the collateral pool
    ///   could afford to burn, and refuses unless that covers the whole
    ///   quantity.
    /// - `PostOnly` refuses if anything would cross, in either book. A
    ///   maker quoting 63 for YES is offering NO at 37, so an order that
    ///   does not cross its own book can still be taking.
    /// - `ImmediateOrCancel` matches normally and then cancels its
    ///   remainder instead of resting, releasing that remainder's escrow.
    ///
    /// A killed or refused order comes back with status
    /// [`OrderStatus::Rejected`] rather than an error: its terms were
    /// honoured, and nothing about the request was malformed.
    pub fn place(&mut self, req: OrderRequest<'_>) -> Result<PlaceResult, EngineError> {
        let OrderRequest {
            account,
            market,
            outcome,
            side,
            price,
            quantity,
            time_in_force,
            now_ms,
        } = req;
        if !(1..=99).contains(&price) {
            return Err(EngineError::InvalidPrice(price));
        }
        if quantity == 0 {
            return Err(EngineError::InvalidQuantity);
        }
        self.require_open(market)?;
        if !self.accounts.contains_key(account) {
            return Err(EngineError::UnknownAccount(account.to_string()));
        }

        // Decide the refusing policies first, while nothing has moved. A
        // rejected order still gets an id and a record, so a caller can ask
        // what happened to it, but it never touches escrow or a book.
        let refuse = match time_in_force {
            TimeInForce::FillOrKill => self.fillable(market, outcome, side, price) < quantity,
            TimeInForce::PostOnly => self
                .best_candidate(market, outcome, side, price, self.burn_budget(market))
                .is_some(),
            _ => false,
        };
        if refuse {
            self.next_order_id += 1;
            self.next_seq += 1;
            let id = self.next_order_id;
            self.orders.insert(
                id,
                Order {
                    id,
                    account: account.to_string(),
                    market: market.to_string(),
                    outcome,
                    side,
                    price,
                    quantity,
                    filled: 0,
                    status: OrderStatus::Rejected,
                    time_in_force,
                    seq: self.next_seq,
                    created_at: now_ms,
                },
            );
            return Ok(PlaceResult {
                order: self.orders[&id].clone(),
                trades: Vec::new(),
            });
        }

        // Escrow: buys lock cash at the limit price, sells lock shares.
        {
            let acct = self
                .accounts
                .get_mut(account)
                .ok_or_else(|| EngineError::UnknownAccount(account.to_string()))?;
            match side {
                Side::Buy => {
                    let need = price as Cash * quantity as Cash;
                    let available = acct.available_cash();
                    if need > available {
                        return Err(EngineError::InsufficientBalance { need, available });
                    }
                    acct.locked_cash += need;
                }
                Side::Sell => {
                    let pos = acct
                        .positions
                        .entry(market.to_string())
                        .or_default()
                        .get_mut(outcome);
                    let available = pos.available();
                    if (quantity as i64) > available {
                        return Err(EngineError::InsufficientPosition {
                            need: quantity as i64,
                            available,
                        });
                    }
                    pos.locked += quantity as i64;
                }
            }
        }

        self.next_order_id += 1;
        self.next_seq += 1;
        let taker_id = self.next_order_id;
        let order = Order {
            id: taker_id,
            account: account.to_string(),
            market: market.to_string(),
            outcome,
            side,
            price,
            quantity,
            filled: 0,
            status: OrderStatus::Open,
            time_in_force,
            seq: self.next_seq,
            created_at: now_ms,
        };
        self.orders.insert(taker_id, order);

        let mut trades = Vec::new();

        // Match, best price first across both books, FIFO within a level.
        loop {
            let taker_remaining = self.orders[&taker_id].remaining();
            if taker_remaining == 0 {
                break;
            }
            // A burn pays out of the market's collateral pool, so the pool
            // caps how many pairs one fill may destroy. An empty pool takes
            // the complementary side off the table rather than lending
            // against cents the market does not hold.
            let burn_budget = self.burn_budget(market);
            let Some(c) = self.best_candidate(market, outcome, side, price, burn_budget) else {
                break;
            };

            let mut fill = taker_remaining.min(self.orders[&c.maker].remaining());
            if c.kind == TradeKind::Burn {
                fill = fill.min(burn_budget);
            }
            debug_assert!(fill > 0, "a candidate that cannot fill must not be offered");

            // The complementary maker is a buyer when the taker is a buyer
            // and a seller when the taker is a seller, but of the other
            // outcome; in this outcome's frame that puts it on the opposite
            // side of the trade, exactly like a same-book maker.
            let (buy_id, sell_id) = match side {
                Side::Buy => (taker_id, c.maker),
                Side::Sell => (c.maker, taker_id),
            };
            match c.kind {
                TradeKind::Match => {
                    self.settle_fill(market, outcome, buy_id, sell_id, c.taker_price, fill)
                }
                TradeKind::Mint => self.settle_mint(market, outcome, taker_id, c, fill),
                TradeKind::Burn => self.settle_burn(market, outcome, taker_id, c, fill),
            }

            for id in [taker_id, c.maker] {
                let o = self.orders.get_mut(&id).expect("order exists");
                o.filled += fill;
                if o.remaining() == 0 {
                    o.status = OrderStatus::Filled;
                }
            }
            if self.orders[&c.maker].remaining() == 0 {
                let (maker_outcome, maker_is_bid) = match c.kind {
                    TradeKind::Match => (outcome, side == Side::Sell),
                    // A complementary maker rests in the other book on the
                    // same side the taker is on here: two buyers mint, two
                    // sellers burn.
                    _ => (outcome.complement(), side == Side::Buy),
                };
                self.books
                    .get_mut(market)
                    .expect("market book")
                    .get_mut(maker_outcome)
                    .remove(maker_is_bid, c.maker_price, c.maker);
            }

            self.next_trade_id += 1;
            let trade = Trade {
                id: self.next_trade_id,
                market: market.to_string(),
                outcome,
                price: c.taker_price,
                quantity: fill,
                taker_side: side,
                buyer: self.orders[&buy_id].account.clone(),
                seller: self.orders[&sell_id].account.clone(),
                buy_order: buy_id,
                sell_order: sell_id,
                kind: c.kind,
                ts: now_ms,
            };
            self.trades
                .get_mut(market)
                .expect("market trades")
                .push(trade.clone());
            trades.push(trade);
        }

        // Rest any remainder in the book at the limit price, unless the
        // order said not to. An immediate-or-cancel remainder is cancelled
        // here and its escrow released, which is the whole point of the
        // policy: it is never available to anyone else.
        let remaining = self.orders[&taker_id].remaining();
        if remaining > 0 {
            if time_in_force == TimeInForce::ImmediateOrCancel {
                let acct = self.accounts.get_mut(account).expect("account exists");
                match side {
                    Side::Buy => acct.locked_cash -= price as Cash * remaining as Cash,
                    Side::Sell => {
                        acct.positions
                            .entry(market.to_string())
                            .or_default()
                            .get_mut(outcome)
                            .locked -= remaining as i64;
                    }
                }
                self.orders.get_mut(&taker_id).expect("order exists").status =
                    OrderStatus::Cancelled;
            } else {
                let book = self
                    .books
                    .get_mut(market)
                    .expect("market book")
                    .get_mut(outcome);
                match side {
                    Side::Buy => book.add_bid(price, taker_id),
                    Side::Sell => book.add_ask(price, taker_id),
                }
            }
        }

        Ok(PlaceResult {
            order: self.orders[&taker_id].clone(),
            trades,
        })
    }

    /// How many pairs a market's collateral pool could afford to burn.
    fn burn_budget(&self, market: &str) -> Qty {
        (self.collateral(market) / SHARE_PAYOUT).max(0) as Qty
    }

    /// How much of a taker order at `limit` the books could fill right now,
    /// across both of them. Fill-or-kill has to know this before it takes
    /// any escrow, and the total does not depend on the order the levels
    /// would be taken in, so this sums rather than simulating the walk.
    fn fillable(&self, market: &str, outcome: Outcome, side: Side, limit: Price) -> Qty {
        let Some(books) = self.books.get(market) else {
            return 0;
        };
        let same = books.get(outcome);
        let other = books.get(outcome.complement());
        let depth = |queue: &std::collections::VecDeque<OrderId>| -> Qty {
            queue.iter().map(|&id| self.orders[&id].remaining()).sum()
        };
        match side {
            Side::Buy => {
                // Asks at or below the limit, plus complementary bids high
                // enough that the pair they offer costs no more.
                let own: Qty = same
                    .asks
                    .range(..=limit)
                    .map(|(_, queue)| depth(queue))
                    .sum();
                let cross: Qty = other
                    .bids
                    .range(PAIR_CENTS.saturating_sub(limit)..)
                    .map(|(_, queue)| depth(queue))
                    .sum();
                own + cross
            }
            Side::Sell => {
                let own: Qty = same
                    .bids
                    .range(limit..)
                    .map(|(_, queue)| depth(queue))
                    .sum();
                // Burning is capped by what the pool can release, so the
                // complementary side may be worth less than it looks.
                let cross: Qty = other
                    .asks
                    .range(..=PAIR_CENTS.saturating_sub(limit))
                    .map(|(_, queue)| depth(queue))
                    .sum();
                own + cross.min(self.burn_budget(market))
            }
        }
    }

    /// The best fill available to a taker right now, or `None` if nothing
    /// crosses its limit. Looks at the front of the best level of this
    /// outcome's own book and at the front of the best complementary level,
    /// and returns whichever is better on price, older on ties.
    ///
    /// `burn_budget` is how many pairs the market's collateral pool can
    /// afford to destroy. At zero the complementary sell side is not
    /// offered at all, which also keeps the matching loop terminating: a
    /// candidate is only returned if it can fill at least one share.
    fn best_candidate(
        &self,
        market: &str,
        outcome: Outcome,
        side: Side,
        limit: Price,
        burn_budget: Qty,
    ) -> Option<Candidate> {
        let books = self.books.get(market)?;
        let same = books.get(outcome);
        let other = books.get(outcome.complement());
        let front = |queue: &std::collections::VecDeque<OrderId>| {
            let id = *queue.front().expect("levels are never empty");
            (id, self.orders[&id].seq)
        };

        let mut best: Option<Candidate> = None;
        let mut offer = |c: Candidate| {
            if best.is_none_or(|b| c.beats(&b, side)) {
                best = Some(c);
            }
        };

        match side {
            Side::Buy => {
                if let Some(ask) = same.best_ask().filter(|&a| a <= limit) {
                    let (maker, seq) = front(&same.asks[&ask]);
                    offer(Candidate {
                        maker,
                        seq,
                        taker_price: ask,
                        maker_price: ask,
                        kind: TradeKind::Match,
                    });
                }
                // A resting bid on the other outcome is an offer of this
                // one: bidding q for NO is offering YES at 100 - q.
                if let Some(bid) = other
                    .best_bid()
                    .filter(|&q| PAIR_CENTS - q <= limit && q < PAIR_CENTS)
                {
                    let (maker, seq) = front(&other.bids[&bid]);
                    offer(Candidate {
                        maker,
                        seq,
                        taker_price: PAIR_CENTS - bid,
                        maker_price: bid,
                        kind: TradeKind::Mint,
                    });
                }
            }
            Side::Sell => {
                if let Some(bid) = same.best_bid().filter(|&b| b >= limit) {
                    let (maker, seq) = front(&same.bids[&bid]);
                    offer(Candidate {
                        maker,
                        seq,
                        taker_price: bid,
                        maker_price: bid,
                        kind: TradeKind::Match,
                    });
                }
                // A resting ask on the other outcome is a bid for this one:
                // offering NO at a is bidding 100 - a for YES.
                if burn_budget > 0 {
                    if let Some(ask) = other
                        .best_ask()
                        .filter(|&a| a < PAIR_CENTS && PAIR_CENTS - a >= limit)
                    {
                        let (maker, seq) = front(&other.asks[&ask]);
                        offer(Candidate {
                            maker,
                            seq,
                            taker_price: PAIR_CENTS - ask,
                            maker_price: ask,
                            kind: TradeKind::Burn,
                        });
                    }
                }
            }
        }
        best
    }

    /// Cross two buyers on opposite outcomes by minting the pair they are
    /// paying for between them.
    ///
    /// The taker pays its execution price and the maker pays the maker's,
    /// and those two sum to [`SHARE_PAYOUT`], so the cents that leave the
    /// two accounts are exactly the cents the collateral pool takes in. No
    /// cash is created and none is destroyed: it moves from accounts into
    /// the pool that will pay the winning half of the pair back out.
    fn settle_mint(
        &mut self,
        market: &str,
        taker_outcome: Outcome,
        taker_order: OrderId,
        c: Candidate,
        qty: Qty,
    ) {
        debug_assert_eq!(
            c.taker_price + c.maker_price,
            PAIR_CENTS,
            "the two buyers must fund the whole pair and no more"
        );
        for (order, outcome, exec) in [
            (taker_order, taker_outcome, c.taker_price),
            (c.maker, taker_outcome.complement(), c.maker_price),
        ] {
            // Both sides escrowed cash at their own limit; both pay their
            // execution price, and any difference goes back to available.
            let limit = self.orders[&order].price;
            let who = self.orders[&order].account.clone();
            let acct = self.accounts.get_mut(&who).expect("buyer account");
            acct.locked_cash -= limit as Cash * qty as Cash;
            acct.balance -= exec as Cash * qty as Cash;
            acct.positions
                .entry(market.to_string())
                .or_default()
                .get_mut(outcome)
                .quantity += qty as i64;
        }
        *self.collateral.entry(market.to_string()).or_insert(0) += SHARE_PAYOUT * qty as Cash;
    }

    /// Cross two sellers on opposite outcomes by burning the pair they hold
    /// between them and releasing its collateral to pay them.
    ///
    /// Both sides had their shares locked against a resting or incoming
    /// sell, so the escrow is released and the shares destroyed in the same
    /// step. The 100 cents that leave the pool are exactly the cents the
    /// two accounts receive.
    fn settle_burn(
        &mut self,
        market: &str,
        taker_outcome: Outcome,
        taker_order: OrderId,
        c: Candidate,
        qty: Qty,
    ) {
        debug_assert_eq!(
            c.taker_price + c.maker_price,
            PAIR_CENTS,
            "a burned pair pays out exactly what it was collateralized for"
        );
        for (order, outcome, exec) in [
            (taker_order, taker_outcome, c.taker_price),
            (c.maker, taker_outcome.complement(), c.maker_price),
        ] {
            let who = self.orders[&order].account.clone();
            let acct = self.accounts.get_mut(&who).expect("seller account");
            acct.balance += exec as Cash * qty as Cash;
            let pos = acct
                .positions
                .entry(market.to_string())
                .or_default()
                .get_mut(outcome);
            pos.locked -= qty as i64;
            pos.quantity -= qty as i64;
        }
        let pool = self.collateral.entry(market.to_string()).or_insert(0);
        *pool -= SHARE_PAYOUT * qty as Cash;
        debug_assert!(
            *pool >= 0,
            "a burn must never release cents the market does not hold"
        );
    }

    /// Move cash and shares for one fill. Buys were escrowed at their
    /// limit price; when a buy fills below its limit the difference is
    /// released back to available cash.
    fn settle_fill(
        &mut self,
        market: &str,
        outcome: Outcome,
        buy_order: OrderId,
        sell_order: OrderId,
        price: Price,
        qty: Qty,
    ) {
        let cost = price as Cash * qty as Cash;
        let buy_limit = self.orders[&buy_order].price;
        let buyer = self.orders[&buy_order].account.clone();
        let seller = self.orders[&sell_order].account.clone();

        let b = self.accounts.get_mut(&buyer).expect("buyer account");
        b.locked_cash -= buy_limit as Cash * qty as Cash;
        b.balance -= cost;
        b.positions
            .entry(market.to_string())
            .or_default()
            .get_mut(outcome)
            .quantity += qty as i64;

        let s = self.accounts.get_mut(&seller).expect("seller account");
        s.balance += cost;
        let pos = s
            .positions
            .entry(market.to_string())
            .or_default()
            .get_mut(outcome);
        pos.locked -= qty as i64;
        pos.quantity -= qty as i64;
    }

    /// Cancel an open order and release its escrow.
    pub fn cancel_order(&mut self, account: &str, order_id: OrderId) -> Result<Order, EngineError> {
        let order = self
            .orders
            .get(&order_id)
            .ok_or(EngineError::UnknownOrder(order_id))?
            .clone();
        if order.account != account {
            return Err(EngineError::NotOrderOwner {
                order: order_id,
                account: account.to_string(),
            });
        }
        if order.status != OrderStatus::Open {
            return Err(EngineError::OrderNotOpen(order_id));
        }

        let removed = self
            .books
            .get_mut(&order.market)
            .expect("market book")
            .get_mut(order.outcome)
            .remove(order.side == Side::Buy, order.price, order_id);
        debug_assert!(removed, "open order must be resting in the book");

        let remaining = order.remaining();
        let acct = self.accounts.get_mut(account).expect("account exists");
        match order.side {
            Side::Buy => acct.locked_cash -= order.price as Cash * remaining as Cash,
            Side::Sell => {
                acct.positions
                    .entry(order.market.clone())
                    .or_default()
                    .get_mut(order.outcome)
                    .locked -= remaining as i64;
            }
        }

        let o = self.orders.get_mut(&order_id).expect("order exists");
        o.status = OrderStatus::Cancelled;
        Ok(o.clone())
    }

    /// Cancel every open order for an account, then reset its cash to
    /// `balance` with no locked cash and no positions. Creates the account
    /// if it does not exist. This exists so a game round can start each
    /// player and bot from a clean slate without disturbing any other
    /// account or the seeded markets; it is not part of normal trading.
    ///
    /// It destroys shares without touching any collateral pool, so calling
    /// it on an account holding shares of a collateralized market would
    /// leave that market over-collateralized. The game only ever resets its
    /// own player and bot accounts, which hold nothing but the one-sided
    /// inventory the round granted them.
    pub fn reset_account(&mut self, id: &str, balance: Cash) {
        let open: Vec<OrderId> = self
            .orders
            .values()
            .filter(|o| o.account == id && o.status == OrderStatus::Open)
            .map(|o| o.id)
            .collect();
        for oid in open {
            let _ = self.cancel_order(id, oid);
        }
        let acct = self
            .accounts
            .entry(id.to_string())
            .or_insert_with(|| Account {
                id: id.to_string(),
                balance: 0,
                locked_cash: 0,
                positions: HashMap::new(),
            });
        acct.balance = balance;
        acct.locked_cash = 0;
        acct.positions.clear();
    }

    // ---- settlement ------------------------------------------------------

    /// Settle a market to `winner`. Every share of the winning outcome pays
    /// [`SHARE_PAYOUT`] cents, every share of the other pays nothing, every
    /// resting order is voided, and the market stops trading for good.
    ///
    /// Orders are voided BEFORE anything is paid, and that order matters. A
    /// resting sell has shares locked against it and a resting buy has cash
    /// locked against it; clearing positions first would leave that escrow
    /// pointing at a position that no longer exists, and the account would
    /// carry a lock it could never release on a market that no longer
    /// trades. Releasing the escrow first means every share the payout sees
    /// is a share its owner actually holds free and clear.
    ///
    /// Resolution is not reversible and cannot be repeated: a second call
    /// returns [`EngineError::MarketAlreadyResolved`] rather than paying
    /// twice.
    pub fn resolve_market(
        &mut self,
        market: &str,
        winner: Outcome,
        now_ms: u64,
    ) -> Result<Settlement, EngineError> {
        match self.markets.get(market) {
            None => return Err(EngineError::UnknownMarket(market.to_string())),
            Some(m) if m.is_resolved() => {
                return Err(EngineError::MarketAlreadyResolved(market.to_string()))
            }
            Some(_) => {}
        }

        // 1. Void every resting order in both books and release its escrow.
        let mut open: Vec<OrderId> = self
            .orders
            .values()
            .filter(|o| o.market == market && o.status == OrderStatus::Open)
            .map(|o| o.id)
            .collect();
        open.sort_unstable();

        let mut orders_voided = 0usize;
        let mut cash_released: Cash = 0;
        let mut shares_released: Qty = 0;
        for oid in open {
            let order = self.orders[&oid].clone();
            let remaining = order.remaining();
            let removed = self
                .books
                .get_mut(market)
                .expect("market book")
                .get_mut(order.outcome)
                .remove(order.side == Side::Buy, order.price, oid);
            debug_assert!(removed, "open order must be resting in the book");

            let acct = self
                .accounts
                .get_mut(&order.account)
                .expect("order account exists");
            match order.side {
                Side::Buy => {
                    let release = order.price as Cash * remaining as Cash;
                    acct.locked_cash -= release;
                    cash_released += release;
                }
                Side::Sell => {
                    acct.positions
                        .entry(market.to_string())
                        .or_default()
                        .get_mut(order.outcome)
                        .locked -= remaining as i64;
                    shares_released += remaining;
                }
            }
            self.orders.get_mut(&oid).expect("order exists").status = OrderStatus::Voided;
            orders_voided += 1;
        }

        // 2. Price every remaining holding. `accounts` is a BTreeMap, so
        //    the report comes out in account-id order without a sort.
        let holdings: Vec<Holding> = self
            .accounts
            .values()
            .filter_map(|a| {
                a.positions.get(market).map(|mp| Holding {
                    account: a.id.clone(),
                    yes: mp.yes.quantity.max(0) as Qty,
                    no: mp.no.quantity.max(0) as Qty,
                })
            })
            .collect();
        let payouts = compute_payouts(&holdings, winner);
        let total_paid: Cash = payouts.iter().map(|p| p.paid).sum();
        let winning_shares: Qty = payouts.iter().map(|p| p.winning_shares).sum();
        let losing_shares: Qty = payouts.iter().map(|p| p.losing_shares).sum();

        // 3. Pay, then clear every position in this market. Both outcomes
        //    go, winning and losing alike: a settled share is spent.
        for payout in &payouts {
            self.accounts
                .get_mut(&payout.account)
                .expect("paid account exists")
                .balance += payout.paid;
        }
        for acct in self.accounts.values_mut() {
            if let Some(pos) = acct.positions.remove(market) {
                debug_assert!(
                    pos.yes.locked == 0 && pos.no.locked == 0,
                    "voiding must release every share lock before payout"
                );
            }
        }

        // 4. Draw on the market's collateral and name what was not funded.
        let collateral = self.collateral.remove(market).unwrap_or(0);
        let unbacked_cash = unbacked(total_paid, collateral);

        self.markets
            .get_mut(market)
            .expect("market exists")
            .resolution = Some(Resolution {
            outcome: winner,
            resolved_at: now_ms,
        });

        let settlement = Settlement {
            market: market.to_string(),
            outcome: winner,
            resolved_at: now_ms,
            payouts,
            total_paid,
            winning_shares,
            losing_shares,
            orders_voided,
            cash_released,
            shares_released,
            collateral,
            unbacked_cash,
        };
        self.settlements
            .insert(market.to_string(), settlement.clone());
        Ok(settlement)
    }

    // ---- queries ---------------------------------------------------------

    /// How a market settled, or `None` while it is still trading.
    pub fn resolution(&self, market: &str) -> Option<Resolution> {
        self.markets.get(market).and_then(|m| m.resolution)
    }

    /// The full settlement report of a resolved market.
    pub fn settlement(&self, market: &str) -> Option<&Settlement> {
        self.settlements.get(market)
    }

    /// Cents currently backing a market's outstanding shares. Emptied when
    /// the market resolves.
    pub fn collateral(&self, market: &str) -> Cash {
        self.collateral.get(market).copied().unwrap_or(0)
    }

    pub fn markets(&self) -> impl Iterator<Item = &Market> {
        self.markets.values()
    }

    pub fn market(&self, id: &str) -> Option<&Market> {
        self.markets.get(id)
    }

    pub fn order(&self, id: OrderId) -> Option<&Order> {
        self.orders.get(&id)
    }

    pub fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.get(id)
    }

    pub fn book_view(
        &self,
        market: &str,
        outcome: Outcome,
        depth: usize,
    ) -> Result<BookView, EngineError> {
        let books = self
            .books
            .get(market)
            .ok_or_else(|| EngineError::UnknownMarket(market.to_string()))?;
        let (bids, asks) = books
            .get(outcome)
            .levels(depth, |id| self.orders[&id].remaining());
        Ok(BookView {
            market: market.to_string(),
            outcome,
            bids,
            asks,
        })
    }

    /// Most recent trades first.
    pub fn recent_trades(&self, market: &str, limit: usize) -> Result<Vec<Trade>, EngineError> {
        let trades = self
            .trades
            .get(market)
            .ok_or_else(|| EngineError::UnknownMarket(market.to_string()))?;
        Ok(trades.iter().rev().take(limit).cloned().collect())
    }

    /// Best estimate of an outcome's price in cents: last trade if any,
    /// otherwise the midpoint of the book, otherwise None.
    pub fn price_estimate(&self, market: &str, outcome: Outcome) -> Option<Price> {
        if let Some(trades) = self.trades.get(market) {
            if let Some(t) = trades.iter().rev().find(|t| t.outcome == outcome) {
                return Some(t.price);
            }
        }
        let book = self.books.get(market)?.get(outcome);
        match (book.best_bid(), book.best_ask()) {
            (Some(b), Some(a)) => Some((b + a) / 2),
            (Some(b), None) => Some(b),
            (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }

    /// Total shares traded in a market across both outcomes.
    pub fn volume(&self, market: &str) -> Qty {
        self.trades
            .get(market)
            .map(|ts| ts.iter().map(|t| t.quantity).sum())
            .unwrap_or(0)
    }

    pub fn open_orders(&self, account: &str) -> Vec<Order> {
        let mut orders: Vec<Order> = self
            .orders
            .values()
            .filter(|o| o.account == account && o.status == OrderStatus::Open)
            .cloned()
            .collect();
        orders.sort_by_key(|o| o.seq);
        orders
    }

    // ---- snapshots -------------------------------------------------------

    pub fn to_snapshot(&self) -> String {
        serde_json::to_string(self).expect("exchange state serializes")
    }

    pub fn from_snapshot(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(taker_price: Price, seq: u64, kind: TradeKind) -> Candidate {
        Candidate {
            maker: seq,
            seq,
            taker_price,
            maker_price: match kind {
                TradeKind::Match => taker_price,
                _ => PAIR_CENTS - taker_price,
            },
            kind,
        }
    }

    #[test]
    fn a_buyer_prefers_the_cheaper_candidate_from_either_book() {
        let own = candidate(64, 1, TradeKind::Match);
        let other = candidate(62, 2, TradeKind::Mint);
        assert!(other.beats(&own, Side::Buy));
        assert!(!own.beats(&other, Side::Buy));
    }

    #[test]
    fn a_seller_prefers_the_dearer_candidate_from_either_book() {
        let own = candidate(60, 1, TradeKind::Match);
        let other = candidate(62, 2, TradeKind::Burn);
        assert!(other.beats(&own, Side::Sell));
        assert!(!own.beats(&other, Side::Sell));
    }

    #[test]
    fn equal_prices_are_broken_by_time_priority_across_the_books() {
        let older = candidate(62, 1, TradeKind::Match);
        let newer = candidate(62, 2, TradeKind::Mint);
        for side in [Side::Buy, Side::Sell] {
            assert!(older.beats(&newer, side));
            assert!(!newer.beats(&older, side));
        }
    }

    #[test]
    fn a_complementary_candidate_always_splits_a_whole_pair() {
        for p in 1..PAIR_CENTS {
            let c = candidate(p, 1, TradeKind::Mint);
            assert_eq!(c.taker_price + c.maker_price, PAIR_CENTS);
        }
    }

    #[test]
    fn an_outcome_is_its_own_complement_twice_over() {
        for o in [Outcome::Yes, Outcome::No] {
            assert_ne!(o, o.complement());
            assert_eq!(o, o.complement().complement());
        }
    }
}
