//! Trading bots with real, distinct strategies. Each bot decides what
//! orders to place this tick from what it can observe (the top of book, a
//! short window of the tape, its own inventory) plus its private, noisy
//! estimate of the hidden fair value. The game runtime cancels a bot's
//! resting orders before each call, so `decide` returns the full set of
//! orders the bot wants live for the coming tick.
//!
//! Every decision is a pure function of its inputs and the supplied `Rng`,
//! which is what makes the bots testable and a round replayable.

use super::rng::Rng;
use crate::types::{Price, Qty, Side};

/// What a bot sees about the single traded outcome this tick.
#[derive(Clone, Debug, Default)]
pub struct MarketSnapshot {
    pub best_bid: Option<Price>,
    pub best_ask: Option<Price>,
    /// Book midpoint in cents; falls back to the fair estimate when the
    /// book is one-sided or empty (the runtime fills this in).
    pub mid: f64,
    /// Recent execution prices, oldest first.
    pub recent: Vec<Price>,
    /// The bot's net inventory relative to its starting baseline. Positive
    /// means it is long, negative means it is short of where it began.
    pub inventory: i64,
}

impl MarketSnapshot {
    /// Standard deviation of the recent tape, a cheap volatility proxy.
    pub fn volatility(&self) -> f64 {
        if self.recent.len() < 2 {
            return 0.0;
        }
        let n = self.recent.len() as f64;
        let mean = self.recent.iter().map(|&p| p as f64).sum::<f64>() / n;
        let var = self
            .recent
            .iter()
            .map(|&p| (p as f64 - mean).powi(2))
            .sum::<f64>()
            / n;
        var.sqrt()
    }

    /// Signed short-term trend: the last price minus the price `lookback`
    /// trades earlier. Positive means the tape is rising.
    pub fn trend(&self, lookback: usize) -> f64 {
        let n = self.recent.len();
        if n < 2 {
            return 0.0;
        }
        let last = self.recent[n - 1] as f64;
        let idx = n.saturating_sub(1 + lookback);
        last - self.recent[idx] as f64
    }
}

/// One order a bot wants resting (or crossing) this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BotOrder {
    pub side: Side,
    pub price: Price,
    pub qty: Qty,
}

fn clamp_price(p: i64) -> Price {
    p.clamp(1, 99) as Price
}

/// The behavior every bot implements.
pub trait Strategy {
    /// Decide the orders to place given a private fair estimate `est`, the
    /// observable market, and a random source.
    fn decide(&mut self, est: f64, snap: &MarketSnapshot, rng: &mut Rng) -> Vec<BotOrder>;
    /// Short display name shown in the opponents panel.
    fn name(&self) -> &'static str;
    /// One line hinting at the behavior, shown to the player.
    fn hint(&self) -> &'static str;
}

// ---- noise trader --------------------------------------------------------

/// Posts a single order on a random side at a random offset from its (very
/// noisy) idea of fair value. It is not trying to make money; it is the
/// pickable liquidity that everyone else feeds on.
#[derive(Clone, Debug)]
pub struct NoiseTrader {
    pub max_offset: i64,
    pub base_qty: i64,
    pub qty_jitter: i64,
}

impl Strategy for NoiseTrader {
    fn decide(&mut self, est: f64, _snap: &MarketSnapshot, rng: &mut Rng) -> Vec<BotOrder> {
        let side = if rng.chance(0.5) {
            Side::Buy
        } else {
            Side::Sell
        };
        let offset = rng.int(1, self.max_offset);
        let base = est.round() as i64;
        let price = match side {
            Side::Buy => base - offset,
            Side::Sell => base + offset,
        };
        let qty = (self.base_qty + rng.int(0, self.qty_jitter)) as Qty;
        vec![BotOrder {
            side,
            price: clamp_price(price),
            qty,
        }]
    }

    fn name(&self) -> &'static str {
        "Drift"
    }
    fn hint(&self) -> &'static str {
        "posts random orders around a wandering value; free liquidity"
    }
}

