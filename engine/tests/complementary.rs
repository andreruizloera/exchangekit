//! Complementary matching: the two books of a binary market trading
//! against each other.
//!
//! Buying NO at `q` is selling YES at `100 - q`, so a resting NO bid is an
//! offer of YES and a resting NO ask is a bid for YES. Two buyers on
//! opposite outcomes cross by minting the pair they are paying for; two
//! sellers cross by burning the pair they hold and releasing its
//! collateral. These tests cover which of the two books a taker picks, the
//! cash and share arithmetic on both sides, the collateral pool that funds
//! a burn, and redeeming a pair directly.

use exchangekit_engine::{Cash, EngineError, Exchange, Outcome, PlaceResult, Qty, Side, TradeKind};

const CASH: i64 = 1_000_000; // $10,000.00 in play-money cents
const T: u64 = 1_000; // arbitrary timestamp
const WHO: [&str; 4] = ["alice", "bob", "carol", "dave"];

/// One market whose every share was minted as a collateralized pair, so
/// the pool can fund any burn these tests ask for. Each account holds 500
/// YES and 500 NO; the market holds $2,000.00.
fn setup() -> Exchange {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    for who in WHO {
        ex.create_account(who, CASH).unwrap();
        ex.mint_pair(who, "m", 500).unwrap();
    }
    ex
}

/// A second market on the same exchange whose shares were granted rather
/// than minted, so it holds no collateral at all.
fn add_granted_market(ex: &mut Exchange) {
    ex.create_market("free", "Granted market?", "", T).unwrap();
    for who in WHO {
        ex.grant_shares(who, "free", Outcome::Yes, 100).unwrap();
        ex.grant_shares(who, "free", Outcome::No, 100).unwrap();
    }
}

fn order(
    ex: &mut Exchange,
    who: &str,
    market: &str,
    outcome: Outcome,
    side: Side,
    price: u32,
    qty: u64,
) -> PlaceResult {
    ex.place_order(who, market, outcome, side, price, qty, T)
        .unwrap()
}

