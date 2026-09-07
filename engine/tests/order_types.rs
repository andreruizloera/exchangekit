//! Order types beyond the plain limit order: immediate-or-cancel,
//! fill-or-kill, and post-only.
//!
//! All three are decided against both books, because in a binary market a
//! bid on one outcome is an offer of the other. A post-only order that
//! does not cross its own book can still be taking, and a fill-or-kill
//! counts complementary depth toward the quantity it needs, capped by the
//! pairs the collateral pool could afford to burn.

use exchangekit_engine::{
    Exchange, OrderRequest, OrderStatus, Outcome, PlaceResult, Price, Qty, Side, TimeInForce,
    TradeKind,
};

const CASH: i64 = 1_000_000; // $10,000.00 in play-money cents
const T: u64 = 1_000; // arbitrary timestamp
const WHO: [&str; 3] = ["alice", "bob", "carol"];

/// One market holding 300 pairs of collateral, with 100 YES and 100 NO in
/// each account, so both a same-book cross and a burn are possible.
fn setup() -> Exchange {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    for who in WHO {
        ex.create_account(who, CASH).unwrap();
        ex.mint_pair(who, "m", 100).unwrap();
    }
    ex
}

fn place(
    ex: &mut Exchange,
    who: &str,
    outcome: Outcome,
    side: Side,
    price: Price,
    qty: Qty,
    tif: TimeInForce,
) -> PlaceResult {
    ex.place(OrderRequest::limit(who, "m", outcome, side, price, qty, T).tif(tif))
        .unwrap()
}

fn rest_yes_ask(ex: &mut Exchange, who: &str, price: Price, qty: Qty) {
    let res = place(
        ex,
        who,
        Outcome::Yes,
        Side::Sell,
        price,
        qty,
        TimeInForce::default(),
    );
    assert!(res.trades.is_empty(), "the fixture must not trade");
}

fn rest_yes_bid(ex: &mut Exchange, who: &str, price: Price, qty: Qty) {
    let res = place(
        ex,
        who,
        Outcome::Yes,
        Side::Buy,
        price,
        qty,
        TimeInForce::default(),
    );
    assert!(res.trades.is_empty(), "the fixture must not trade");
}

/// One side of a book as (price, quantity) pairs, best price first.
type BookSide = Vec<(Price, Qty)>;

fn levels(ex: &Exchange, outcome: Outcome) -> (BookSide, BookSide) {
    let b = ex.book_view("m", outcome, 20).unwrap();
    (
        b.bids.iter().map(|l| (l.price, l.quantity)).collect(),
        b.asks.iter().map(|l| (l.price, l.quantity)).collect(),
    )
}

fn locked(ex: &Exchange, who: &str, outcome: Outcome) -> (i64, i64) {
    let a = ex.account(who).unwrap();
    (a.locked_cash, a.positions["m"].get(outcome).locked)
}

// ---- the default has not changed -----------------------------------------

#[test]
fn place_order_is_still_an_ordinary_limit_order() {
    let mut ex = setup();
    let res = ex
        .place_order("alice", "m", Outcome::Yes, Side::Buy, 60, 10, T)
        .unwrap();
    assert_eq!(res.order.time_in_force, TimeInForce::GoodTillCancelled);
    assert_eq!(res.order.status, OrderStatus::Open);
    assert_eq!(levels(&ex, Outcome::Yes).0, vec![(60, 10)]);
}

// ---- immediate or cancel -------------------------------------------------

#[test]
fn an_ioc_fills_what_it_can_and_cancels_the_rest() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 4);

    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.order.filled, 4);
    assert_eq!(
        res.order.status,
        OrderStatus::Cancelled,
        "the remainder was cancelled, which is what the policy says"
    );
    assert!(
        levels(&ex, Outcome::Yes).0.is_empty(),
        "an immediate-or-cancel order never rests"
    );
    assert!(ex.open_orders("bob").is_empty());
}

#[test]
fn an_ioc_remainder_releases_its_cash_escrow() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 4);
    let before = ex.account("bob").unwrap().balance;

    place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    let bob = ex.account("bob").unwrap();
    assert_eq!(bob.balance, before - 240, "paid for the 4 it got");
    assert_eq!(bob.locked_cash, 0, "and holds nothing against the other 6");
}