// ---- market maker --------------------------------------------------------

/// Quotes both sides around its fair estimate, earning the spread. It
/// widens when the tape is volatile, skews its quotes to shed inventory,
/// and (on the hard tier) pulls back hard when flow looks toxic, which is
/// how it defends against being adversely selected.
#[derive(Clone, Debug)]
pub struct MarketMaker {
    pub base_half_spread: f64,
    pub vol_factor: f64,
    pub skew_factor: f64,
    pub quote_qty: i64,
    pub inventory_limit: i64,
    pub pull_on_toxic: bool,
    pub toxic_vol: f64,
    /// Fixed extra half-spread added when flow looks toxic.
    pub toxic_widen: f64,
    /// Hard cap on the half-spread so it never runs away into a market that
    /// is wide enough to be trivially picked off.
    pub max_half_spread: f64,
}

impl Strategy for MarketMaker {
    fn decide(&mut self, est: f64, snap: &MarketSnapshot, _rng: &mut Rng) -> Vec<BotOrder> {
        // Cap the volatility input so one violent print cannot blow the quote
        // out on its own.
        let vol = snap.volatility().min(8.0);
        let mut half = self.base_half_spread + self.vol_factor * vol;
        // Adverse-selection defense: when the tape is running, back off by a
        // fixed amount rather than fleeing the market entirely.
        if self.pull_on_toxic && vol > self.toxic_vol {
            half += self.toxic_widen;
        }
        half = half.min(self.max_half_spread);
        // Skew both quotes against the current inventory so fills pull the
        // book back toward flat: long inventory lowers both quotes.
        let skew = self.skew_factor * snap.inventory as f64;
        let bid = (est - half - skew).round() as i64;
        let ask = (est + half - skew).round() as i64;
        // Never quote a crossed or locked market against yourself.
        let (bid, ask) = if ask <= bid {
            (bid, bid + 1)
        } else {
            (bid, ask)
        };

        let mut orders = Vec::new();
        if snap.inventory < self.inventory_limit {
            orders.push(BotOrder {
                side: Side::Buy,
                price: clamp_price(bid),
                qty: self.quote_qty as Qty,
            });
        }
        if snap.inventory > -self.inventory_limit {
            orders.push(BotOrder {
                side: Side::Sell,
                price: clamp_price(ask),
                qty: self.quote_qty as Qty,
            });
        }
        orders
    }

    fn name(&self) -> &'static str {
        "Quotefill"
    }
    fn hint(&self) -> &'static str {
        "quotes both sides, widens on volatility, skews to stay flat"
    }
}

// ---- momentum ------------------------------------------------------------

/// Reads a short-term trend off the tape and trades with it, taking
/// liquidity to push price in the direction it is already moving.
#[derive(Clone, Debug)]
pub struct MomentumBot {
    pub lookback: usize,
    pub threshold: f64,
    pub qty: i64,
}

impl Strategy for MomentumBot {
    fn decide(&mut self, est: f64, snap: &MarketSnapshot, _rng: &mut Rng) -> Vec<BotOrder> {
        let t = snap.trend(self.lookback);
        if t > self.threshold {
            // Rising: cross up to lift resting asks.
            let price = snap
                .best_ask
                .map(|a| a as i64 + 1)
                .unwrap_or(est.round() as i64 + 1);
            vec![BotOrder {
                side: Side::Buy,
                price: clamp_price(price),
                qty: self.qty as Qty,
            }]
        } else if t < -self.threshold {
            // Falling: cross down to hit resting bids.
            let price = snap
                .best_bid
                .map(|b| b as i64 - 1)
                .unwrap_or(est.round() as i64 - 1);
            vec![BotOrder {
                side: Side::Sell,
                price: clamp_price(price),
                qty: self.qty as Qty,
            }]
        } else {
            vec![]
        }
    }