fn buy_yes(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> PlaceResult {
    order(ex, who, "m", Outcome::Yes, Side::Buy, price, qty)
}

fn buy_no(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> PlaceResult {
    order(ex, who, "m", Outcome::No, Side::Buy, price, qty)
}

fn sell_yes(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> PlaceResult {
    order(ex, who, "m", Outcome::Yes, Side::Sell, price, qty)
}

fn sell_no(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> PlaceResult {
    order(ex, who, "m", Outcome::No, Side::Sell, price, qty)
}

fn held(ex: &Exchange, who: &str, market: &str, outcome: Outcome) -> (i64, i64) {
    ex.account(who)
        .unwrap()
        .positions
        .get(market)
        .map(|mp| {
            let p = mp.get(outcome);
            (p.quantity, p.locked)
        })
        .unwrap_or((0, 0))
}

fn total_cash(ex: &Exchange) -> Cash {
    WHO.iter().map(|w| ex.account(w).unwrap().balance).sum()
}

/// Cash in accounts plus cents held as collateral. Minting moves cash into
/// the pool and burning moves it back out, so this total is what has to
/// stay put across a session of complementary matching.
fn cash_and_collateral(ex: &Exchange) -> Cash {
    total_cash(ex) + ex.collateral("m") + ex.collateral("free")
}

fn outstanding(ex: &Exchange, market: &str, outcome: Outcome) -> i64 {
    WHO.iter().map(|w| held(ex, w, market, outcome).0).sum()
}

// ---- minting: two buyers cross -------------------------------------------

#[test]
fn a_yes_bid_and_a_no_bid_at_the_complement_mint_a_pair() {
    let mut ex = setup();
    let bid = buy_yes(&mut ex, "alice", 63, 20);
    assert!(bid.trades.is_empty(), "nothing to trade with yet");

    let res = buy_no(&mut ex, "bob", 37, 20);
    assert_eq!(res.trades.len(), 1);
    let t = &res.trades[0];
    assert_eq!(t.kind, TradeKind::Mint);
    assert_eq!(t.quantity, 20);
    assert_eq!(t.outcome, Outcome::No, "the trade is priced from the taker");
    assert_eq!(t.price, 37);
    assert_eq!(res.order.filled, 20);

    // Each side got the outcome it bid for, and only that one.
    assert_eq!(held(&ex, "alice", "m", Outcome::Yes), (520, 0));
    assert_eq!(held(&ex, "alice", "m", Outcome::No), (500, 0));
    assert_eq!(held(&ex, "bob", "m", Outcome::No), (520, 0));
    assert_eq!(held(&ex, "bob", "m", Outcome::Yes), (500, 0));
    // Neither book has anything left resting.
    assert!(ex.book_view("m", Outcome::Yes, 10).unwrap().bids.is_empty());
    assert!(ex.book_view("m", Outcome::No, 10).unwrap().bids.is_empty());
}

#[test]
fn the_two_buyers_pay_exactly_one_dollar_per_pair_between_them() {
    let mut ex = setup();
    let cash_before = total_cash(&ex);
    let pool_before = ex.collateral("m");

    buy_yes(&mut ex, "alice", 63, 20);
    let alice_before = ex.account("alice").unwrap().balance;
    buy_no(&mut ex, "bob", 37, 20);

    assert_eq!(
        ex.account("alice").unwrap().balance,
        alice_before - 63 * 20,
        "the resting bid paid its own price"
    );
    assert_eq!(total_cash(&ex), cash_before - 100 * 20);
    assert_eq!(
        ex.collateral("m"),
        pool_before + 100 * 20,
        "every cent the two of them paid went into the pool"
    );
    assert_eq!(
        cash_and_collateral(&ex),
        cash_before + pool_before,
        "a mint moves cash, it does not create it"
    );
}

#[test]
fn a_mint_taker_gets_price_improvement_and_its_escrow_back() {
    let mut ex = setup();
    buy_yes(&mut ex, "alice", 63, 10); // offers NO at 37
    let before = ex.account("bob").unwrap().balance;

    // Bob would pay 45 but the resting bid only asks 37 of him.
    let res = buy_no(&mut ex, "bob", 45, 10);
    assert_eq!(res.trades[0].price, 37);
    let bob = ex.account("bob").unwrap();
    assert_eq!(bob.balance, before - 370);
    assert_eq!(bob.locked_cash, 0, "the 8 cents a share it did not spend");
}

#[test]
fn complementary_bids_that_do_not_add_up_to_a_dollar_both_rest() {
    let mut ex = setup();
    buy_yes(&mut ex, "alice", 60, 10);
    let res = buy_no(&mut ex, "bob", 39, 10); // 60 + 39 = 99
    assert!(
        res.trades.is_empty(),
        "99 cents does not pay for a 100 cent pair"
    );
    assert_eq!(
        ex.book_view("m", Outcome::Yes, 10).unwrap().bids[0].price,
        60
    );
    assert_eq!(
        ex.book_view("m", Outcome::No, 10).unwrap().bids[0].price,
        39
    );
    assert_eq!(ex.collateral("m"), 200_000, "nothing was minted");
}

#[test]
fn a_complementary_fill_can_be_partial_and_the_rest_still_rests() {
    let mut ex = setup();
    buy_yes(&mut ex, "alice", 63, 8);
    let res = buy_no(&mut ex, "bob", 37, 20);
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].quantity, 8);
    assert_eq!(res.order.filled, 8);
    // The 12 bob could not mint rests in his own book at his limit.
    let no_book = ex.book_view("m", Outcome::No, 10).unwrap();
    assert_eq!(no_book.bids[0].price, 37);
    assert_eq!(no_book.bids[0].quantity, 12);
    assert!(ex.book_view("m", Outcome::Yes, 10).unwrap().bids.is_empty());
}

// ---- burning: two sellers cross ------------------------------------------

#[test]
fn two_asks_that_add_up_to_less_than_a_dollar_burn_a_pair() {
    let mut ex = setup();
    let pool_before = ex.collateral("m");
    let cash_before = total_cash(&ex);
    sell_yes(&mut ex, "alice", 62, 20);

    let res = sell_no(&mut ex, "bob", 30, 20);
    assert_eq!(res.trades.len(), 1);
    let t = &res.trades[0];
    assert_eq!(t.kind, TradeKind::Burn);
    assert_eq!(t.outcome, Outcome::No);
    assert_eq!(t.price, 38, "bob asked 30 and the resting ask paid him 38");
    assert_eq!(t.quantity, 20);

    // Both sides gave up shares and were paid out of the pool.
    assert_eq!(held(&ex, "alice", "m", Outcome::Yes), (480, 0));
    assert_eq!(held(&ex, "bob", "m", Outcome::No), (480, 0));
    assert_eq!(ex.collateral("m"), pool_before - 100 * 20);
    assert_eq!(total_cash(&ex), cash_before + 100 * 20);
    assert_eq!(cash_and_collateral(&ex), cash_before + pool_before);
}

#[test]
fn a_burn_releases_the_share_escrow_on_both_sides() {
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 62, 20);
    assert_eq!(
        held(&ex, "alice", "m", Outcome::Yes),
        (500, 20),
        "a resting sell locks the shares it offers"
    );
    sell_no(&mut ex, "bob", 30, 20);
    // The lock is released in the same step the share is destroyed, so
    // neither side is left holding a lock on a share it no longer owns.
    assert_eq!(held(&ex, "alice", "m", Outcome::Yes), (480, 0));
    assert_eq!(held(&ex, "bob", "m", Outcome::No), (480, 0));
    assert!(ex.open_orders("alice").is_empty());
}

#[test]
fn complementary_asks_that_add_up_to_more_than_a_dollar_both_rest() {
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 70, 10);
    let res = sell_no(&mut ex, "bob", 35, 10); // 70 + 35 = 105
    assert!(
        res.trades.is_empty(),
        "a pair only fetches 100 cents; nobody pays 105 for one"
    );
    assert_eq!(
        ex.book_view("m", Outcome::Yes, 10).unwrap().asks[0].price,
        70
    );
    assert_eq!(
        ex.book_view("m", Outcome::No, 10).unwrap().asks[0].price,
        35
    );
}

