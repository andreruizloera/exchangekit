//! What happens when a binary market resolves.
//!
//! This module is pure: it knows about holdings, outcomes, and cents, and
//! nothing about books, orders, or the [`Exchange`](crate::Exchange) that
//! calls it. The state mutation lives in `exchange.rs`; the arithmetic and
//! the funding rule live here so both can be tested on their own.
//!
//! A binary contract settles at 0 or 100 cents. When a market resolves to
//! an outcome, every share of that outcome pays [`SHARE_PAYOUT`] cents and
//! every share of the other outcome pays nothing.

use serde::{Deserialize, Serialize};

use crate::types::{Cash, Outcome, Qty};

/// What one winning share pays at settlement, in play-money cents.
pub const SHARE_PAYOUT: Cash = 100;

/// A market's terminal state: which outcome won, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub outcome: Outcome,
    /// Unix milliseconds.
    pub resolved_at: u64,
}

/// One account's shares in a market that is about to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    pub account: String,
    pub yes: Qty,
    pub no: Qty,
}

/// What one account received when a market resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payout {
    pub account: String,
    /// Shares of the winning outcome, each paid [`SHARE_PAYOUT`] cents.
    pub winning_shares: Qty,
    /// Shares of the losing outcome, which paid nothing and were cleared.
    pub losing_shares: Qty,
    pub paid: Cash,
}

/// The full report of a resolution: who was paid, what was voided, and
/// whether the payout was funded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    pub market: String,
    /// The winning outcome.
    pub outcome: Outcome,
    /// Unix milliseconds.
    pub resolved_at: u64,
    /// Per-account payouts, ordered by account id. Accounts holding no
    /// shares of either outcome are omitted.
    pub payouts: Vec<Payout>,
    pub total_paid: Cash,
    pub winning_shares: Qty,
    pub losing_shares: Qty,
    /// Resting orders cancelled by the resolution, across both books.
    pub orders_voided: usize,
    /// Escrow released by voiding those orders: cash held against resting
    /// buys, and shares locked by resting sells.
    pub cash_released: Cash,
    pub shares_released: Qty,
    /// Cents this market held as collateral when it resolved. Minting a
    /// YES/NO pair adds [`SHARE_PAYOUT`] cents here.
    pub collateral: Cash,
    /// The part of `total_paid` that no collateral stood behind. Zero when
    /// every share was minted as a pair; positive when shares were granted
    /// with [`grant_shares`](crate::Exchange::grant_shares), because a
    /// granted share has nothing behind it and settling it creates cash.
    pub unbacked_cash: Cash,
}

/// Split each holding into winning and losing shares and price them.
///
/// Holdings with no shares on either side are dropped, so an account that
/// once traded the market but ended flat is not listed with a zero payout.
/// Order is preserved, so a caller iterating accounts in id order gets a
/// report in id order.
pub fn compute_payouts(holdings: &[Holding], winner: Outcome) -> Vec<Payout> {
    holdings
        .iter()
        .filter(|h| h.yes > 0 || h.no > 0)
        .map(|h| {
            let (winning_shares, losing_shares) = match winner {
                Outcome::Yes => (h.yes, h.no),
                Outcome::No => (h.no, h.yes),
            };
            Payout {
                account: h.account.clone(),
                winning_shares,
                losing_shares,
                paid: winning_shares as Cash * SHARE_PAYOUT,
            }
        })
        .collect()
}

/// How much of a payout no collateral stood behind.
///
/// This is the honest half of resolution. Cash is conserved by trading: a
/// buyer's cents become a seller's cents. Settlement is different, because
/// it pays out against shares, and a share only carries its own funding if
/// it was minted as a YES/NO pair against 100 cents of collateral. Shares
/// created by `grant_shares` carry nothing, so settling them creates cash
/// out of nothing. The engine does not forbid that (the demo seed and the
/// game both need free inventory), but it refuses to hide it: the number
/// is computed and reported on every settlement.
pub fn unbacked(total_paid: Cash, collateral: Cash) -> Cash {
    (total_paid - collateral).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(account: &str, yes: Qty, no: Qty) -> Holding {
        Holding {
            account: account.to_string(),
            yes,
            no,
        }
    }

    #[test]
    fn winning_shares_pay_one_dollar_and_losing_shares_pay_nothing() {
        let payouts = compute_payouts(&[h("alice", 10, 4)], Outcome::Yes);
        assert_eq!(payouts.len(), 1);
        assert_eq!(payouts[0].winning_shares, 10);
        assert_eq!(payouts[0].losing_shares, 4);
        assert_eq!(payouts[0].paid, 1_000);
    }

    #[test]
    fn the_winning_side_flips_with_the_outcome() {
        let holdings = [h("alice", 10, 4)];
        let yes = compute_payouts(&holdings, Outcome::Yes);
        let no = compute_payouts(&holdings, Outcome::No);
        assert_eq!(yes[0].paid, 1_000);
        assert_eq!(no[0].paid, 400);
        assert_eq!(no[0].winning_shares, 4);
    }

    #[test]
    fn holders_of_only_the_losing_side_are_listed_with_a_zero_payout() {
        let payouts = compute_payouts(&[h("bob", 0, 7)], Outcome::Yes);
        assert_eq!(payouts.len(), 1, "bob held shares, so he is in the report");
        assert_eq!(payouts[0].paid, 0);
        assert_eq!(payouts[0].losing_shares, 7);
    }

    #[test]
    fn flat_accounts_are_left_out_of_the_report() {
        let payouts = compute_payouts(&[h("alice", 5, 0), h("ghost", 0, 0)], Outcome::Yes);
        let accounts: Vec<&str> = payouts.iter().map(|p| p.account.as_str()).collect();
        assert_eq!(accounts, vec!["alice"]);
    }

    #[test]
    fn payout_order_follows_the_holdings_it_was_given() {
        let payouts = compute_payouts(&[h("zoe", 1, 0), h("adam", 1, 0)], Outcome::Yes);
        let accounts: Vec<&str> = payouts.iter().map(|p| p.account.as_str()).collect();
        assert_eq!(accounts, vec!["zoe", "adam"]);
    }

    #[test]
    fn a_fully_collateralized_payout_is_entirely_backed() {
        // 12 pairs minted: 1,200 cents of collateral, 12 winning shares.
        assert_eq!(unbacked(1_200, 1_200), 0);
    }

    #[test]
    fn granted_shares_show_up_as_unbacked_cash() {
        // 12 pairs plus 3 granted winning shares: 1,500 paid, 1,200 backed.
        assert_eq!(unbacked(1_500, 1_200), 300);
    }

    #[test]
    fn surplus_collateral_never_reads_as_negative_unbacked_cash() {
        assert_eq!(unbacked(400, 1_000), 0);
    }
}
