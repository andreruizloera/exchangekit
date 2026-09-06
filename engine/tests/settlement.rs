//! Market resolution: paying out winning shares, voiding resting orders,
//! closing the market, and the collateral accounting that decides whether
//! a payout was funded.

use exchangekit_engine::{
    EngineError, Exchange, Order, OrderStatus, Outcome, Settlement, Side, SHARE_PAYOUT,
};

const CASH: i64 = 1_000_000; // $10,000.00 in play-money cents
const T: u64 = 1_000; // arbitrary timestamp

/// One market and three funded accounts, no shares yet. Each test creates
/// the shares it wants, so the collateral story is explicit every time.
fn setup() -> Exchange {
    let mut ex = Exchange::new();
    ex.create_market("m", "Test market?", "", T).unwrap();
    ex.create_market("other", "Untouched market?", "", T)
        .unwrap();
    for who in ["alice", "bob", "carol"] {
        ex.create_account(who, CASH).unwrap();
    }
    ex
}

fn total_cash(ex: &Exchange) -> i64 {
    ["alice", "bob", "carol"]
        .iter()
        .map(|w| ex.account(w).unwrap().balance)
        .sum()
}

fn position(ex: &Exchange, who: &str, market: &str, outcome: Outcome) -> (i64, i64) {
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

fn payout_for<'a>(s: &'a Settlement, account: &str) -> Option<&'a exchangekit_engine::Payout> {
    s.payouts.iter().find(|p| p.account == account)
}

// ---- minting pairs -------------------------------------------------------

#[test]
fn minting_a_pair_charges_a_dollar_and_hands_back_both_outcomes() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 25).unwrap();
    assert_eq!(ex.account("alice").unwrap().balance, CASH - 2_500);
    assert_eq!(position(&ex, "alice", "m", Outcome::Yes), (25, 0));
    assert_eq!(position(&ex, "alice", "m", Outcome::No), (25, 0));
    assert_eq!(ex.collateral("m"), 2_500);
}

#[test]
fn minting_is_rejected_when_the_account_cannot_pay() {
    let mut ex = setup();
    let err = ex.mint_pair("alice", "m", 20_000).unwrap_err();
    assert!(matches!(err, EngineError::InsufficientBalance { .. }));
    assert_eq!(ex.collateral("m"), 0, "a rejected mint posts no collateral");
}

#[test]
fn minting_counts_cash_locked_in_resting_buys_as_unavailable() {
    let mut ex = setup();
    ex.place_order("alice", "m", Outcome::Yes, Side::Buy, 50, 19_000, T)
        .unwrap(); // locks 950,000 of 1,000,000
    let err = ex.mint_pair("alice", "m", 1_000).unwrap_err();
    assert!(matches!(err, EngineError::InsufficientBalance { .. }));
}

#[test]
fn minting_rejects_zero_and_unknown_names() {
    let mut ex = setup();
    assert_eq!(
        ex.mint_pair("alice", "m", 0).unwrap_err(),
        EngineError::InvalidQuantity
    );
    assert!(matches!(
        ex.mint_pair("nobody", "m", 1),
        Err(EngineError::UnknownAccount(_))
    ));
    assert!(matches!(
        ex.mint_pair("alice", "nope", 1),
        Err(EngineError::UnknownMarket(_))
    ));
}

// ---- the payout ----------------------------------------------------------

#[test]
fn winning_shares_pay_one_dollar_each_and_losing_shares_pay_nothing() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 10).unwrap();
    // Alice sells her NO shares to bob, so she is long YES only.
    ex.place_order("alice", "m", Outcome::No, Side::Sell, 40, 10, T)
        .unwrap();
    ex.place_order("bob", "m", Outcome::No, Side::Buy, 40, 10, T)
        .unwrap();

    let before = ex.account("alice").unwrap().balance;
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();

    assert_eq!(s.outcome, Outcome::Yes);
    assert_eq!(payout_for(&s, "alice").unwrap().paid, 10 * SHARE_PAYOUT);
    assert_eq!(payout_for(&s, "bob").unwrap().paid, 0);
    assert_eq!(payout_for(&s, "bob").unwrap().losing_shares, 10);
    assert_eq!(ex.account("alice").unwrap().balance, before + 1_000);
    assert_eq!(s.winning_shares, 10);
    assert_eq!(s.losing_shares, 10);
}

