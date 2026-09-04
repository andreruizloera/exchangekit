//! Matching engine behavior tests: price priority, time priority,
//! partial and full fills, cancels, crossing orders, empty books,
//! multiple price levels, escrow accounting, and snapshots.

use exchangekit_engine::{EngineError, Exchange, OrderStatus, Outcome, Side};

const CASH: i64 = 1_000_000; // $10,000.00 in play-money cents
const T: u64 = 1_000; // arbitrary timestamp

/// Exchange with one market and three funded accounts. Sellers get
/// 1,000 YES and 1,000 NO shares so sell-side tests have inventory.
fn setup() -> Exchange {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    for who in ["alice", "bob", "carol"] {
        ex.create_account(who, CASH).unwrap();
        ex.grant_shares(who, "m", Outcome::Yes, 1_000).unwrap();
        ex.grant_shares(who, "m", Outcome::No, 1_000).unwrap();
    }
    ex
}

fn buy(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> exchangekit_engine::PlaceResult {
    ex.place_order(who, "m", Outcome::Yes, Side::Buy, price, qty, T)
        .unwrap()
}

fn sell(ex: &mut Exchange, who: &str, price: u32, qty: u64) -> exchangekit_engine::PlaceResult {
    ex.place_order(who, "m", Outcome::Yes, Side::Sell, price, qty, T)
        .unwrap()
}

// ---- empty book ----------------------------------------------------------

#[test]
fn buy_on_empty_book_rests_without_trading() {
    let mut ex = setup();
    let res = buy(&mut ex, "alice", 60, 10);
    assert!(res.trades.is_empty());
    assert_eq!(res.order.status, OrderStatus::Open);
    assert_eq!(res.order.filled, 0);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.bids.len(), 1);
    assert_eq!(book.bids[0].price, 60);
    assert_eq!(book.bids[0].quantity, 10);
    assert!(book.asks.is_empty());
}

#[test]
fn sell_on_empty_book_rests_without_trading() {
    let mut ex = setup();
    let res = sell(&mut ex, "alice", 60, 10);
    assert!(res.trades.is_empty());
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert!(book.bids.is_empty());
    assert_eq!(
        book.asks,
        vec![exchangekit_engine::Level {
            price: 60,
            quantity: 10
        }]
    );
}

#[test]
fn non_crossing_orders_do_not_trade() {
    let mut ex = setup();
    buy(&mut ex, "alice", 55, 10);
    let res = sell(&mut ex, "bob", 60, 10);
    assert!(res.trades.is_empty());
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.bids[0].price, 55);
    assert_eq!(book.asks[0].price, 60);
}

// ---- crossing, full and partial fills ------------------------------------

#[test]
fn crossing_orders_fill_completely_at_resting_price() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    let res = buy(&mut ex, "bob", 60, 10);
    assert_eq!(res.trades.len(), 1);
    let t = &res.trades[0];
    assert_eq!(t.price, 60);
    assert_eq!(t.quantity, 10);
    assert_eq!(t.buyer, "bob");
    assert_eq!(t.seller, "alice");
    assert_eq!(t.taker_side, Side::Buy);
    assert_eq!(res.order.status, OrderStatus::Filled);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert!(book.bids.is_empty() && book.asks.is_empty());
}

#[test]
fn aggressive_buy_executes_at_resting_ask_price() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    let res = buy(&mut ex, "bob", 75, 10);
    assert_eq!(res.trades[0].price, 60, "taker gets price improvement");
    // Bob escrowed 75*10 but paid 60*10; the difference is released.
    let bob = ex.account("bob").unwrap();
    assert_eq!(bob.balance, CASH - 600);
    assert_eq!(bob.locked_cash, 0);
}

#[test]
fn aggressive_sell_executes_at_resting_bid_price() {
    let mut ex = setup();
    buy(&mut ex, "alice", 60, 10);
    let res = sell(&mut ex, "bob", 40, 10);
    assert_eq!(res.trades[0].price, 60);
    assert_eq!(ex.account("bob").unwrap().balance, CASH + 600);
}

#[test]
fn partial_fill_leaves_taker_remainder_resting() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 4);
    let res = buy(&mut ex, "bob", 60, 10);
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].quantity, 4);
    assert_eq!(res.order.filled, 4);
    assert_eq!(res.order.status, OrderStatus::Open);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.bids[0].price, 60);
    assert_eq!(book.bids[0].quantity, 6);
    assert!(book.asks.is_empty());
}