#[test]
fn a_burn_is_not_offered_when_the_market_holds_no_collateral() {
    let mut ex = setup();
    add_granted_market(&mut ex);
    assert_eq!(ex.collateral("free"), 0);

    order(&mut ex, "alice", "free", Outcome::Yes, Side::Sell, 40, 10);
    let res = order(&mut ex, "bob", "free", Outcome::No, Side::Sell, 40, 10);
    assert!(
        res.trades.is_empty(),
        "40 and 40 would cross, but the market has no dollar to release"
    );
    assert_eq!(res.order.status, exchangekit_engine::OrderStatus::Open);
    assert_eq!(ex.collateral("free"), 0, "the pool never goes negative");
}

#[test]
fn a_burn_fills_only_as_many_pairs_as_the_collateral_pool_covers() {
    let mut ex = setup();
    add_granted_market(&mut ex);
    ex.mint_pair("carol", "free", 3).unwrap(); // the pool now holds $3.00

    order(&mut ex, "alice", "free", Outcome::Yes, Side::Sell, 40, 10);
    let res = order(&mut ex, "bob", "free", Outcome::No, Side::Sell, 40, 10);
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].quantity, 3, "three dollars, three pairs");
    assert_eq!(res.order.filled, 3);
    assert_eq!(res.order.status, exchangekit_engine::OrderStatus::Open);
    assert_eq!(ex.collateral("free"), 0);
    // The remainder rests instead of spinning on an unfundable candidate.
    let no_book = ex.book_view("free", Outcome::No, 10).unwrap();
    assert_eq!(no_book.asks[0].quantity, 7);
    let yes_book = ex.book_view("free", Outcome::Yes, 10).unwrap();
    assert_eq!(yes_book.asks[0].quantity, 7);
}

// ---- priority across the two books ---------------------------------------

#[test]
fn a_buyer_takes_the_cheaper_of_the_two_books_first() {
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 64, 10); // YES offered at 64
    buy_no(&mut ex, "bob", 38, 10); // YES offered at 62

    let res = buy_yes(&mut ex, "dave", 70, 15);
    let seen: Vec<(u32, TradeKind, &str)> = res
        .trades
        .iter()
        .map(|t| (t.price, t.kind, t.seller.as_str()))
        .collect();
    assert_eq!(
        seen,
        vec![
            (62, TradeKind::Mint, "bob"),
            (64, TradeKind::Match, "alice")
        ],
        "62 from the complementary book beats 64 from its own"
    );
    assert_eq!(res.trades[1].quantity, 5);
}

