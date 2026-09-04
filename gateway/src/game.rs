//! The live game runtime. A round runs on top of the same matching engine
//! the rest of the gateway uses: a background task advances a hidden fair
//! value on the tier's clock, and on every tick each bot cancels its resting
//! orders and requotes through the real `place_order` path. The player trades
//! through the ordinary order endpoints against the same book. At the end the
//! round is scored from the player's mark-to-market equity curve.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use exchangekit_engine::game::{
    score, tier_config, FairValue, MarketSnapshot, Scorecard, Strategy, Tier,
};
use exchangekit_engine::{Exchange, Outcome, Price};
use serde_json::{json, Value};

use crate::{now_ms, AppState};

/// Player starting cash in play-money cents ($1,000.00).
const STARTING_CASH: i64 = 100_000;
/// Player starting inventory of the traded asset.
const STARTING_SHARES: u64 = 1_000;
/// Bots get deep pockets so they never run out mid-round.
const BOT_CASH: i64 = 1_000_000_000;
/// How many recent prints a bot looks at for trend and volatility.
const RECENT_WINDOW: usize = 12;
/// Round length bounds in seconds.
const MIN_DURATION: u64 = 60;
const MAX_DURATION: u64 = 600;
pub const DEFAULT_DURATION: u64 = 180;

/// The traded outcome. A game round trades a single synthetic asset, which
/// maps onto the YES book of a throwaway market.
const OUTCOME: Outcome = Outcome::Yes;

/// One bot instance for a running round.
pub struct BotRuntime {
    pub account: String,
    pub strategy: Box<dyn Strategy + Send + Sync>,
    pub fair_noise: f64,
    pub baseline: i64,
}

pub enum GameStatus {
    Running,
    Ended,
}

/// A running (or just-finished) round.
pub struct GameSession {
    pub generation: u64,
    pub tier: Tier,
    pub market: String,
    pub player: String,
    pub started_ms: u64,
    pub duration_secs: u64,
    pub tick_ms: u64,
    pub rng: exchangekit_engine::game::Rng,
    pub fair: FairValue,
    pub bots: Vec<BotRuntime>,
    pub equity_curve: Vec<i64>,
    pub starting_equity: i64,
    pub status: GameStatus,
    pub scorecard: Option<Scorecard>,
    pub final_fair: Option<Price>,
}

fn player_shares(ex: &Exchange, market: &str, account: &str) -> i64 {
    ex.account(account)
        .and_then(|a| a.positions.get(market))
        .map(|mp| mp.get(OUTCOME).quantity)
        .unwrap_or(0)
}

/// Mark-to-market equity in cents: cash plus inventory valued at `mark`.
fn equity(ex: &Exchange, market: &str, account: &str, mark: Price) -> i64 {
    let cash = ex.account(account).map(|a| a.balance).unwrap_or(0);
    cash + player_shares(ex, market, account) * mark as i64
}

/// The price used to mark inventory: the book midpoint when both sides are
/// quoted (the most honest read of where the asset trades right now), falling
/// back to the last print and then to the fair value. Marking at the mid
/// keeps the equity curve from lurching on every one-sided sweep.
fn mark(ex: &Exchange, market: &str, fallback: Price) -> Price {
    if let Ok(book) = ex.book_view(market, OUTCOME, 1) {
        if let (Some(bid), Some(ask)) = (book.bids.first(), book.asks.first()) {
            return ((bid.price + ask.price) / 2).clamp(1, 99);
        }
    }
    ex.price_estimate(market, OUTCOME).unwrap_or(fallback)
}

/// Build a bot's view of the market. `fallback_mid` is used when the book is
/// empty so the bot still has a sensible reference.
fn snapshot(
    ex: &Exchange,
    market: &str,
    account: &str,
    baseline: i64,
    fallback_mid: f64,
) -> MarketSnapshot {
    let book = ex.book_view(market, OUTCOME, 1).ok();
    let best_bid = book.as_ref().and_then(|b| b.bids.first().map(|l| l.price));
    let best_ask = book.as_ref().and_then(|b| b.asks.first().map(|l| l.price));
    let mid = match (best_bid, best_ask) {
        (Some(b), Some(a)) => (b as f64 + a as f64) / 2.0,
        (Some(b), None) => b as f64,
        (None, Some(a)) => a as f64,
        (None, None) => fallback_mid,
    };
    // recent_trades is newest-first; the bots want oldest-first.
    let recent: Vec<Price> = ex
        .recent_trades(market, RECENT_WINDOW)
        .map(|ts| ts.iter().rev().map(|t| t.price).collect())
        .unwrap_or_default();
    let inventory = player_shares(ex, market, account) - baseline;
    MarketSnapshot {
        best_bid,
        best_ask,
        mid,
        recent,
        inventory,
    }
}