#[test]
fn partial_fill_leaves_maker_remainder_resting() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    buy(&mut ex, "bob", 60, 4);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.asks[0].quantity, 6);
    let maker = ex.open_orders("alice");
    assert_eq!(maker.len(), 1);
    assert_eq!(maker[0].filled, 4);
}

#[test]
fn marketable_limit_walks_multiple_price_levels() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    sell(&mut ex, "bob", 61, 10);
    sell(&mut ex, "carol", 62, 10);
    let res = buy(&mut ex, "alice", 62, 25);
    let prices: Vec<u32> = res.trades.iter().map(|t| t.price).collect();
    let qtys: Vec<u64> = res.trades.iter().map(|t| t.quantity).collect();
    assert_eq!(prices, vec![60, 61, 62], "fills walk the book best-first");
    assert_eq!(qtys, vec![10, 10, 5]);
    assert_eq!(res.order.status, OrderStatus::Filled);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(
        book.asks,
        vec![exchangekit_engine::Level {
            price: 62,
            quantity: 5
        }]
    );
}

#[test]
fn limit_price_is_respected_when_walking_the_book() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    sell(&mut ex, "bob", 65, 10);
    let res = buy(&mut ex, "carol", 62, 20);
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].price, 60);
    assert_eq!(res.order.filled, 10);
    // Remainder rests at the limit, below the next ask.
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.bids[0].price, 62);
    assert_eq!(book.bids[0].quantity, 10);
    assert_eq!(book.asks[0].price, 65);
}

// ---- price priority ------------------------------------------------------

#[test]
fn buy_takes_lowest_ask_first() {
    let mut ex = setup();
    sell(&mut ex, "alice", 64, 10);
    sell(&mut ex, "bob", 61, 10);
    sell(&mut ex, "carol", 62, 10);
    let res = buy(&mut ex, "alice", 70, 10);
    assert_eq!(res.trades[0].price, 61);
    assert_eq!(res.trades[0].seller, "bob");
}

#[test]
fn sell_takes_highest_bid_first() {
    let mut ex = setup();
    buy(&mut ex, "alice", 55, 10);
    buy(&mut ex, "bob", 59, 10);
    buy(&mut ex, "carol", 57, 10);
    let res = sell(&mut ex, "alice", 40, 10);
    assert_eq!(res.trades[0].price, 59);
    assert_eq!(res.trades[0].buyer, "bob");
}

// ---- time priority -------------------------------------------------------

#[test]
fn fifo_within_a_price_level() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10); // first at 60
    sell(&mut ex, "bob", 60, 10); // second at 60
    let res = buy(&mut ex, "carol", 60, 10);
    assert_eq!(res.trades.len(), 1);
    assert_eq!(
        res.trades[0].seller, "alice",
        "earlier order at same price fills first"
    );
    let res2 = buy(&mut ex, "carol", 60, 10);
    assert_eq!(res2.trades[0].seller, "bob");
}

#[test]
fn time_priority_survives_partial_fills() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    sell(&mut ex, "bob", 60, 10);
    buy(&mut ex, "carol", 60, 4); // partially fills alice
    let res = buy(&mut ex, "carol", 60, 8);
    // Alice's remaining 6 still has priority, then bob's 2.
    assert_eq!(res.trades.len(), 2);
    assert_eq!(res.trades[0].seller, "alice");
    assert_eq!(res.trades[0].quantity, 6);
    assert_eq!(res.trades[1].seller, "bob");
    assert_eq!(res.trades[1].quantity, 2);
}

// ---- cancel --------------------------------------------------------------

#[test]
fn cancel_removes_order_from_book() {
    let mut ex = setup();
    let res = sell(&mut ex, "alice", 60, 10);
    ex.cancel_order("alice", res.order.id).unwrap();
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert!(book.asks.is_empty());
    // A crossing buy now rests instead of trading.
    let res2 = buy(&mut ex, "bob", 60, 10);
    assert!(res2.trades.is_empty());
    assert_eq!(
        ex.order(res.order.id).unwrap().status,
        OrderStatus::Cancelled
    );
}

#[test]
fn cancel_releases_cash_escrow() {
    let mut ex = setup();
    let res = buy(&mut ex, "alice", 60, 10);
    assert_eq!(ex.account("alice").unwrap().locked_cash, 600);
    ex.cancel_order("alice", res.order.id).unwrap();
    let a = ex.account("alice").unwrap();
    assert_eq!(a.locked_cash, 0);
    assert_eq!(a.balance, CASH);
}

