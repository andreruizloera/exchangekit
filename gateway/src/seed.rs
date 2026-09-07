//! Demo state: three markets, four play-money accounts, a market maker
//! providing two-sided liquidity, and a short trade history so the tape
//! and price estimates are populated on first boot.
//!
//! Every share in the seeded exchange is minted as a collateralized
//! YES/NO pair rather than granted, so each seeded market holds exactly
//! what it will owe and `POST /api/markets/{id}/resolve` pays out with no
//! unbacked cash. The game seeds its own one-sided inventory separately;
//! see `game.rs`.

use exchangekit_engine::{Exchange, Outcome, Side};

use crate::now_ms;

const MIN: u64 = 60_000;
/// Starting cash per demo user, in play-money cents ($12,000.00). Enough
/// to cover the pairs each user mints below and still leave $10,000 to
/// trade with.
const USER_CASH: i64 = 1_200_000;
/// The market maker gets deep pockets and deep inventory.
const MM_CASH: i64 = 100_000_000;
/// Pairs minted per market: the market maker's book-making inventory, and
/// a starter position for each demo user so they can sell in the UI.
const MM_PAIRS: u64 = 100_000;
const USER_PAIRS: u64 = 500;

struct SeedMarket {
    id: &'static str,
    question: &'static str,
    description: &'static str,
    /// Target YES price in cents.
    yes: u32,
}

const MARKETS: &[SeedMarket] = &[
    SeedMarket {
        id: "btc-100k",
        question: "Will Bitcoin close above $100,000 this year?",
        description: "Resolves YES if the BTC/USD close on December 31 is above $100,000.",
        yes: 63,
    },
    SeedMarket {
        id: "fed-cut-dec",
        question: "Will the Fed cut rates at its December meeting?",
        description:
            "Resolves YES if the FOMC lowers the federal funds target at the December meeting.",
        yes: 44,
    },
    SeedMarket {
        id: "mars-2030",
        question: "Will humans land on Mars before 2030?",
        description:
            "Resolves YES if a crewed spacecraft touches down on Mars before January 1, 2030.",
        yes: 8,
    },
];

