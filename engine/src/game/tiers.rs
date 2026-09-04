//! Difficulty tiers. A tier is a recipe: how fast the clock ticks, how the
//! hidden fair value behaves, and which bots are in the pit. The three tiers
//! differ in ways that change how hard it is to make money, not just in
//! cosmetic numbers.
//!
//! - Easy: mostly noise traders and one lazy, wide market maker on a slow
//!   clock. Basic two-sided quoting captures the spread.
//! - Medium: a tighter market maker plus a momentum chaser and a mean-
//!   reversion fader on a faster clock. You need an actual edge.
//! - Hard: tight makers that pull on toxic flow, a chaser, and an informed
//!   sniper that knows fair value and picks off stale quotes. Lazy quoting
//!   gets adversely selected into the ground.

use super::bots::{InformedBot, MarketMaker, MeanReversionBot, MomentumBot, NoiseTrader, Strategy};
use super::fair::FairValueConfig;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Easy,
    Medium,
    Hard,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Easy => "easy",
            Tier::Medium => "medium",
            Tier::Hard => "hard",
        }
    }

    pub fn parse(s: &str) -> Option<Tier> {
        match s.to_ascii_lowercase().as_str() {
            "easy" => Some(Tier::Easy),
            "medium" => Some(Tier::Medium),
            "hard" => Some(Tier::Hard),
            _ => None,
        }
    }
}

/// One bot in a round: its account, its strategy, how noisy its private
/// fair-value estimate is, and the share inventory it starts with (its
/// inventory skew is measured against this baseline).
pub struct BotSpec {
    pub account: String,
    pub strategy: Box<dyn Strategy + Send + Sync>,
    pub fair_noise: f64,
    pub baseline_shares: i64,
}

/// Everything the runtime needs to run one round of a tier.
pub struct TierConfig {
    pub tier: Tier,
    pub tick_ms: u64,
    pub fair: FairValueConfig,
    pub bots: Vec<BotSpec>,
}

const BOT_SHARES: i64 = 5_000_000;

fn fair(start: f64, drift: f64, sigma: f64, jump_prob: f64, jump_sigma: f64) -> FairValueConfig {
    FairValueConfig {
        start,
        drift,
        sigma,
        jump_prob,
        jump_sigma,
        min: 1.0,
        max: 99.0,
    }
}

fn spec(account: &str, strategy: Box<dyn Strategy + Send + Sync>, fair_noise: f64) -> BotSpec {
    BotSpec {
        account: account.to_string(),
        strategy,
        fair_noise,
        baseline_shares: BOT_SHARES,
    }
}