#[test]
fn settled_positions_are_cleared_on_both_sides() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 7).unwrap();
    ex.resolve_market("m", Outcome::No, T).unwrap();
    assert_eq!(position(&ex, "alice", "m", Outcome::Yes), (0, 0));
    assert_eq!(position(&ex, "alice", "m", Outcome::No), (0, 0));
}

#[test]
fn an_account_that_holds_nothing_is_left_out_of_the_report() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 5).unwrap();
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    let accounts: Vec<&str> = s.payouts.iter().map(|p| p.account.as_str()).collect();
    assert_eq!(accounts, vec!["alice"], "bob and carol never held a share");
}

#[test]
fn payouts_are_reported_in_account_order() {
    let mut ex = setup();
    for who in ["carol", "alice", "bob"] {
        ex.mint_pair(who, "m", 1).unwrap();
    }
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    let accounts: Vec<&str> = s.payouts.iter().map(|p| p.account.as_str()).collect();
    assert_eq!(accounts, vec!["alice", "bob", "carol"]);
}

// ---- voiding resting orders ----------------------------------------------

#[test]
fn resting_orders_are_voided_and_say_so() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 100).unwrap();
    let ask = ex
        .place_order("alice", "m", Outcome::Yes, Side::Sell, 70, 10, T)
        .unwrap()
        .order
        .id;
    let bid = ex
        .place_order("bob", "m", Outcome::Yes, Side::Buy, 30, 10, T)
        .unwrap()
        .order
        .id;

    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.orders_voided, 2);
    for id in [ask, bid] {
        assert_eq!(
            ex.order(id).unwrap().status,
            OrderStatus::Voided,
            "a voided order is not something its owner cancelled"
        );
    }
    let book = ex.book_view("m", Outcome::Yes, 10).unwrap();
    assert!(book.bids.is_empty() && book.asks.is_empty());
    assert!(ex.open_orders("alice").is_empty());
}

#[test]
fn voiding_a_resting_buy_releases_its_cash_escrow() {
    let mut ex = setup();
    ex.place_order("bob", "m", Outcome::Yes, Side::Buy, 30, 10, T)
        .unwrap();
    assert_eq!(ex.account("bob").unwrap().locked_cash, 300);
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.cash_released, 300);
    let bob = ex.account("bob").unwrap();
    assert_eq!(bob.locked_cash, 0);
    assert_eq!(bob.balance, CASH, "the escrow was never spent");
}

/// The ordering invariant. Shares committed to a resting sell are locked;
/// if the payout ran before the void, those shares would either be paid
/// while still locked (leaving a lock on a position that no longer exists)
/// or skipped entirely (robbing the seller of a share they still own).
#[test]
fn shares_locked_in_a_resting_sell_are_released_and_still_paid() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 40).unwrap();
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 90, 40, T)
        .unwrap();
    assert_eq!(position(&ex, "alice", "m", Outcome::Yes), (40, 40));

    let before = ex.account("alice").unwrap().balance;
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();

    assert_eq!(s.shares_released, 40);
    assert_eq!(
        payout_for(&s, "alice").unwrap().winning_shares,
        40,
        "shares offered for sale are still owned and still settle"
    );
    assert_eq!(ex.account("alice").unwrap().balance, before + 4_000);
    assert_eq!(position(&ex, "alice", "m", Outcome::Yes), (0, 0));
}

#[test]
fn a_partially_filled_resting_order_releases_only_its_remainder() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 10).unwrap();
    ex.place_order("bob", "m", Outcome::Yes, Side::Buy, 60, 10, T)
        .unwrap(); // locks 600
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 60, 4, T)
        .unwrap(); // fills 4, leaving 6 resting
    assert_eq!(ex.account("bob").unwrap().locked_cash, 360);

    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.cash_released, 360);
    assert_eq!(ex.account("bob").unwrap().locked_cash, 0);
}

#[test]
fn both_books_are_voided_not_only_the_winning_one() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 20).unwrap();
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 70, 5, T)
        .unwrap();
    ex.place_order("alice", "m", Outcome::No, Side::Sell, 20, 5, T)
        .unwrap();
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.orders_voided, 2);
    assert!(ex.book_view("m", Outcome::No, 10).unwrap().asks.is_empty());
}