pub fn seed_exchange() -> Exchange {
    let mut ex = Exchange::new();
    let now = now_ms();

    for who in ["demo", "alice", "bob"] {
        ex.create_account(who, USER_CASH).expect("seed account");
    }
    ex.create_account("marketmaker", MM_CASH).expect("seed mm");

    for (i, m) in MARKETS.iter().enumerate() {
        ex.create_market(m.id, m.question, m.description, now - 240 * MIN)
            .expect("seed market");

        // Inventory: the market maker holds deep stock of both outcomes;
        // demo users get a starter position so they can sell in the UI.
        // Minting pairs charges 100 cents each and posts them as this
        // market's collateral, which is what lets it settle fully funded.
        ex.mint_pair("marketmaker", m.id, MM_PAIRS)
            .expect("mm inventory");
        for who in ["demo", "alice", "bob"] {
            ex.mint_pair(who, m.id, USER_PAIRS).expect("inventory");
        }

        let p = m.yes;
        let np = 100 - p;
        let jitter = (i as u64) * 13;

        // Trade history: cross a few controlled orders so the tape and
        // last prices are real fills, oldest first. The last YES trade
        // lands on the target price.
        let crosses: &[(Outcome, u32, u64, &str, u64)] = &[
            (Outcome::Yes, p - 1, 40 + jitter, "alice", 95),
            (Outcome::Yes, p + 1, 15 + jitter, "bob", 70),
            (Outcome::No, np, 20 + jitter, "alice", 45),
            (Outcome::Yes, p, 30 + jitter, "demo", 20),
        ];
        for &(outcome, price, qty, buyer, mins_ago) in crosses {
            let ts = now - mins_ago * MIN;
            ex.place_order("marketmaker", m.id, outcome, Side::Sell, price, qty, ts)
                .expect("seed ask");
            let res = ex
                .place_order(buyer, m.id, outcome, Side::Buy, price, qty, ts)
                .expect("seed cross");
            assert_eq!(res.trades.len(), 1, "seed cross must fill exactly once");
        }

        // Two-sided liquidity ladders from the market maker.
        let ladder: &[(u32, u64)] = &[(1, 180), (2, 260), (4, 420), (7, 600)];
        for &(step, qty) in ladder {
            let q = qty + jitter * 3;
            for (outcome, mid) in [(Outcome::Yes, p), (Outcome::No, np)] {
                ex.place_order("marketmaker", m.id, outcome, Side::Sell, mid + step, q, now)
                    .expect("seed ask ladder");
                ex.place_order("marketmaker", m.id, outcome, Side::Buy, mid - step, q, now)
                    .expect("seed bid ladder");
            }
        }

        // A little non-market-maker interest so the book is not uniform.
        ex.place_order("alice", m.id, Outcome::Yes, Side::Buy, p - 3, 120, now)
            .expect("seed bid");
        ex.place_order("bob", m.id, Outcome::Yes, Side::Sell, p + 3, 90, now)
            .expect("seed ask");
    }

    ex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_state_is_coherent() {
        let ex = seed_exchange();
        assert_eq!(ex.markets().count(), 3);
        for m in MARKETS {
            assert_eq!(ex.price_estimate(m.id, Outcome::Yes), Some(m.yes));
            assert_eq!(ex.price_estimate(m.id, Outcome::No), Some(100 - m.yes));
            let book = ex.book_view(m.id, Outcome::Yes, 20).unwrap();
            assert!(book.bids.len() >= 4, "seeded YES book has bid depth");
            assert!(book.asks.len() >= 4, "seeded YES book has ask depth");
            assert!(!ex.recent_trades(m.id, 10).unwrap().is_empty());
        }
        for who in ["demo", "alice", "bob", "marketmaker"] {
            assert!(ex.account(who).is_some());
        }
    }

    #[test]
    fn every_seeded_share_is_backed_by_collateral() {
        let ex = seed_exchange();
        let pairs = (MM_PAIRS + 3 * USER_PAIRS) as i64;
        for m in MARKETS {
            assert_eq!(
                ex.collateral(m.id),
                pairs * 100,
                "{} holds 100 cents per outstanding pair",
                m.id
            );
        }
    }

    #[test]
    fn a_seeded_market_settles_with_no_unbacked_cash() {
        let mut ex = seed_exchange();
        let s = ex
            .resolve_market("mars-2030", Outcome::No, now_ms())
            .unwrap();
        assert_eq!(s.unbacked_cash, 0, "seeded shares are all minted as pairs");
        assert_eq!(s.total_paid, s.collateral);
        assert!(s.orders_voided > 0, "the seeded ladders were resting");
    }

    /// The seeded ladders are symmetric around the target price, so a YES
    /// quote and the NO quote at the same step never add up to a dollar in
    /// the direction that would cross. That is what keeps the demo's
    /// numbers the same on every fresh gateway now that the two books
    /// match against each other.
    #[test]
    fn the_seeded_books_do_not_cross_each_other() {
        let ex = seed_exchange();
        for m in MARKETS {
            let yes = ex.book_view(m.id, Outcome::Yes, 99).unwrap();
            let no = ex.book_view(m.id, Outcome::No, 99).unwrap();
            let top = |levels: &[exchangekit_engine::Level]| levels.first().map(|l| l.price);
            let (yes_bid, no_bid) = (top(&yes.bids), top(&no.bids));
            let (yes_ask, no_ask) = (top(&yes.asks), top(&no.asks));
            assert!(
                yes_bid.unwrap() + no_bid.unwrap() < 100,
                "{}: the two best bids would mint a pair",
                m.id
            );
            assert!(
                yes_ask.unwrap() + no_ask.unwrap() > 100,
                "{}: the two best asks would burn a pair",
                m.id
            );
        }
    }

    /// The engine will not release collateral a market does not hold, and
    /// the seeded markets hold plenty, so a complementary sell there is
    /// funded rather than skipped.
    #[test]
    fn a_seeded_market_can_fund_a_burn() {
        let mut ex = seed_exchange();
        let before = ex.collateral("btc-100k");
        ex.place_order(
            "demo",
            "btc-100k",
            Outcome::Yes,
            Side::Sell,
            55,
            10,
            now_ms(),
        )
        .unwrap();
        let res = ex
            .place_order(
                "alice",
                "btc-100k",
                Outcome::No,
                Side::Sell,
                30,
                10,
                now_ms(),
            )
            .unwrap();
        assert_eq!(res.trades.len(), 1);
        assert_eq!(
            res.trades[0].kind,
            exchangekit_engine::TradeKind::Burn,
            "55 and 30 leave 15 cents on the table, so the pair is burned"
        );
        assert_eq!(ex.collateral("btc-100k"), before - 1_000);
    }

    #[test]
    fn resolving_one_seeded_market_leaves_the_others_trading() {
        let mut ex = seed_exchange();
        ex.resolve_market("mars-2030", Outcome::No, now_ms())
            .unwrap();
        assert!(ex
            .place_order("demo", "btc-100k", Outcome::Yes, Side::Buy, 1, 1, now_ms())
            .is_ok());
        assert!(ex.collateral("btc-100k") > 0);
    }
}