#[test]
fn a_seller_takes_the_dearer_of_the_two_books_first() {
    let mut ex = setup();
    buy_yes(&mut ex, "alice", 60, 10); // bids 60 for YES
    sell_no(&mut ex, "bob", 38, 10); // bids 62 for YES

    let res = sell_yes(&mut ex, "dave", 50, 15);
    let seen: Vec<(u32, TradeKind, &str)> = res
        .trades
        .iter()
        .map(|t| (t.price, t.kind, t.buyer.as_str()))
        .collect();
    assert_eq!(
        seen,
        vec![
            (62, TradeKind::Burn, "bob"),
            (60, TradeKind::Match, "alice")
        ],
        "62 from the complementary book beats 60 from its own"
    );
}

#[test]
fn equal_prices_across_the_books_go_to_the_older_order() {
    // Same-book ask first: it keeps priority over the later NO bid.
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 62, 10);
    buy_no(&mut ex, "bob", 38, 10);
    let res = buy_yes(&mut ex, "dave", 62, 10);
    assert_eq!(res.trades[0].kind, TradeKind::Match);
    assert_eq!(res.trades[0].seller, "alice");

    // Complementary bid first: now it is the older order and it wins.
    let mut ex = setup();
    buy_no(&mut ex, "bob", 38, 10);
    sell_yes(&mut ex, "alice", 62, 10);
    let res = buy_yes(&mut ex, "dave", 62, 10);
    assert_eq!(res.trades[0].kind, TradeKind::Mint);
    assert_eq!(res.trades[0].seller, "bob");
}

#[test]
fn a_taker_walks_from_one_book_into_the_other_and_back() {
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 60, 10); // YES at 60
    buy_no(&mut ex, "bob", 39, 10); // YES at 61
    sell_yes(&mut ex, "carol", 62, 10); // YES at 62

    let res = buy_yes(&mut ex, "dave", 62, 25);
    let seen: Vec<(u32, u64, TradeKind)> = res
        .trades
        .iter()
        .map(|t| (t.price, t.quantity, t.kind))
        .collect();
    assert_eq!(
        seen,
        vec![
            (60, 10, TradeKind::Match),
            (61, 10, TradeKind::Mint),
            (62, 5, TradeKind::Match),
        ]
    );
    assert_eq!(res.order.status, exchangekit_engine::OrderStatus::Filled);
}

// ---- what the trade record says ------------------------------------------

#[test]
fn a_mint_trade_names_both_buyers_and_says_where_the_shares_came_from() {
    let mut ex = setup();
    let maker = buy_no(&mut ex, "bob", 37, 20).order.id;
    let res = buy_yes(&mut ex, "alice", 70, 20);
    let t = &res.trades[0];

    assert_eq!(t.kind, TradeKind::Mint);
    assert_eq!(t.taker_side, Side::Buy);
    assert_eq!(t.outcome, Outcome::Yes);
    assert_eq!(t.price, 63, "100 minus bob's 37");
    assert_eq!(t.buyer, "alice");
    assert_eq!(
        t.seller, "bob",
        "bidding 37 for NO is offering YES at 63, which is what he did"
    );
    assert_eq!(t.buy_order, res.order.id);
    assert_eq!(t.sell_order, maker);

    // The order the trade calls the sell side is a NO buy. That is the
    // whole point of the kind field: the trade is real, the delivery was a
    // mint, and bob never held a YES share.
    let seller_order = ex.order(t.sell_order).unwrap();
    assert_eq!(seller_order.side, Side::Buy);
    assert_eq!(seller_order.outcome, Outcome::No);
    assert_eq!(held(&ex, "bob", "m", Outcome::Yes).0, 500);
}

