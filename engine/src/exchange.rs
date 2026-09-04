use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::book::Book;
use crate::error::EngineError;
use crate::types::{
    BookView, Cash, Market, Order, OrderId, OrderStatus, Outcome, PlaceResult, Price, Qty, Side,
    Trade, TradeId,
};

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
            },
        );
        self.books.insert(id.to_string(), MarketBooks::default());
        self.trades.insert(id.to_string(), Vec::new());
        Ok(())
    }

    /// Mint shares into an account. Used only for seeding demo liquidity;
    /// a real listing flow would mint YES/NO pairs against collateral.
    pub fn grant_shares(
        &mut self,
        account: &str,
        market: &str,
        outcome: Outcome,
        qty: Qty,
    ) -> Result<(), EngineError> {
        if !self.markets.contains_key(market) {
            return Err(EngineError::UnknownMarket(market.to_string()));
        }
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
        if !(1..=99).contains(&price) {
            return Err(EngineError::InvalidPrice(price));
        }
        if quantity == 0 {
            return Err(EngineError::InvalidQuantity);
        }
        if !self.markets.contains_key(market) {
            return Err(EngineError::UnknownMarket(market.to_string()));
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
            seq: self.next_seq,
            created_at: now_ms,
        };
        self.orders.insert(taker_id, order);

        let mut trades = Vec::new();

        // Match against the opposite side, best price first, FIFO per level.
        loop {
            let taker_remaining = self.orders[&taker_id].remaining();
            if taker_remaining == 0 {
                break;
            }
            let book = self.books[market].get(outcome);
            let (level_price, maker_id) = match side {
                Side::Buy => match book.best_ask() {
                    Some(best) if best <= price => {
                        (best, *book.asks[&best].front().expect("nonempty level"))
                    }
                    _ => break,
                },
                Side::Sell => match book.best_bid() {
                    Some(best) if best >= price => {
                        (best, *book.bids[&best].front().expect("nonempty level"))
                    }
                    _ => break,
                },
            };

            let maker_remaining = self.orders[&maker_id].remaining();
            let fill = taker_remaining.min(maker_remaining);
            let (buy_id, sell_id) = match side {
                Side::Buy => (taker_id, maker_id),
                Side::Sell => (maker_id, taker_id),
            };
            self.settle_fill(market, outcome, buy_id, sell_id, level_price, fill);

            for id in [taker_id, maker_id] {
                let o = self.orders.get_mut(&id).expect("order exists");
                o.filled += fill;
                if o.remaining() == 0 {
                    o.status = OrderStatus::Filled;
                }
            }
            if self.orders[&maker_id].remaining() == 0 {
                let maker_is_bid = side == Side::Sell;
                self.books
                    .get_mut(market)
                    .expect("market book")
                    .get_mut(outcome)
                    .remove(maker_is_bid, level_price, maker_id);
            }

            self.next_trade_id += 1;
            let trade = Trade {
                id: self.next_trade_id,
                market: market.to_string(),
                outcome,
                price: level_price,
                quantity: fill,
                taker_side: side,
                buyer: self.orders[&buy_id].account.clone(),
                seller: self.orders[&sell_id].account.clone(),
                buy_order: buy_id,
                sell_order: sell_id,
                ts: now_ms,
            };
            self.trades
                .get_mut(market)
                .expect("market trades")
                .push(trade.clone());
            trades.push(trade);
        }

        // Rest any remainder in the book at the limit price.
        let remaining = self.orders[&taker_id].remaining();
        if remaining > 0 {
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

        Ok(PlaceResult {
            order: self.orders[&taker_id].clone(),
            trades,
        })
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

    // ---- queries ---------------------------------------------------------

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
