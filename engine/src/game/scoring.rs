//! Scoring for a finished round. Everything is computed from the player's
//! mark-to-market equity curve (one sample per tick) plus a trade count, so
//! the numbers are ordinary finance rather than a made-up score.

use serde::{Deserialize, Serialize};

/// Maximum peak-to-trough decline of an equity curve, as a fraction in
/// [0, 1]. A curve that only ever rises has a drawdown of 0.
pub fn max_drawdown(equity: &[i64]) -> f64 {
    let mut peak = f64::MIN;
    let mut mdd = 0.0;
    for &e in equity {
        let e = e as f64;
        if e > peak {
            peak = e;
        }
        if peak > 0.0 {
            let dd = (peak - e) / peak;
            if dd > mdd {
                mdd = dd;
            }
        }
    }
    mdd
}

/// Per-round Sharpe ratio: the mean per-tick return divided by its standard
/// deviation, scaled by the square root of the number of returns so the
/// figure reads on the familiar Sharpe scale rather than as a tiny per-tick
/// number. A flat curve, or one return, scores 0.
pub fn sharpe(equity: &[i64]) -> f64 {
    if equity.len() < 2 {
        return 0.0;
    }
    let rets: Vec<f64> = equity
        .windows(2)
        .map(|w| {
            let prev = w[0] as f64;
            if prev == 0.0 {
                0.0
            } else {
                (w[1] as f64 - prev) / prev
            }
        })
        .collect();
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd == 0.0 {
        0.0
    } else {
        mean / sd * n.sqrt()
    }
}

/// The full result of a round.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scorecard {
    /// Mark-to-market equity at the first and last tick, in cents.
    pub starting_equity: i64,
    pub final_equity: i64,
    /// final minus starting equity, in cents.
    pub pnl: i64,
    /// PnL as a percentage of starting equity.
    pub return_pct: f64,
    pub sharpe: f64,
    /// Worst peak-to-trough decline over the round, as a fraction in [0, 1].
    pub max_drawdown: f64,
    /// Number of trades the player was a party to.
    pub trades: u32,
    /// Number of equity samples (ticks) in the round.
    pub ticks: u32,
}

/// Build a scorecard from an equity curve (cents, one sample per tick) and
/// the player's trade count.
pub fn score(equity: &[i64], trades: u32) -> Scorecard {
    let starting_equity = *equity.first().unwrap_or(&0);
    let final_equity = *equity.last().unwrap_or(&0);
    let pnl = final_equity - starting_equity;
    let return_pct = if starting_equity != 0 {
        pnl as f64 / starting_equity as f64 * 100.0
    } else {
        0.0
    };
    Scorecard {
        starting_equity,
        final_equity,
        pnl,
        return_pct,
        sharpe: sharpe(equity),
        max_drawdown: max_drawdown(equity),
        trades,
        ticks: equity.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawdown_of_a_known_curve() {
        // Peak 120, trough 60: (120 - 60) / 120 = 0.5.
        assert!((max_drawdown(&[100, 120, 90, 110, 60]) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn drawdown_is_zero_for_a_monotone_curve() {
        assert_eq!(max_drawdown(&[100, 120, 120, 130]), 0.0);
        assert_eq!(max_drawdown(&[100]), 0.0);
    }

    #[test]
    fn sharpe_of_a_flat_curve_is_zero() {
        assert_eq!(sharpe(&[100, 100, 100]), 0.0);
        // Constant positive growth rate has zero return variance too.
        assert_eq!(sharpe(&[100, 110, 121]), 0.0);
        assert_eq!(sharpe(&[100]), 0.0);
    }

    #[test]
    fn sharpe_of_a_known_curve() {
        // Returns are 0.2 then 0.0: mean 0.1, population sd 0.1, two returns.
        // sharpe = 0.1 / 0.1 * sqrt(2) = sqrt(2).
        let s = sharpe(&[100, 120, 120]);
        assert!((s - std::f64::consts::SQRT_2).abs() < 1e-9, "got {s}");
    }

    #[test]
    fn score_reports_pnl_and_return() {
        let card = score(&[100_000, 105_000, 110_000], 7);
        assert_eq!(card.starting_equity, 100_000);
        assert_eq!(card.final_equity, 110_000);
        assert_eq!(card.pnl, 10_000);
        assert!((card.return_pct - 10.0).abs() < 1e-9);
        assert_eq!(card.trades, 7);
        assert_eq!(card.ticks, 3);
        assert_eq!(card.max_drawdown, 0.0);
    }

    #[test]
    fn score_handles_a_losing_round() {
        let card = score(&[100_000, 80_000], 3);
        assert_eq!(card.pnl, -20_000);
        assert!((card.return_pct + 20.0).abs() < 1e-9);
        assert!((card.max_drawdown - 0.2).abs() < 1e-12);
    }
}