#[test]
fn a_burn_trade_names_both_sellers_and_says_where_the_shares_went() {
    let mut ex = setup();
    let maker = sell_no(&mut ex, "bob", 30, 20).order.id;
    let res = sell_yes(&mut ex, "alice", 50, 20);
    let t = &res.trades[0];

    assert_eq!(t.kind, TradeKind::Burn);
    assert_eq!(t.taker_side, Side::Sell);
    assert_eq!(t.price, 70, "100 minus bob's 30");
    assert_eq!(t.seller, "alice");
    assert_eq!(t.buyer, "bob", "offering NO at 30 is bidding 70 for YES");
    assert_eq!(t.sell_order, res.order.id);
    assert_eq!(t.buy_order, maker);

    let buyer_order = ex.order(t.buy_order).unwrap();
    assert_eq!(buyer_order.side, Side::Sell);
    assert_eq!(buyer_order.outcome, Outcome::No);
    assert_eq!(held(&ex, "bob", "m", Outcome::Yes).0, 500);
}

#[test]
fn an_ordinary_cross_is_still_reported_as_a_match() {
    let mut ex = setup();
    sell_yes(&mut ex, "alice", 60, 10);
    let res = buy_yes(&mut ex, "bob", 60, 10);
    assert_eq!(res.trades[0].kind, TradeKind::Match);
    assert_eq!(ex.collateral("m"), 200_000, "a match mints nothing");
}

#[test]
fn trade_kind_serializes_as_snake_case_and_defaults_to_match() {
    let mut ex = setup();
    buy_yes(&mut ex, "alice", 63, 5);
    let res = buy_no(&mut ex, "bob", 37, 5);
    let json = serde_json::to_value(&res.trades[0]).unwrap();
    assert_eq!(json["kind"], "mint");

    // A snapshot written before complementary matching has no kind key.
    let mut snap: serde_json::Value = serde_json::from_str(&ex.to_snapshot()).unwrap();
    for list in snap["trades"].as_object_mut().unwrap().values_mut() {
        for trade in list.as_array_mut().unwrap() {
            trade.as_object_mut().unwrap().remove("kind");
        }
    }
    let old = Exchange::from_snapshot(&snap.to_string()).unwrap();
    assert_eq!(
        old.recent_trades("m", 10).unwrap()[0].kind,
        TradeKind::Match,
        "an old trade reads as an ordinary cross"
    );
}

// ---- odd prices and rounding ---------------------------------------------

#[test]
fn every_price_splits_a_pair_with_nothing_left_to_round() {
    // Prices are whole cents and a pair is 100 of them, so both halves of
    // a complementary cross are exact at any price in the range.
    for p in [1u32, 2, 33, 49, 50, 51, 67, 98, 99] {
        let mut ex = setup();
        let before = cash_and_collateral(&ex);
        buy_yes(&mut ex, "alice", p, 7);
        let res = buy_no(&mut ex, "bob", 100 - p, 7);
        assert_eq!(res.trades.len(), 1, "p = {p} must cross");
        assert_eq!(res.trades[0].price, 100 - p);
        assert_eq!(ex.collateral("m"), 200_000 + 700);
        assert_eq!(cash_and_collateral(&ex), before, "exact at p = {p}");
    }
}

// ---- redeeming a pair directly -------------------------------------------

#[test]
fn redeeming_a_pair_returns_a_dollar_and_gives_up_both_outcomes() {
    let mut ex = setup();
    let before = ex.account("alice").unwrap().balance;
    ex.redeem_pair("alice", "m", 25).unwrap();
    assert_eq!(ex.account("alice").unwrap().balance, before + 2_500);
    assert_eq!(held(&ex, "alice", "m", Outcome::Yes), (475, 0));
    assert_eq!(held(&ex, "alice", "m", Outcome::No), (475, 0));
    assert_eq!(ex.collateral("m"), 200_000 - 2_500);
}

#[test]
fn minting_and_redeeming_the_same_pairs_is_a_round_trip() {
    let mut ex = setup();
    let before = ex.account("alice").unwrap().balance;
    let pool = ex.collateral("m");
    ex.mint_pair("alice", "m", 40).unwrap();
    ex.redeem_pair("alice", "m", 40).unwrap();
    assert_eq!(ex.account("alice").unwrap().balance, before);
    assert_eq!(ex.collateral("m"), pool);
    assert_eq!(held(&ex, "alice", "m", Outcome::Yes), (500, 0));
}