#[test]
fn an_ioc_remainder_releases_its_share_escrow() {
    let mut ex = setup();
    rest_yes_bid(&mut ex, "alice", 60, 4);

    place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Sell,
        60,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    assert_eq!(locked(&ex, "bob", Outcome::Yes), (0, 0));
    assert_eq!(
        ex.account("bob").unwrap().positions["m"]
            .get(Outcome::Yes)
            .quantity,
        96
    );
}

#[test]
fn an_ioc_that_fills_completely_is_simply_filled() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 10);
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    assert_eq!(res.order.status, OrderStatus::Filled);
    assert_eq!(res.order.filled, 10);
}

#[test]
fn an_ioc_that_crosses_nothing_leaves_no_trace() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 70, 10);
    let before = ex.account("bob").unwrap().balance;

    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    assert!(res.trades.is_empty());
    assert_eq!(res.order.status, OrderStatus::Cancelled);
    assert!(levels(&ex, Outcome::Yes).0.is_empty());
    let bob = ex.account("bob").unwrap();
    assert_eq!((bob.balance, bob.locked_cash), (before, 0));
}

#[test]
fn an_ioc_takes_the_complementary_book_like_any_other_order() {
    let mut ex = setup();
    // A NO bid at 38 is an offer of YES at 62.
    place(
        &mut ex,
        "alice",
        Outcome::No,
        Side::Buy,
        38,
        6,
        TimeInForce::default(),
    );
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        70,
        10,
        TimeInForce::ImmediateOrCancel,
    );
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].kind, TradeKind::Mint);
    assert_eq!(res.trades[0].price, 62);
    assert_eq!(res.order.status, OrderStatus::Cancelled);
    assert_eq!(ex.account("bob").unwrap().locked_cash, 0);
}

// ---- fill or kill --------------------------------------------------------

#[test]
fn a_fill_or_kill_the_books_can_cover_fills_in_full() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 6);
    rest_yes_ask(&mut ex, "bob", 61, 6);

    let res = place(
        &mut ex,
        "carol",
        Outcome::Yes,
        Side::Buy,
        61,
        10,
        TimeInForce::FillOrKill,
    );
    assert_eq!(res.order.status, OrderStatus::Filled);
    assert_eq!(res.order.filled, 10);
    assert_eq!(res.trades.len(), 2);
    assert_eq!(levels(&ex, Outcome::Yes).1, vec![(61, 2)]);
}

#[test]
fn a_fill_or_kill_the_books_cannot_cover_is_rejected_and_moves_nothing() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 9);
    let cash_before = ex.account("carol").unwrap().balance;

    let res = place(
        &mut ex,
        "carol",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::FillOrKill,
    );
    assert_eq!(res.order.status, OrderStatus::Rejected);
    assert!(res.trades.is_empty(), "nine of ten is not a fill");
    assert_eq!(res.order.filled, 0);
    // The resting ask is untouched and so is the taker's cash.
    assert_eq!(levels(&ex, Outcome::Yes).1, vec![(60, 9)]);
    let carol = ex.account("carol").unwrap();
    assert_eq!((carol.balance, carol.locked_cash), (cash_before, 0));
    assert!(ex.open_orders("carol").is_empty());
}

#[test]
fn a_fill_or_kill_counts_both_books_toward_the_quantity_it_needs() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 62, 6); // 6 from this book
    place(
        &mut ex,
        "bob",
        Outcome::No,
        Side::Buy,
        38,
        4,
        TimeInForce::default(),
    ); // 4 more, as YES at 62

    let res = place(
        &mut ex,
        "carol",
        Outcome::Yes,
        Side::Buy,
        62,
        10,
        TimeInForce::FillOrKill,
    );
    assert_eq!(res.order.status, OrderStatus::Filled);
    let kinds: Vec<TradeKind> = res.trades.iter().map(|t| t.kind).collect();
    assert!(
        kinds.contains(&TradeKind::Match) && kinds.contains(&TradeKind::Mint),
        "it needed both books to fill: {kinds:?}"
    );
    assert_eq!(res.trades.iter().map(|t| t.quantity).sum::<Qty>(), 10);
}