#[test]
fn cancel_releases_share_escrow() {
    let mut ex = setup();
    let res = sell(&mut ex, "alice", 60, 10);
    let pos = ex.account("alice").unwrap().positions["m"].get(Outcome::Yes);
    assert_eq!(pos.locked, 10);
    ex.cancel_order("alice", res.order.id).unwrap();
    let pos = ex.account("alice").unwrap().positions["m"].get(Outcome::Yes);
    assert_eq!(pos.locked, 0);
    assert_eq!(pos.quantity, 1_000);
}

#[test]
fn cancel_partially_filled_order_releases_only_remainder() {
    let mut ex = setup();
    let res = buy(&mut ex, "alice", 60, 10);
    sell(&mut ex, "bob", 60, 4);
    assert_eq!(ex.account("alice").unwrap().locked_cash, 360);
    ex.cancel_order("alice", res.order.id).unwrap();
    let a = ex.account("alice").unwrap();
    assert_eq!(a.locked_cash, 0);
    assert_eq!(a.balance, CASH - 240); // paid for 4 shares at 60
}

#[test]
fn cancel_rejects_wrong_owner() {
    let mut ex = setup();
    let res = buy(&mut ex, "alice", 60, 10);
    let err = ex.cancel_order("bob", res.order.id).unwrap_err();
    assert!(matches!(err, EngineError::NotOrderOwner { .. }));
}

#[test]
fn cancel_rejects_filled_or_cancelled_orders() {
    let mut ex = setup();
    let res = sell(&mut ex, "alice", 60, 10);
    buy(&mut ex, "bob", 60, 10);
    assert_eq!(
        ex.cancel_order("alice", res.order.id).unwrap_err(),
        EngineError::OrderNotOpen(res.order.id)
    );
    let res2 = sell(&mut ex, "alice", 61, 5);
    ex.cancel_order("alice", res2.order.id).unwrap();
    assert_eq!(
        ex.cancel_order("alice", res2.order.id).unwrap_err(),
        EngineError::OrderNotOpen(res2.order.id)
    );
}

#[test]
fn cancel_unknown_order_rejected() {
    let mut ex = setup();
    assert_eq!(
        ex.cancel_order("alice", 999).unwrap_err(),
        EngineError::UnknownOrder(999)
    );
}

// ---- validation and risk checks ------------------------------------------

#[test]
fn rejects_prices_outside_1_to_99() {
    let mut ex = setup();
    for bad in [0u32, 100, 250] {
        let err = ex
            .place_order("alice", "m", Outcome::Yes, Side::Buy, bad, 1, T)
            .unwrap_err();
        assert_eq!(err, EngineError::InvalidPrice(bad));
    }
}

#[test]
fn rejects_zero_quantity() {
    let mut ex = setup();
    let err = ex
        .place_order("alice", "m", Outcome::Yes, Side::Buy, 50, 0, T)
        .unwrap_err();
    assert_eq!(err, EngineError::InvalidQuantity);
}

#[test]
fn rejects_buy_beyond_available_cash() {
    let mut ex = setup();
    // 99 cents * 20,000 shares = 1,980,000 > 1,000,000 balance.
    let err = ex
        .place_order("alice", "m", Outcome::Yes, Side::Buy, 99, 20_000, T)
        .unwrap_err();
    assert!(matches!(err, EngineError::InsufficientBalance { .. }));
    // Open orders count against available cash.
    buy(&mut ex, "alice", 50, 19_000); // locks 950,000
    let err = ex
        .place_order("alice", "m", Outcome::Yes, Side::Buy, 50, 2_000, T)
        .unwrap_err();
    assert!(matches!(err, EngineError::InsufficientBalance { .. }));
}

#[test]
fn rejects_sell_beyond_available_shares() {
    let mut ex = setup();
    let err = ex
        .place_order("alice", "m", Outcome::Yes, Side::Sell, 60, 1_001, T)
        .unwrap_err();
    assert!(matches!(err, EngineError::InsufficientPosition { .. }));
    // Locked shares in open sells also count.
    sell(&mut ex, "alice", 60, 900);
    let err = ex
        .place_order("alice", "m", Outcome::Yes, Side::Sell, 61, 200, T)
        .unwrap_err();
    assert!(matches!(err, EngineError::InsufficientPosition { .. }));
}

#[test]
fn rejects_unknown_market_and_account() {
    let mut ex = setup();
    assert!(matches!(
        ex.place_order("alice", "nope", Outcome::Yes, Side::Buy, 50, 1, T),
        Err(EngineError::UnknownMarket(_))
    ));
    assert!(matches!(
        ex.place_order("nobody", "m", Outcome::Yes, Side::Buy, 50, 1, T),
        Err(EngineError::UnknownAccount(_))
    ));
}

// ---- accounting invariants -----------------------------------------------