/// Build the configuration for a tier. The fair value starts at a fixed 50c
/// so the player is not handed a free directional read at the open.
pub fn config(tier: Tier) -> TierConfig {
    match tier {
        Tier::Easy => TierConfig {
            tier,
            tick_ms: 1100,
            fair: fair(50.0, 0.0, 0.5, 0.01, 6.0),
            bots: vec![
                spec(
                    "bot-drift-1",
                    Box::new(NoiseTrader {
                        max_offset: 6,
                        base_qty: 40,
                        qty_jitter: 80,
                    }),
                    5.0,
                ),
                spec(
                    "bot-drift-2",
                    Box::new(NoiseTrader {
                        max_offset: 6,
                        base_qty: 40,
                        qty_jitter: 80,
                    }),
                    5.0,
                ),
                spec(
                    "bot-drift-3",
                    Box::new(NoiseTrader {
                        max_offset: 5,
                        base_qty: 30,
                        qty_jitter: 60,
                    }),
                    4.0,
                ),
                spec(
                    "bot-quotefill-1",
                    Box::new(MarketMaker {
                        base_half_spread: 4.0,
                        vol_factor: 0.4,
                        skew_factor: 0.02,
                        quote_qty: 120,
                        inventory_limit: 20_000,
                        pull_on_toxic: false,
                        toxic_vol: 6.0,
                        toxic_widen: 2.0,
                        max_half_spread: 8.0,
                    }),
                    2.5,
                ),
            ],
        },
        Tier::Medium => TierConfig {
            tier,
            tick_ms: 700,
            fair: fair(50.0, 0.0, 0.6, 0.015, 6.0),
            bots: vec![
                spec(
                    "bot-drift-1",
                    Box::new(NoiseTrader {
                        max_offset: 5,
                        base_qty: 30,
                        qty_jitter: 60,
                    }),
                    4.0,
                ),
                spec(
                    "bot-drift-2",
                    Box::new(NoiseTrader {
                        max_offset: 4,
                        base_qty: 30,
                        qty_jitter: 50,
                    }),
                    3.5,
                ),
                spec(
                    "bot-quotefill-1",
                    Box::new(MarketMaker {
                        base_half_spread: 2.0,
                        vol_factor: 0.4,
                        skew_factor: 0.03,
                        quote_qty: 150,
                        inventory_limit: 15_000,
                        pull_on_toxic: false,
                        toxic_vol: 4.0,
                        toxic_widen: 2.0,
                        max_half_spread: 6.0,
                    }),
                    1.5,
                ),
                spec(
                    "bot-chaser-1",
                    Box::new(MomentumBot {
                        lookback: 5,
                        threshold: 2.0,
                        qty: 90,
                    }),
                    2.0,
                ),
                spec(
                    "bot-fade-1",
                    Box::new(MeanReversionBot::new(0.3, 3.0, 80)),
                    2.0,
                ),
            ],
        },
        Tier::Hard => TierConfig {
            tier,
            tick_ms: 400,
            fair: fair(50.0, 0.0, 0.9, 0.03, 8.0),
            bots: vec![
                spec(
                    "bot-drift-1",
                    Box::new(NoiseTrader {
                        max_offset: 4,
                        base_qty: 25,
                        qty_jitter: 40,
                    }),
                    3.0,
                ),
                spec(
                    "bot-quotefill-1",
                    Box::new(MarketMaker {
                        base_half_spread: 1.0,
                        vol_factor: 0.25,
                        skew_factor: 0.04,
                        quote_qty: 180,
                        inventory_limit: 12_000,
                        pull_on_toxic: true,
                        toxic_vol: 2.5,
                        toxic_widen: 2.0,
                        max_half_spread: 4.0,
                    }),
                    0.8,
                ),
                spec(
                    "bot-quotefill-2",
                    Box::new(MarketMaker {
                        base_half_spread: 1.0,
                        vol_factor: 0.3,
                        skew_factor: 0.05,
                        quote_qty: 150,
                        inventory_limit: 10_000,
                        pull_on_toxic: true,
                        toxic_vol: 2.5,
                        toxic_widen: 2.0,
                        max_half_spread: 5.0,
                    }),
                    0.8,
                ),
                spec(
                    "bot-chaser-1",
                    Box::new(MomentumBot {
                        lookback: 4,
                        threshold: 1.5,
                        qty: 90,
                    }),
                    1.2,
                ),
                spec(
                    "bot-sniper-1",
                    Box::new(InformedBot {
                        edge: 1.0,
                        qty: 120,
                    }),
                    0.15,
                ),
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(cfg: &TierConfig) -> Vec<&'static str> {
        cfg.bots.iter().map(|b| b.strategy.name()).collect()
    }

    #[test]
    fn only_hard_fields_the_sniper() {
        assert!(!names(&config(Tier::Easy)).contains(&"Sniper"));
        assert!(!names(&config(Tier::Medium)).contains(&"Sniper"));
        assert!(names(&config(Tier::Hard)).contains(&"Sniper"));
    }

    #[test]
    fn easy_is_mostly_noise_and_has_no_momentum() {
        let n = names(&config(Tier::Easy));
        assert!(n.iter().filter(|&&x| x == "Drift").count() >= 3);
        assert!(!n.contains(&"Chaser"));
    }

    #[test]
    fn harder_tiers_tick_faster() {
        let e = config(Tier::Easy).tick_ms;
        let m = config(Tier::Medium).tick_ms;
        let h = config(Tier::Hard).tick_ms;
        assert!(e > m && m > h, "easy {e} medium {m} hard {h}");
    }

    #[test]
    fn harder_tiers_move_the_fair_value_more() {
        let e = config(Tier::Easy).fair.sigma;
        let m = config(Tier::Medium).fair.sigma;
        let h = config(Tier::Hard).fair.sigma;
        assert!(h > m && m > e, "sigma should grow with difficulty");
    }

    #[test]
    fn every_tier_has_a_market_maker() {
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            assert!(
                names(&config(tier)).contains(&"Quotefill"),
                "{tier:?} needs a maker"
            );
        }
    }

    #[test]
    fn tier_string_round_trips() {
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            assert_eq!(Tier::parse(tier.as_str()), Some(tier));
        }
        assert_eq!(Tier::parse("nope"), None);
    }
}