#[test]
fn redeeming_needs_both_outcomes_free_of_resting_sells() {
    let mut ex = setup();
    // Alice holds 500 of each but has offered 480 NO for sale.
    sell_no(&mut ex, "alice", 90, 480);
    let err = ex.redeem_pair("alice", "m", 100).unwrap_err();
    assert_eq!(
        err,
        EngineError::InsufficientPosition {
            need: 100,
            available: 20
        },
        "a share promised to a buyer cannot also be redeemed"
    );
    assert_eq!(ex.redeem_pair("alice", "m", 20), Ok(()));
}

#[test]
fn redeeming_needs_one_side_to_be_short_of_neither_outcome() {
    let mut ex = setup();
    sell_yes(&mut ex, "bob", 40, 500); // bob sells all his YES
    buy_yes(&mut ex, "alice", 40, 500);
    assert_eq!(held(&ex, "bob", "m", Outcome::Yes).0, 0);
    assert!(matches!(
        ex.redeem_pair("bob", "m", 1),
        Err(EngineError::InsufficientPosition { .. })
    ));
    assert_eq!(ex.redeem_pair("alice", "m", 500), Ok(()));
}

#[test]
fn redeeming_more_than_the_market_holds_is_refused() {
    let mut ex = setup();
    add_granted_market(&mut ex);
    let err = ex.redeem_pair("alice", "free", 1).unwrap_err();
    assert_eq!(
        err,
        EngineError::InsufficientCollateral {
            need: 100,
            available: 0
        },
        "granted shares never paid anything in, so there is nothing to pay out"
    );
    assert_eq!(held(&ex, "alice", "free", Outcome::Yes), (100, 0));
}

#[test]
fn redeeming_rejects_zero_and_unknown_names_and_closed_markets() {
    let mut ex = setup();
    assert_eq!(
        ex.redeem_pair("alice", "m", 0).unwrap_err(),
        EngineError::InvalidQuantity
    );
    assert!(matches!(
        ex.redeem_pair("nobody", "m", 1),
        Err(EngineError::UnknownAccount(_))
    ));
    assert!(matches!(
        ex.redeem_pair("alice", "nope", 1),
        Err(EngineError::UnknownMarket(_))
    ));
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(
        ex.redeem_pair("alice", "m", 1).unwrap_err(),
        EngineError::MarketResolved("m".to_string())
    );
}

// ---- the invariants ------------------------------------------------------

/// The complementary-matching counterpart of the conservation test in
/// tests/matching.rs. Ordinary trading conserves cash on its own because a
/// buyer's cents become a seller's. A mint and a burn move cash between
/// accounts and the collateral pool instead, so the total that has to stay
/// put is cash plus collateral.
#[test]
fn cash_plus_collateral_is_conserved_by_a_session_of_complementary_matching() {
    let mut ex = setup();
    let start = cash_and_collateral(&ex);
    let start_pairs = outstanding(&ex, "m", Outcome::Yes);

    buy_yes(&mut ex, "alice", 63, 40);
    buy_no(&mut ex, "bob", 37, 40); // mints 40 pairs
    sell_yes(&mut ex, "carol", 58, 30);
    buy_yes(&mut ex, "dave", 58, 30); // an ordinary match
    sell_no(&mut ex, "dave", 30, 25);
    sell_yes(&mut ex, "alice", 55, 25); // burns 25 pairs
    ex.redeem_pair("bob", "m", 10).unwrap();
    ex.mint_pair("carol", "m", 15).unwrap();
    buy_yes(&mut ex, "carol", 70, 12);
    buy_no(&mut ex, "dave", 45, 12); // mints 12 more

    assert_eq!(
        cash_and_collateral(&ex),
        start,
        "no cent was created or destroyed"
    );
    // Every share still exists in pairs: the mints and burns moved both
    // outcomes together, and the match moved neither total.
    let net = 40 - 25 - 10 + 15 + 12;
    assert_eq!(outstanding(&ex, "m", Outcome::Yes), start_pairs + net);
    assert_eq!(outstanding(&ex, "m", Outcome::No), start_pairs + net);
    assert_eq!(
        ex.collateral("m"),
        (start_pairs + net) * 100,
        "the pool holds a dollar for every outstanding pair"
    );
}