#[test]
fn cash_and_shares_are_conserved_by_trading() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    sell(&mut ex, "bob", 62, 30);
    buy(&mut ex, "carol", 62, 25);
    let total_cash: i64 = ["alice", "bob", "carol"]
        .iter()
        .map(|w| ex.account(w).unwrap().balance)
        .sum();
    assert_eq!(total_cash, 3 * CASH);
    let total_yes: i64 = ["alice", "bob", "carol"]
        .iter()
        .map(|w| {
            ex.account(w).unwrap().positions["m"]
                .get(Outcome::Yes)
                .quantity
        })
        .sum();
    assert_eq!(total_yes, 3_000);
    // Carol paid 10*60 + 15*62 = 1530 and holds 25 more YES shares.
    let carol = ex.account("carol").unwrap();
    assert_eq!(carol.balance, CASH - 1_530);
    assert_eq!(carol.positions["m"].get(Outcome::Yes).quantity, 1_025);
}

#[test]
fn yes_and_no_books_are_independent() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10); // YES ask
    let res = ex
        .place_order("bob", "m", Outcome::No, Side::Buy, 60, 10, T)
        .unwrap();
    assert!(res.trades.is_empty(), "NO buy must not match a YES ask");
    let no_book = ex.book_view("m", Outcome::No, 10).unwrap();
    assert_eq!(no_book.bids[0].price, 60);
    let yes_book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(yes_book.asks[0].price, 60);
}

// ---- views and history ---------------------------------------------------

#[test]
fn book_view_aggregates_levels_in_price_order() {
    let mut ex = setup();
    buy(&mut ex, "alice", 55, 10);
    buy(&mut ex, "bob", 57, 5);
    buy(&mut ex, "carol", 57, 7);
    sell(&mut ex, "alice", 60, 3);
    sell(&mut ex, "bob", 64, 9);
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    let bids: Vec<(u32, u64)> = book.bids.iter().map(|l| (l.price, l.quantity)).collect();
    let asks: Vec<(u32, u64)> = book.asks.iter().map(|l| (l.price, l.quantity)).collect();
    assert_eq!(bids, vec![(57, 12), (55, 10)], "bids best (highest) first");
    assert_eq!(asks, vec![(60, 3), (64, 9)], "asks best (lowest) first");
    // Depth limit applies per side.
    let shallow = ex.book_view("m", Outcome::Yes, 1).unwrap();
    assert_eq!(shallow.bids.len(), 1);
    assert_eq!(shallow.asks.len(), 1);
}

#[test]
fn recent_trades_are_newest_first() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 5);
    sell(&mut ex, "alice", 61, 5);
    buy(&mut ex, "bob", 61, 10);
    let trades = ex.recent_trades("m", 10).unwrap();
    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].price, 61, "newest trade first");
    assert_eq!(trades[1].price, 60);
    assert_eq!(ex.recent_trades("m", 1).unwrap().len(), 1);
}

#[test]
fn price_estimate_prefers_last_trade_then_midpoint() {
    let mut ex = setup();
    assert_eq!(ex.price_estimate("m", Outcome::Yes), None);
    buy(&mut ex, "alice", 58, 10);
    sell(&mut ex, "bob", 66, 10);
    assert_eq!(
        ex.price_estimate("m", Outcome::Yes),
        Some(62),
        "midpoint of 58/66"
    );
    sell(&mut ex, "bob", 58, 5); // trades at 58
    assert_eq!(ex.price_estimate("m", Outcome::Yes), Some(58));
}

#[test]
fn volume_sums_traded_shares() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    buy(&mut ex, "bob", 60, 4);
    buy(&mut ex, "carol", 60, 6);
    assert_eq!(ex.volume("m"), 10);
}

#[test]
fn snapshot_roundtrip_preserves_state() {
    let mut ex = setup();
    sell(&mut ex, "alice", 60, 10);
    buy(&mut ex, "bob", 60, 4);
    let snap = ex.to_snapshot();
    let mut restored = Exchange::from_snapshot(&snap).unwrap();
    // Book, balances, and history survive.
    let book = restored.book_view("m", Outcome::Yes, 10).unwrap();
    assert_eq!(book.asks[0].quantity, 6);
    assert_eq!(restored.account("bob").unwrap().balance, CASH - 240);
    assert_eq!(restored.recent_trades("m", 10).unwrap().len(), 1);
    // And the restored exchange keeps matching correctly.
    let res = restored
        .place_order("carol", "m", Outcome::Yes, Side::Buy, 60, 6, T)
        .unwrap();
    assert_eq!(res.trades.len(), 1);
    assert_eq!(res.trades[0].seller, "alice");
}
