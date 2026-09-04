//! The game layer on top of the matching engine: a hidden fair-value
//! process, a set of bots with real strategies, difficulty tiers, and the
//! scoring for a finished round. Everything here is pure and deterministic
//! given a seed, which is what lets the gateway run a live round on top of
//! the same engine while the tests pin every decision.

pub mod bots;
pub mod fair;
pub mod rng;
pub mod scoring;
pub mod tiers;

pub use bots::{
    BotOrder, InformedBot, MarketMaker, MarketSnapshot, MeanReversionBot, MomentumBot, NoiseTrader,
    Strategy,
};
pub use fair::{FairValue, FairValueConfig};
pub use rng::Rng;
pub use scoring::{max_drawdown, score, sharpe, Scorecard};
pub use tiers::{config as tier_config, BotSpec, Tier, TierConfig};