#[test]
fn a_fill_or_kill_ignores_depth_beyond_its_limit_price() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 6);
    rest_yes_ask(&mut ex, "bob", 65, 20);

    let res = place(
        &mut ex,
        "carol",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::FillOrKill,
    );
    assert_eq!(
        res.order.status,
        OrderStatus::Rejected,
        "the 65 ask is deep but it is not for sale at 60"
    );
    assert_eq!(levels(&ex, Outcome::Yes).1, vec![(60, 6), (65, 20)]);
}

/// A complementary sell can only fill as far as the collateral pool covers,
/// so a fill-or-kill has to count the pool, not just the depth.
#[test]
fn a_fill_or_kill_sell_is_capped_by_the_collateral_pool() {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    for who in WHO {
        ex.create_account(who, CASH).unwrap();
        ex.grant_shares(who, "m", Outcome::Yes, 100).unwrap();
        ex.grant_shares(who, "m", Outcome::No, 100).unwrap();
    }
    ex.mint_pair("carol", "m", 4).unwrap(); // the pool holds $4.00

    // 10 NO offered at 38 is a bid of 62 for YES, but only 4 can be burned.
    place(
        &mut ex,
        "alice",
        Outcome::No,
        Side::Sell,
        38,
        10,
        TimeInForce::default(),
    );
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Sell,
        50,
        10,
        TimeInForce::FillOrKill,
    );
    assert_eq!(res.order.status, OrderStatus::Rejected);
    assert_eq!(ex.collateral("m"), 400, "the pool was not touched");

    // Four is exactly what the pool covers, so four fills.
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Sell,
        50,
        4,
        TimeInForce::FillOrKill,
    );
    assert_eq!(res.order.status, OrderStatus::Filled);
    assert_eq!(res.trades[0].kind, TradeKind::Burn);
    assert_eq!(ex.collateral("m"), 0);
}

// ---- post only -----------------------------------------------------------

#[test]
fn a_post_only_order_that_does_not_cross_rests_in_full() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 65, 10);
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::PostOnly,
    );
    assert_eq!(res.order.status, OrderStatus::Open);
    assert!(res.trades.is_empty());
    assert_eq!(levels(&ex, Outcome::Yes).0, vec![(60, 10)]);
    assert_eq!(ex.account("bob").unwrap().locked_cash, 600);
}

#[test]
fn a_post_only_order_that_would_take_is_rejected() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 10);
    let before = ex.account("bob").unwrap().balance;

    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::PostOnly,
    );
    assert_eq!(res.order.status, OrderStatus::Rejected);
    assert!(res.trades.is_empty());
    assert_eq!(levels(&ex, Outcome::Yes).1, vec![(60, 10)], "ask untouched");
    let bob = ex.account("bob").unwrap();
    assert_eq!((bob.balance, bob.locked_cash), (before, 0));
}

/// The case that only exists because the two books cross each other. This
/// bid is nowhere near the YES ask, but a resting NO bid at 38 is an offer
/// of YES at 62, so posting a YES bid at 63 would take it.
#[test]
fn a_post_only_order_is_rejected_for_crossing_the_complementary_book() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 90, 10);
    place(
        &mut ex,
        "alice",
        Outcome::No,
        Side::Buy,
        38,
        10,
        TimeInForce::default(),
    );

    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        63,
        10,
        TimeInForce::PostOnly,
    );
    assert_eq!(
        res.order.status,
        OrderStatus::Rejected,
        "63 for YES would have minted against the 38 bid for NO"
    );
    assert!(levels(&ex, Outcome::Yes).0.is_empty());
    assert_eq!(levels(&ex, Outcome::No).0, vec![(38, 10)]);

    // One cent lower is a quote rather than a take, and it rests.
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        61,
        10,
        TimeInForce::PostOnly,
    );
    assert_eq!(res.order.status, OrderStatus::Open);
}