#[test]
fn a_voided_order_cannot_then_be_cancelled() {
    let mut ex = setup();
    let id = ex
        .place_order("bob", "m", Outcome::Yes, Side::Buy, 30, 10, T)
        .unwrap()
        .order
        .id;
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(
        ex.cancel_order("bob", id).unwrap_err(),
        EngineError::OrderNotOpen(id),
        "the escrow came back at resolution; cancelling would release it twice"
    );
    assert_eq!(ex.account("bob").unwrap().locked_cash, 0);
}

// ---- the market closes ---------------------------------------------------

#[test]
fn a_resolved_market_stops_accepting_orders() {
    let mut ex = setup();
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(
        ex.place_order("alice", "m", Outcome::Yes, Side::Buy, 50, 1, T)
            .unwrap_err(),
        EngineError::MarketResolved("m".to_string())
    );
}

#[test]
fn a_resolved_market_stops_issuing_shares() {
    let mut ex = setup();
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(
        ex.mint_pair("alice", "m", 1).unwrap_err(),
        EngineError::MarketResolved("m".to_string())
    );
    assert_eq!(
        ex.grant_shares("alice", "m", Outcome::Yes, 1).unwrap_err(),
        EngineError::MarketResolved("m".to_string())
    );
}

#[test]
fn resolving_twice_is_refused_and_pays_nothing_the_second_time() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 10).unwrap();
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    let after_first = ex.account("alice").unwrap().balance;

    assert_eq!(
        ex.resolve_market("m", Outcome::No, T).unwrap_err(),
        EngineError::MarketAlreadyResolved("m".to_string())
    );
    assert_eq!(ex.account("alice").unwrap().balance, after_first);
    assert_eq!(
        ex.resolution("m").unwrap().outcome,
        Outcome::Yes,
        "the first resolution stands"
    );
}

#[test]
fn resolving_an_unknown_market_is_refused() {
    let mut ex = setup();
    assert_eq!(
        ex.resolve_market("nope", Outcome::Yes, T).unwrap_err(),
        EngineError::UnknownMarket("nope".to_string())
    );
}

#[test]
fn resolution_touches_only_its_own_market() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 10).unwrap();
    ex.mint_pair("alice", "other", 10).unwrap();
    ex.place_order("alice", "other", Outcome::Yes, Side::Sell, 70, 10, T)
        .unwrap();

    ex.resolve_market("m", Outcome::Yes, T).unwrap();

    assert_eq!(position(&ex, "alice", "other", Outcome::Yes), (10, 10));
    assert_eq!(ex.collateral("other"), 1_000);
    assert!(ex.resolution("other").is_none());
    assert!(ex
        .place_order("bob", "other", Outcome::Yes, Side::Buy, 70, 1, T)
        .is_ok());
}

// ---- funding -------------------------------------------------------------

#[test]
fn a_fully_paired_market_conserves_cash_across_trading_and_settlement() {
    let mut ex = setup();
    let start = total_cash(&ex);

    ex.mint_pair("alice", "m", 100).unwrap();
    ex.mint_pair("bob", "m", 50).unwrap();
    assert_eq!(ex.collateral("m"), 15_000);
    assert_eq!(total_cash(&ex), start - 15_000);

    // Trade both outcomes around between the three accounts.
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 60, 30, T)
        .unwrap();
    ex.place_order("carol", "m", Outcome::Yes, Side::Buy, 60, 30, T)
        .unwrap();
    ex.place_order("bob", "m", Outcome::No, Side::Sell, 35, 20, T)
        .unwrap();
    ex.place_order("alice", "m", Outcome::No, Side::Buy, 35, 20, T)
        .unwrap();
    assert_eq!(
        total_cash(&ex),
        start - 15_000,
        "trading moves cash, not more"
    );

    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.total_paid, 15_000);
    assert_eq!(s.collateral, 15_000);
    assert_eq!(s.unbacked_cash, 0);
    assert_eq!(
        total_cash(&ex),
        start,
        "every cent paid out came from the collateral the pairs posted"
    );
    assert_eq!(ex.collateral("m"), 0, "the pool was spent");
}

#[test]
fn granted_shares_settle_as_unbacked_cash_and_the_report_names_it() {
    let mut ex = setup();
    let start = total_cash(&ex);
    ex.mint_pair("alice", "m", 10).unwrap(); // 1,000 cents of collateral
    ex.grant_shares("bob", "m", Outcome::Yes, 3).unwrap(); // free shares

    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.total_paid, 1_300);
    assert_eq!(s.collateral, 1_000);
    assert_eq!(
        s.unbacked_cash, 300,
        "bob's three granted shares had nothing behind them"
    );
    assert_eq!(total_cash(&ex), start + 300, "the engine created that cash");
}