fn count_player_trades(ex: &Exchange, market: &str, player: &str) -> u32 {
    ex.recent_trades(market, 1_000_000)
        .map(|ts| {
            ts.iter()
                .filter(|t| t.buyer == player || t.seller == player)
                .count() as u32
        })
        .unwrap_or(0)
}

/// Start a fresh round, replacing any round in progress. Returns the new
/// generation and market id.
pub fn start(state: &Arc<AppState>, tier: Tier, duration_secs: u64, seed: u64) -> (u64, String) {
    let duration_secs = duration_secs.clamp(MIN_DURATION, MAX_DURATION);
    let generation = state.game_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let round = state.game_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let market = format!("game-{round}");

    let cfg = tier_config(tier);
    let fair = FairValue::new(cfg.fair);
    let start_mark = fair.cents();

    let mut bots = Vec::new();
    {
        let mut ex = state.exchange.write().expect("lock");
        let now = now_ms();
        ex.create_market(
            &market,
            "Synthetic asset with a hidden fair value",
            "One traded asset per round. Read the book and the tape, trade against the bots, and finish ahead. The true value is revealed on the scorecard.",
            now,
        )
        .expect("game market is unique");

        ex.reset_account(&market_player(), STARTING_CASH);
        ex.grant_shares(&market_player(), &market, OUTCOME, STARTING_SHARES)
            .expect("player inventory");

        for spec in cfg.bots {
            ex.reset_account(&spec.account, BOT_CASH);
            ex.grant_shares(&spec.account, &market, OUTCOME, spec.baseline_shares as u64)
                .expect("bot inventory");
            bots.push(BotRuntime {
                account: spec.account,
                strategy: spec.strategy,
                fair_noise: spec.fair_noise,
                baseline: spec.baseline_shares,
            });
        }
    }

    let starting_equity = STARTING_CASH + STARTING_SHARES as i64 * start_mark as i64;

    let session = GameSession {
        generation,
        tier,
        market: market.clone(),
        player: market_player(),
        started_ms: now_ms(),
        duration_secs,
        tick_ms: cfg.tick_ms,
        rng: exchangekit_engine::game::Rng::seed(seed),
        fair,
        bots,
        equity_curve: vec![starting_equity],
        starting_equity,
        status: GameStatus::Running,
        scorecard: None,
        final_fair: None,
    };

    *state.game.write().expect("lock") = Some(session);
    spawn_tick(state.clone(), generation);
    (generation, market)
}

fn market_player() -> String {
    "player".to_string()
}