    fn name(&self) -> &'static str {
        "Chaser"
    }
    fn hint(&self) -> &'static str {
        "follows short-term trends and runs price in their direction"
    }
}

// ---- mean reversion ------------------------------------------------------

/// Fades moves away from its own moving average of the mid: sells when
/// price runs above the average, buys when it runs below. Supplies the
/// counter-flow that keeps a quiet market anchored.
#[derive(Clone, Debug)]
pub struct MeanReversionBot {
    pub alpha: f64,
    pub threshold: f64,
    pub qty: i64,
    ema: Option<f64>,
}

impl MeanReversionBot {
    pub fn new(alpha: f64, threshold: f64, qty: i64) -> Self {
        MeanReversionBot {
            alpha,
            threshold,
            qty,
            ema: None,
        }
    }

    /// The moving average as it currently stands (for tests and display).
    pub fn average(&self) -> Option<f64> {
        self.ema
    }
}

impl Strategy for MeanReversionBot {
    fn decide(&mut self, _est: f64, snap: &MarketSnapshot, _rng: &mut Rng) -> Vec<BotOrder> {
        let price = snap.mid;
        let ema = match self.ema {
            Some(e) => e + self.alpha * (price - e),
            None => price,
        };
        self.ema = Some(ema);
        let dev = price - ema;
        if dev > self.threshold {
            // Above average: sell into the strength, hitting the bid.
            let p = snap
                .best_bid
                .map(|b| b as i64)
                .unwrap_or((price - 1.0) as i64);
            vec![BotOrder {
                side: Side::Sell,
                price: clamp_price(p),
                qty: self.qty as Qty,
            }]
        } else if dev < -self.threshold {
            let p = snap
                .best_ask
                .map(|a| a as i64)
                .unwrap_or((price + 1.0) as i64);
            vec![BotOrder {
                side: Side::Buy,
                price: clamp_price(p),
                qty: self.qty as Qty,
            }]
        } else {
            vec![]
        }
    }

    fn name(&self) -> &'static str {
        "Fade"
    }
    fn hint(&self) -> &'static str {
        "sells rallies and buys dips against its moving average"
    }
}

// ---- informed / adversarial ---------------------------------------------

/// Knows the fair value almost exactly and sweeps any resting order priced
/// on the wrong side of it. A buy limit at `floor(true - edge)` lifts every
/// ask that is too cheap; a sell limit at `ceil(true + edge)` hits every bid
/// that is too rich. This is the adverse selection that punishes stale
/// quotes on the hard tier, the player's included.
#[derive(Clone, Debug)]
pub struct InformedBot {
    pub edge: f64,
    pub qty: i64,
}

impl Strategy for InformedBot {
    fn decide(&mut self, est: f64, snap: &MarketSnapshot, _rng: &mut Rng) -> Vec<BotOrder> {
        let true_v = est; // the runtime hands this bot a near-exact estimate
        let mut orders = Vec::new();

        let buy_limit = (true_v - self.edge).floor() as i64;
        if let Some(ask) = snap.best_ask {
            if (ask as i64) <= buy_limit {
                orders.push(BotOrder {
                    side: Side::Buy,
                    price: clamp_price(buy_limit),
                    qty: self.qty as Qty,
                });
            }
        }

        let sell_limit = (true_v + self.edge).ceil() as i64;
        if let Some(bid) = snap.best_bid {
            if (bid as i64) >= sell_limit {
                orders.push(BotOrder {
                    side: Side::Sell,
                    price: clamp_price(sell_limit),
                    qty: self.qty as Qty,
                });
            }
        }
        orders
    }