#[test]
fn a_market_traded_through_pairs_still_settles_with_no_unbacked_cash() {
    let mut ex = setup();
    // Cash plus collateral before, because at this point the pool is
    // holding cents the accounts have already paid in.
    let start = cash_and_collateral(&ex);

    buy_yes(&mut ex, "alice", 63, 40);
    buy_no(&mut ex, "bob", 37, 40);
    sell_no(&mut ex, "dave", 30, 25);
    sell_yes(&mut ex, "alice", 55, 25);
    ex.redeem_pair("bob", "m", 10).unwrap();
    // Leave some resting on both books so voiding has work to do.
    buy_yes(&mut ex, "carol", 20, 50);
    sell_no(&mut ex, "carol", 95, 50);

    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(
        s.unbacked_cash, 0,
        "every share outstanding was paid for when it was minted"
    );
    assert_eq!(s.total_paid, s.collateral);
    assert_eq!(ex.collateral("m"), 0, "the pool was spent");
    assert_eq!(
        total_cash(&ex),
        start,
        "the pool paid back exactly what the accounts had put into it"
    );
}

/// Burning is neutral on the honesty number. A market holding granted
/// shares settles with unbacked cash whether or not any pairs were burned
/// first, because a burn takes one winning share out of circulation for
/// each dollar it removes from the pool.
#[test]
fn burning_does_not_change_how_much_cash_a_settlement_creates() {
    let unbacked_after = |burn: bool| -> Cash {
        let mut ex = Exchange::new();
        ex.create_market("m", "Test market?", "", T).unwrap();
        for who in WHO {
            ex.create_account(who, CASH).unwrap();
        }
        ex.mint_pair("alice", "m", 30).unwrap();
        ex.grant_shares("bob", "m", Outcome::Yes, 25).unwrap();
        ex.grant_shares("bob", "m", Outcome::No, 25).unwrap();
        if burn {
            sell_yes(&mut ex, "alice", 55, 20);
            let res = sell_no(&mut ex, "bob", 40, 20);
            assert_eq!(res.trades[0].kind, TradeKind::Burn);
        }
        ex.resolve_market("m", Outcome::Yes, T)
            .unwrap()
            .unbacked_cash
    };
    // Bob's 25 granted YES shares are the whole shortfall either way.
    assert_eq!(unbacked_after(false), 2_500);
    assert_eq!(unbacked_after(true), 2_500);
}

#[test]
fn no_account_can_end_up_with_a_negative_share_count() {
    let mut ex = setup();
    // Hammer both directions with overlapping sizes and prices.
    let plan: [(&str, Outcome, Side, u32, Qty); 10] = [
        ("alice", Outcome::Yes, Side::Buy, 63, 40),
        ("bob", Outcome::No, Side::Buy, 37, 55),
        ("carol", Outcome::Yes, Side::Buy, 65, 30),
        ("dave", Outcome::No, Side::Sell, 33, 45),
        ("alice", Outcome::Yes, Side::Sell, 60, 60),
        ("bob", Outcome::No, Side::Sell, 25, 20),
        ("carol", Outcome::Yes, Side::Sell, 70, 35),
        ("dave", Outcome::Yes, Side::Buy, 71, 50),
        ("alice", Outcome::No, Side::Buy, 30, 40),
        ("bob", Outcome::Yes, Side::Buy, 72, 25),
    ];
    for (who, outcome, side, price, qty) in plan {
        order(&mut ex, who, "m", outcome, side, price, qty);
    }
    for who in WHO {
        for outcome in [Outcome::Yes, Outcome::No] {
            let (qty, locked) = held(&ex, who, "m", outcome);
            assert!(qty >= 0, "{who} went short {outcome:?}");
            assert!(locked >= 0 && locked <= qty, "{who} escrow is inconsistent");
        }
        let acct = ex.account(who).unwrap();
        assert!(acct.locked_cash >= 0 && acct.locked_cash <= acct.balance);
    }
    assert!(ex.collateral("m") >= 0);
    assert_eq!(
        ex.collateral("m") / 100,
        outstanding(&ex, "m", Outcome::Yes),
        "the pool still matches the pairs outstanding"
    );
}