#[test]
fn a_market_with_only_losing_shares_outstanding_pays_nobody() {
    let mut ex = setup();
    ex.grant_shares("alice", "m", Outcome::No, 12).unwrap();
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.total_paid, 0);
    assert_eq!(s.winning_shares, 0);
    assert_eq!(s.losing_shares, 12);
    assert_eq!(s.unbacked_cash, 0);
}

#[test]
fn resolving_an_empty_market_is_a_clean_no_op() {
    let mut ex = setup();
    let s = ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert!(s.payouts.is_empty());
    assert_eq!(s.total_paid, 0);
    assert_eq!(s.orders_voided, 0);
    assert!(ex.resolution("m").is_some());
}

// ---- reading it back -----------------------------------------------------

#[test]
fn the_settlement_report_stays_readable_afterwards() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 6).unwrap();
    let returned = ex.resolve_market("m", Outcome::No, 4_242).unwrap();
    let stored = ex.settlement("m").expect("recorded");
    assert_eq!(stored, &returned);
    assert_eq!(stored.resolved_at, 4_242);
    assert!(ex.settlement("other").is_none());
}

#[test]
fn trade_history_survives_resolution() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 10).unwrap();
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 60, 5, T)
        .unwrap();
    ex.place_order("bob", "m", Outcome::Yes, Side::Buy, 60, 5, T)
        .unwrap();
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(ex.recent_trades("m", 10).unwrap().len(), 1);
    assert_eq!(ex.volume("m"), 5);
}

#[test]
fn snapshot_roundtrip_preserves_a_resolution() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 8).unwrap();
    let id = ex
        .place_order("bob", "m", Outcome::Yes, Side::Buy, 30, 2, T)
        .unwrap()
        .order
        .id;
    let settlement = ex.resolve_market("m", Outcome::Yes, T).unwrap();

    let mut restored = Exchange::from_snapshot(&ex.to_snapshot()).unwrap();
    assert_eq!(restored.settlement("m"), Some(&settlement));
    assert_eq!(restored.resolution("m").unwrap().outcome, Outcome::Yes);
    assert_eq!(restored.order(id).unwrap().status, OrderStatus::Voided);
    assert_eq!(restored.collateral("m"), 0);
    assert_eq!(
        restored
            .place_order("alice", "m", Outcome::Yes, Side::Buy, 50, 1, T)
            .unwrap_err(),
        EngineError::MarketResolved("m".to_string()),
        "a restored market is still closed"
    );
}

/// Snapshots written before settlement existed have no `resolution`,
/// `collateral`, or `settlements` keys. They must load as open markets
/// rather than failing to parse.
#[test]
fn a_snapshot_without_settlement_fields_loads_as_an_open_market() {
    let mut ex = setup();
    ex.mint_pair("alice", "m", 3).unwrap();
    ex.place_order("alice", "m", Outcome::Yes, Side::Sell, 60, 1, T)
        .unwrap();

    let mut snap: serde_json::Value = serde_json::from_str(&ex.to_snapshot()).unwrap();
    let obj = snap.as_object_mut().unwrap();
    obj.remove("collateral");
    obj.remove("settlements");
    for market in obj["markets"].as_object_mut().unwrap().values_mut() {
        market.as_object_mut().unwrap().remove("resolution");
    }

    let mut old = Exchange::from_snapshot(&snap.to_string()).unwrap();
    assert!(old.resolution("m").is_none());
    assert_eq!(
        old.collateral("m"),
        0,
        "an old snapshot posted no collateral"
    );
    // It still trades, and it can still resolve; the payout just reads as
    // unbacked, because the snapshot carries no record of any collateral.
    let s = old.resolve_market("m", Outcome::Yes, T).unwrap();
    assert_eq!(s.total_paid, 300);
    assert_eq!(s.unbacked_cash, 300);
}

#[test]
fn order_status_serializes_as_snake_case() {
    let mut ex = setup();
    let id = ex
        .place_order("bob", "m", Outcome::Yes, Side::Buy, 30, 1, T)
        .unwrap()
        .order
        .id;
    ex.resolve_market("m", Outcome::Yes, T).unwrap();
    let order: Order = ex.order(id).unwrap().clone();
    let json = serde_json::to_value(&order).unwrap();
    assert_eq!(json["status"], "voided");
}