    fn name(&self) -> &'static str {
        "Sniper"
    }
    fn hint(&self) -> &'static str {
        "knows fair value and picks off any stale quote on the wrong side"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_trader_stays_within_its_offset_and_is_deterministic() {
        let mut a = NoiseTrader {
            max_offset: 5,
            base_qty: 40,
            qty_jitter: 60,
        };
        let mut b = a.clone();
        let snap = MarketSnapshot::default();
        for seed in 0..200u64 {
            let mut r1 = Rng::seed(seed);
            let mut r2 = Rng::seed(seed);
            let oa = a.decide(50.0, &snap, &mut r1);
            let ob = b.decide(50.0, &snap, &mut r2);
            assert_eq!(oa, ob, "same seed must give the same order");
            let o = oa[0];
            let dist = (o.price as i64 - 50).abs();
            assert!((1..=5).contains(&dist), "offset out of range: {dist}");
            assert!((40..=100).contains(&(o.qty as i64)), "qty out of range");
        }
    }

    #[test]
    fn market_maker_brackets_fair_and_widens_on_volatility() {
        let mut mm = MarketMaker {
            base_half_spread: 2.0,
            vol_factor: 1.0,
            skew_factor: 0.05,
            quote_qty: 100,
            inventory_limit: 5000,
            pull_on_toxic: false,
            toxic_vol: 3.0,
            toxic_widen: 3.0,
            max_half_spread: 20.0,
        };
        let mut r = Rng::seed(1);
        let quiet = MarketSnapshot {
            mid: 50.0,
            inventory: 0,
            ..Default::default()
        };
        let q = mm.decide(50.0, &quiet, &mut r);
        let bid = q.iter().find(|o| o.side == Side::Buy).unwrap().price;
        let ask = q.iter().find(|o| o.side == Side::Sell).unwrap().price;
        assert!(bid < 50 && ask > 50, "quotes must bracket fair");
        let quiet_spread = ask - bid;

        let noisy = MarketSnapshot {
            mid: 50.0,
            inventory: 0,
            recent: vec![40, 60, 42, 58, 45, 55],
            ..Default::default()
        };
        let q2 = mm.decide(50.0, &noisy, &mut r);
        let bid2 = q2.iter().find(|o| o.side == Side::Buy).unwrap().price;
        let ask2 = q2.iter().find(|o| o.side == Side::Sell).unwrap().price;
        assert!(
            ask2 - bid2 > quiet_spread,
            "a volatile tape must widen the quote"
        );
    }

    #[test]
    fn market_maker_skews_quotes_down_when_long() {
        let mut mm = MarketMaker {
            base_half_spread: 2.0,
            vol_factor: 0.0,
            skew_factor: 0.1,
            quote_qty: 100,
            inventory_limit: 100_000,
            pull_on_toxic: false,
            toxic_vol: 3.0,
            toxic_widen: 3.0,
            max_half_spread: 20.0,
        };
        let mut r = Rng::seed(1);
        let flat = mm.decide(
            50.0,
            &MarketSnapshot {
                mid: 50.0,
                inventory: 0,
                ..Default::default()
            },
            &mut r,
        );
        let long = mm.decide(
            50.0,
            &MarketSnapshot {
                mid: 50.0,
                inventory: 200,
                ..Default::default()
            },
            &mut r,
        );
        let flat_ask = flat.iter().find(|o| o.side == Side::Sell).unwrap().price;
        let long_ask = long.iter().find(|o| o.side == Side::Sell).unwrap().price;
        assert!(long_ask < flat_ask, "being long must cheapen the ask");
    }

    #[test]
    fn toxic_market_maker_pulls_wider_than_a_calm_one() {
        let noisy = MarketSnapshot {
            mid: 50.0,
            inventory: 0,
            recent: vec![40, 60, 40, 60, 40, 60],
            ..Default::default()
        };
        let mut calm = MarketMaker {
            base_half_spread: 1.0,
            vol_factor: 0.5,
            skew_factor: 0.0,
            quote_qty: 100,
            inventory_limit: 5000,
            pull_on_toxic: false,
            toxic_vol: 3.0,
            toxic_widen: 3.0,
            max_half_spread: 20.0,
        };
        let mut toxic = MarketMaker {
            pull_on_toxic: true,
            ..calm.clone()
        };
        let mut r = Rng::seed(1);
        let calm_q = calm.decide(50.0, &noisy, &mut r);
        let toxic_q = toxic.decide(50.0, &noisy, &mut r);
        let span = |q: &[BotOrder]| {
            let b = q.iter().find(|o| o.side == Side::Buy).unwrap().price;
            let a = q.iter().find(|o| o.side == Side::Sell).unwrap().price;
            a - b
        };
        assert!(
            span(&toxic_q) > span(&calm_q),
            "toxic flow must make it pull wider"
        );
    }

    #[test]
    fn momentum_buys_uptrends_sells_downtrends_and_holds_when_flat() {
        let mut m = MomentumBot {
            lookback: 4,
            threshold: 2.0,
            qty: 50,
        };
        let mut r = Rng::seed(1);
        let up = MarketSnapshot {
            recent: vec![40, 42, 45, 48, 52],
            best_ask: Some(53),
            best_bid: Some(51),
            mid: 52.0,
            ..Default::default()
        };
        let down = MarketSnapshot {
            recent: vec![60, 58, 55, 52, 48],
            best_ask: Some(49),
            best_bid: Some(47),
            mid: 48.0,
            ..Default::default()
        };
        let flat = MarketSnapshot {
            recent: vec![50, 50, 51, 50, 50],
            best_ask: Some(51),
            best_bid: Some(49),
            mid: 50.0,
            ..Default::default()
        };
        assert_eq!(m.decide(52.0, &up, &mut r)[0].side, Side::Buy);
        assert_eq!(m.decide(48.0, &down, &mut r)[0].side, Side::Sell);
        assert!(m.decide(50.0, &flat, &mut r).is_empty());
    }

    #[test]
    fn mean_reversion_fades_a_run_above_its_average() {
        let mut f = MeanReversionBot::new(0.2, 1.0, 50);
        let mut r = Rng::seed(1);
        // Warm the average up around 50.
        for _ in 0..20 {
            let snap = MarketSnapshot {
                mid: 50.0,
                best_bid: Some(49),
                best_ask: Some(51),
                ..Default::default()
            };
            f.decide(50.0, &snap, &mut r);
        }
        // Now a spike well above the average must be sold.
        let spike = MarketSnapshot {
            mid: 60.0,
            best_bid: Some(59),
            best_ask: Some(61),
            ..Default::default()
        };
        let out = f.decide(60.0, &spike, &mut r);
        assert_eq!(out[0].side, Side::Sell);
    }

    #[test]
    fn informed_bot_lifts_cheap_asks_and_hits_rich_bids() {
        let mut bot = InformedBot {
            edge: 1.0,
            qty: 100,
        };
        let mut r = Rng::seed(1);

        // True value 60: an ask at 55 is far too cheap and must be lifted.
        let cheap_ask = MarketSnapshot {
            best_ask: Some(55),
            best_bid: Some(53),
            mid: 54.0,
            ..Default::default()
        };
        let out = bot.decide(60.0, &cheap_ask, &mut r);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].side, Side::Buy);
        assert!(out[0].price >= 55, "buy limit must reach the cheap ask");

        // True value 40: a bid at 46 is far too rich and must be hit.
        let rich_bid = MarketSnapshot {
            best_ask: Some(48),
            best_bid: Some(46),
            mid: 47.0,
            ..Default::default()
        };
        let out = bot.decide(40.0, &rich_bid, &mut r);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].side, Side::Sell);
        assert!(out[0].price <= 46, "sell limit must reach the rich bid");
    }

    #[test]
    fn informed_bot_leaves_a_fairly_priced_book_alone() {
        let mut bot = InformedBot {
            edge: 1.0,
            qty: 100,
        };
        let mut r = Rng::seed(1);
        let fair = MarketSnapshot {
            best_ask: Some(51),
            best_bid: Some(49),
            mid: 50.0,
            ..Default::default()
        };
        assert!(bot.decide(50.0, &fair, &mut r).is_empty());
    }
}