/// A complementary ask the pool cannot fund is not something to cross, so
/// post-only lets the order rest.
#[test]
fn a_post_only_sell_rests_when_the_only_cross_is_an_unfundable_burn() {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    for who in WHO {
        ex.create_account(who, CASH).unwrap();
        ex.grant_shares(who, "m", Outcome::Yes, 100).unwrap();
        ex.grant_shares(who, "m", Outcome::No, 100).unwrap();
    }
    place(
        &mut ex,
        "alice",
        Outcome::No,
        Side::Sell,
        38,
        10,
        TimeInForce::default(),
    );
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Sell,
        50,
        10,
        TimeInForce::PostOnly,
    );
    assert_eq!(
        res.order.status,
        OrderStatus::Open,
        "with no collateral there is nothing to take"
    );
}

// ---- what a rejected order is --------------------------------------------

#[test]
fn a_rejected_order_is_readable_but_not_cancellable() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 1);
    let id = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::FillOrKill,
    )
    .order
    .id;

    let order = ex.order(id).expect("a rejected order is still on record");
    assert_eq!(order.status, OrderStatus::Rejected);
    assert_eq!(order.time_in_force, TimeInForce::FillOrKill);
    assert_eq!(
        ex.cancel_order("bob", id).unwrap_err(),
        exchangekit_engine::EngineError::OrderNotOpen(id),
        "there is nothing to cancel: it never rested and never traded"
    );
}

#[test]
fn a_rejected_order_still_reports_its_own_errors_first() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 100);
    for bad in [0u32, 100] {
        assert!(ex
            .place(
                OrderRequest::limit("bob", "m", Outcome::Yes, Side::Buy, bad, 1, T)
                    .tif(TimeInForce::PostOnly)
            )
            .is_err());
    }
    assert!(ex
        .place(
            OrderRequest::limit("nobody", "m", Outcome::Yes, Side::Buy, 60, 1, T)
                .tif(TimeInForce::FillOrKill)
        )
        .is_err());
}

// ---- serialization -------------------------------------------------------

#[test]
fn time_in_force_parses_the_short_and_long_spellings() {
    let cases = [
        ("gtc", TimeInForce::GoodTillCancelled),
        ("good_till_cancelled", TimeInForce::GoodTillCancelled),
        ("IOC", TimeInForce::ImmediateOrCancel),
        ("immediate_or_cancel", TimeInForce::ImmediateOrCancel),
        ("fok", TimeInForce::FillOrKill),
        ("fill_or_kill", TimeInForce::FillOrKill),
        ("post_only", TimeInForce::PostOnly),
    ];
    for (text, want) in cases {
        assert_eq!(text.parse::<TimeInForce>(), Ok(want), "{text}");
    }
    assert!("nonsense".parse::<TimeInForce>().is_err());
}

#[test]
fn an_order_serializes_its_type_and_a_rejection_in_snake_case() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 1);
    let res = place(
        &mut ex,
        "bob",
        Outcome::Yes,
        Side::Buy,
        60,
        10,
        TimeInForce::FillOrKill,
    );
    let json = serde_json::to_value(&res.order).unwrap();
    assert_eq!(json["status"], "rejected");
    assert_eq!(json["time_in_force"], "fill_or_kill");
}

#[test]
fn a_snapshot_without_order_types_loads_as_plain_limit_orders() {
    let mut ex = setup();
    rest_yes_ask(&mut ex, "alice", 60, 10);

    let mut snap: serde_json::Value = serde_json::from_str(&ex.to_snapshot()).unwrap();
    for order in snap["orders"].as_object_mut().unwrap().values_mut() {
        order.as_object_mut().unwrap().remove("time_in_force");
    }
    let mut old = Exchange::from_snapshot(&snap.to_string()).unwrap();
    let restored = old.open_orders("alice");
    assert_eq!(restored[0].time_in_force, TimeInForce::GoodTillCancelled);
    // And it still trades.
    let res = old
        .place_order("bob", "m", Outcome::Yes, Side::Buy, 60, 10, T)
        .unwrap();
    assert_eq!(res.trades.len(), 1);
}
