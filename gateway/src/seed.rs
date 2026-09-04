//! Demo state: three markets, four play-money accounts, a market maker
//! providing two-sided liquidity, and a short trade history so the tape
//! and price estimates are populated on first boot.

use exchangekit_engine::{Exchange, Outcome, Side};

use crate::now_ms;

const MIN: u64 = 60_000;
/// $10,000.00 in play-money cents.
const USER_CASH: i64 = 1_000_000;
/// The market maker gets deep pockets and deep inventory.
const MM_CASH: i64 = 100_000_000;

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
        for outcome in [Outcome::Yes, Outcome::No] {
            ex.grant_shares("marketmaker", m.id, outcome, 100_000)
                .expect("mm inventory");
            for who in ["demo", "alice", "bob"] {
                ex.grant_shares(who, m.id, outcome, 500).expect("inventory");
            }
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
}