fn spawn_tick(state: Arc<AppState>, generation: u64) {
    let tick_ms = state
        .game
        .read()
        .expect("lock")
        .as_ref()
        .map(|g| g.tick_ms)
        .unwrap_or(700);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let mut events: Vec<Value> = Vec::new();
            let mut over_events: Vec<Value> = Vec::new();
            let mut stop = false;

            {
                let mut game_guard = state.game.write().expect("lock");
                let Some(game) = game_guard.as_mut() else {
                    break;
                };
                if game.generation != generation || matches!(game.status, GameStatus::Ended) {
                    break;
                }

                let mut ex = state.exchange.write().expect("lock");
                let now = now_ms();

                // Round over: freeze the book, score, reveal the fair value.
                if now.saturating_sub(game.started_ms) >= game.duration_secs * 1000 {
                    let trades = count_player_trades(&ex, &game.market, &game.player);
                    game.scorecard = Some(score(&game.equity_curve, trades));
                    game.final_fair = Some(game.fair.cents());
                    game.status = GameStatus::Ended;
                    // Cancel every resting order so the frozen book is honest.
                    let mut accounts: Vec<String> =
                        game.bots.iter().map(|b| b.account.clone()).collect();
                    accounts.push(game.player.clone());
                    for acct in accounts {
                        let open: Vec<u64> = ex
                            .open_orders(&acct)
                            .into_iter()
                            .filter(|o| o.market == game.market)
                            .map(|o| o.id)
                            .collect();
                        for oid in open {
                            let _ = ex.cancel_order(&acct, oid);
                        }
                    }
                    over_events.push(json!({ "type": "game_over" }));
                    stop = true;
                } else {
                    let true_fair = game.fair.step(&mut game.rng);
                    let fallback_mid = game.fair.value;

                    for bot in game.bots.iter_mut() {
                        let open: Vec<u64> = ex
                            .open_orders(&bot.account)
                            .into_iter()
                            .filter(|o| o.market == game.market)
                            .map(|o| o.id)
                            .collect();
                        for oid in open {
                            let _ = ex.cancel_order(&bot.account, oid);
                        }
                        let snap =
                            snapshot(&ex, &game.market, &bot.account, bot.baseline, fallback_mid);
                        let est = true_fair + game.rng.gaussian() * bot.fair_noise;
                        for o in bot.strategy.decide(est, &snap, &mut game.rng) {
                            if let Ok(res) = ex.place_order(
                                &bot.account,
                                &game.market,
                                OUTCOME,
                                o.side,
                                o.price,
                                o.qty,
                                now,
                            ) {
                                for t in res.trades {
                                    events.push(
                                        json!({ "type": "trade", "trade": serde_json::to_value(t).unwrap() }),
                                    );
                                }
                            }
                        }
                    }

                    let m = mark(&ex, &game.market, game.fair.cents());
                    let eq = equity(&ex, &game.market, &game.player, m);
                    game.equity_curve.push(eq);
                }

                // Publish the fresh book so the UI redraws every tick.
                if let Ok(book) = ex.book_view(&game.market, OUTCOME, 20) {
                    events.push(json!({ "type": "book", "book": book }));
                }
            }

            for ev in events.into_iter().chain(over_events) {
                let _ = state.events.send(ev.to_string());
            }
            if stop {
                break;
            }
        }
    });
}

/// A public snapshot of the current round for the state endpoint.
pub fn state_json(state: &Arc<AppState>) -> Value {
    let game_guard = state.game.read().expect("lock");
    let Some(game) = game_guard.as_ref() else {
        return json!({ "status": "idle" });
    };

    let now = now_ms();
    let elapsed = now.saturating_sub(game.started_ms) / 1000;
    let remaining = game.duration_secs.saturating_sub(elapsed);

    let (player_equity, pnl) = {
        let ex = state.exchange.read().expect("lock");
        let m = mark(&ex, &game.market, game.fair.cents());
        let eq = equity(&ex, &game.market, &game.player, m);
        (eq, eq - game.starting_equity)
    };

    // Aggregate opponents by strategy name, keeping the hint and a count.
    let mut opponents: Vec<Value> = Vec::new();
    for bot in &game.bots {
        let name = bot.strategy.name();
        if let Some(existing) = opponents.iter_mut().find(|o| o["name"] == json!(name)) {
            existing["count"] = json!(existing["count"].as_u64().unwrap_or(1) + 1);
        } else {
            opponents.push(json!({
                "name": name,
                "hint": bot.strategy.hint(),
                "count": 1,
            }));
        }
    }

    let status = match game.status {
        GameStatus::Running => "running",
        GameStatus::Ended => "ended",
    };

    json!({
        "status": status,
        "tier": game.tier,
        "market": game.market,
        "outcome": OUTCOME.as_str(),
        "player": game.player,
        "started_ms": game.started_ms,
        "duration_secs": game.duration_secs,
        "remaining_secs": remaining,
        "tick_ms": game.tick_ms,
        "starting_equity": game.starting_equity,
        "player_equity": player_equity,
        "pnl": pnl,
        "opponents": opponents,
    })
}

/// The scorecard, or an idle/running marker.
pub fn scorecard_json(state: &Arc<AppState>) -> Value {
    let game_guard = state.game.read().expect("lock");
    let Some(game) = game_guard.as_ref() else {
        return json!({ "status": "idle" });
    };
    match (&game.status, &game.scorecard) {
        (GameStatus::Ended, Some(card)) => json!({
            "status": "ended",
            "tier": game.tier,
            "fair_value": game.final_fair,
            "scorecard": card,
        }),
        _ => json!({ "status": "running" }),
    }
}
