use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use exchangekit_engine::{
    EngineError, Exchange, OrderRequest, Outcome, Side, TimeInForce, TradeKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use exchangekit_engine::game::Tier;

use crate::{game, now_ms, AppState};

type Shared = State<Arc<AppState>>;

/// Per-round game markets are named `game-N`. They are hidden from the public
/// market list so the Python SDK and the demo only ever see the seeded
/// markets, while the game UI drives them directly by id.
fn is_game_market(id: &str) -> bool {
    id.starts_with("game-")
}

// ---- error mapping -------------------------------------------------------

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<EngineError> for ApiError {
    fn from(e: EngineError) -> Self {
        let status = match e {
            EngineError::UnknownMarket(_)
            | EngineError::UnknownAccount(_)
            | EngineError::UnknownOrder(_) => StatusCode::NOT_FOUND,
            // Resolving twice is a conflict with the market's state, not a
            // malformed request; the caller's second try is well formed and
            // simply lost the race.
            EngineError::MarketAlreadyResolved(_) => StatusCode::CONFLICT,
            _ => StatusCode::BAD_REQUEST,
        };
        ApiError(status, e.to_string())
    }
}

fn bad_request(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, msg.into())
}

// ---- views ---------------------------------------------------------------

#[derive(Serialize)]
pub struct MarketSummary {
    id: String,
    question: String,
    description: String,
    created_at: u64,
    /// Price estimates in cents (last trade, else book midpoint). These
    /// stay at the last traded price after resolution: they describe the
    /// tape, not the settlement. `resolved_outcome` is the settled answer.
    yes_price: Option<u32>,
    no_price: Option<u32>,
    volume: u64,
    /// "open" while the market trades, "resolved" once it has settled.
    status: &'static str,
    /// The winning outcome, or null while the market is open.
    resolved_outcome: Option<Outcome>,
    resolved_at: Option<u64>,
    /// Cents backing the market's outstanding shares. Zero after
    /// resolution, because the pool was paid out.
    collateral: i64,
}

fn summarize(ex: &Exchange, id: &str) -> Option<MarketSummary> {
    let m = ex.market(id)?;
    let resolution = m.resolution;
    Some(MarketSummary {
        id: m.id.clone(),
        question: m.question.clone(),
        description: m.description.clone(),
        created_at: m.created_at,
        yes_price: ex.price_estimate(id, Outcome::Yes),
        no_price: ex.price_estimate(id, Outcome::No),
        volume: ex.volume(id),
        status: if resolution.is_some() {
            "resolved"
        } else {
            "open"
        },
        resolved_outcome: resolution.map(|r| r.outcome),
        resolved_at: resolution.map(|r| r.resolved_at),
        collateral: ex.collateral(id),
    })
}

#[derive(Serialize)]
pub struct PositionRow {
    market: String,
    outcome: Outcome,
    quantity: i64,
    locked: i64,
}

// ---- event fanout --------------------------------------------------------

fn broadcast(state: &AppState, value: serde_json::Value) {
    // Errors just mean no subscribers; that is fine.
    let _ = state.events.send(value.to_string());
}

fn broadcast_book_and_market(state: &AppState, ex: &Exchange, market: &str, outcome: Outcome) {
    if let Ok(book) = ex.book_view(market, outcome, 20) {
        broadcast(state, json!({ "type": "book", "book": book }));
    }
    if let Some(summary) = summarize(ex, market) {
        broadcast(
            state,
            json!({ "type": "market", "market": serde_json::to_value(summary).unwrap() }),
        );
    }
}

// ---- handlers ------------------------------------------------------------

pub async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "play_money_only": true }))
}

pub async fn list_markets(State(state): Shared) -> Json<Vec<MarketSummary>> {
    let ex = state.exchange.read().expect("lock");
    let ids: Vec<String> = ex
        .markets()
        .map(|m| m.id.clone())
        .filter(|id| !is_game_market(id))
        .collect();
    Json(ids.iter().filter_map(|id| summarize(&ex, id)).collect())
}

pub async fn get_market(
    State(state): Shared,
    Path(id): Path<String>,
) -> Result<Json<MarketSummary>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    summarize(&ex, &id)
        .map(Json)
        .ok_or_else(|| not_found(format!("unknown market: {id}")))
}

#[derive(Deserialize)]
pub struct BookQuery {
    outcome: Option<String>,
    depth: Option<usize>,
}

pub async fn get_book(
    State(state): Shared,
    Path(id): Path<String>,
    Query(q): Query<BookQuery>,
) -> Result<Json<exchangekit_engine::BookView>, ApiError> {
    let outcome: Outcome = q
        .outcome
        .as_deref()
        .unwrap_or("YES")
        .parse()
        .map_err(bad_request)?;
    let depth = q.depth.unwrap_or(20).min(99);
    let ex = state.exchange.read().expect("lock");
    Ok(Json(ex.book_view(&id, outcome, depth)?))
}

#[derive(Deserialize)]
pub struct TradesQuery {
    limit: Option<usize>,
}

pub async fn get_trades(
    State(state): Shared,
    Path(id): Path<String>,
    Query(q): Query<TradesQuery>,
) -> Result<Json<Vec<exchangekit_engine::Trade>>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    Ok(Json(ex.recent_trades(&id, q.limit.unwrap_or(50).min(500))?))
}

// ---- settlement ----------------------------------------------------------

#[derive(Deserialize)]
pub struct ResolveBody {
    /// The winning outcome, "YES" or "NO".
    outcome: String,
}

/// Settle a market and pay out. This is an admin operation: the gateway has
/// no authentication (see the README), so anyone who can reach it can
/// resolve a market. That is acceptable in a local simulator and would not
/// be anywhere else.
///
/// Game markets are refused. A round's market is driven by the tick loop
/// and scored by marking inventory to the book, so settling it out from
/// under a running round would rewrite the player's equity mid-game.
pub async fn resolve_market(
    State(state): Shared,
    Path(id): Path<String>,
    Json(body): Json<ResolveBody>,
) -> Result<Json<exchangekit_engine::Settlement>, ApiError> {
    let outcome: Outcome = body.outcome.parse().map_err(bad_request)?;
    if is_game_market(&id) {
        return Err(bad_request(format!(
            "{id} is a game round's market; rounds end on the clock, not by resolution"
        )));
    }
    let mut ex = state.exchange.write().expect("lock");
    let settlement = ex.resolve_market(&id, outcome, now_ms())?;
    broadcast(
        &state,
        json!({ "type": "resolution", "settlement": serde_json::to_value(&settlement).unwrap() }),
    );
    // Both books are empty now; publish them and the new market status.
    for o in [Outcome::Yes, Outcome::No] {
        broadcast_book_and_market(&state, &ex, &id, o);
    }
    Ok(Json(settlement))
}

/// The settlement report of an already-resolved market.
pub async fn get_settlement(
    State(state): Shared,
    Path(id): Path<String>,
) -> Result<Json<exchangekit_engine::Settlement>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    if ex.market(&id).is_none() {
        return Err(not_found(format!("unknown market: {id}")));
    }
    ex.settlement(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| not_found(format!("market {id} has not resolved")))
}

#[derive(Deserialize)]
pub struct PlaceOrderBody {
    account: String,
    market: String,
    outcome: String,
    side: String,
    /// Limit price in cents, 1 to 99.
    price: u32,
    quantity: u64,
    /// "gtc" (the default), "ioc", "fok", or "post_only". The long
    /// spellings the engine serializes are accepted too.
    time_in_force: Option<String>,
}

pub async fn place_order(
    State(state): Shared,
    Json(body): Json<PlaceOrderBody>,
) -> Result<Json<exchangekit_engine::PlaceResult>, ApiError> {
    let outcome: Outcome = body.outcome.parse().map_err(bad_request)?;
    let side: Side = body.side.parse().map_err(bad_request)?;
    let tif: TimeInForce = body
        .time_in_force
        .as_deref()
        .unwrap_or("gtc")
        .parse()
        .map_err(bad_request)?;
    let mut ex = state.exchange.write().expect("lock");
    let result = ex.place(
        OrderRequest::limit(
            &body.account,
            &body.market,
            outcome,
            side,
            body.price,
            body.quantity,
            now_ms(),
        )
        .tif(tif),
    )?;
    for trade in &result.trades {
        broadcast(
            &state,
            json!({ "type": "trade", "trade": serde_json::to_value(trade).unwrap() }),
        );
    }
    broadcast_book_and_market(&state, &ex, &body.market, outcome);
    // A complementary cross consumes a resting order in the other book, so
    // that book has to be republished too or a client watching it would
    // keep drawing depth that is no longer there.
    if result.trades.iter().any(|t| t.kind != TradeKind::Match) {
        broadcast_book_and_market(&state, &ex, &body.market, outcome.complement());
    }
    Ok(Json(result))
}

// ---- pairs ---------------------------------------------------------------

#[derive(Deserialize)]
pub struct PairBody {
    account: String,
    /// Number of YES/NO pairs.
    quantity: u64,
}

/// What an account holds in one market after minting or redeeming.
#[derive(Serialize)]
pub struct PairResult {
    account: String,
    market: String,
    /// Pairs minted or redeemed by this call.
    quantity: u64,
    balance: i64,
    available: i64,
    yes: i64,
    no: i64,
    /// Cents the market holds against its outstanding shares, after the call.
    collateral: i64,
}

fn pair_result(ex: &Exchange, account: &str, market: &str, quantity: u64) -> PairResult {
    let acct = ex.account(account).expect("account exists");
    let pos = acct.positions.get(market).copied().unwrap_or_default();
    PairResult {
        account: account.to_string(),
        market: market.to_string(),
        quantity,
        balance: acct.balance,
        available: acct.available_cash(),
        yes: pos.get(Outcome::Yes).quantity,
        no: pos.get(Outcome::No).quantity,
        collateral: ex.collateral(market),
    }
}

/// Game markets are refused for both pair endpoints, the same way
/// resolution is: a round's inventory is granted one-sided on purpose and
/// its equity is marked against the YES book alone.
fn require_not_game(id: &str, what: &str) -> Result<(), ApiError> {
    if is_game_market(id) {
        return Err(bad_request(format!(
            "{id} is a game round's market and does not {what} pairs"
        )));
    }
    Ok(())
}

/// Buy `quantity` YES/NO pairs at 100 cents each. The cents become the
/// market's collateral, which is what lets it settle without creating cash.
pub async fn mint_pair(
    State(state): Shared,
    Path(id): Path<String>,
    Json(body): Json<PairBody>,
) -> Result<Json<PairResult>, ApiError> {
    require_not_game(&id, "mint")?;
    let mut ex = state.exchange.write().expect("lock");
    ex.mint_pair(&body.account, &id, body.quantity)?;
    let result = pair_result(&ex, &body.account, &id, body.quantity);
    broadcast_book_and_market(&state, &ex, &id, Outcome::Yes);
    Ok(Json(result))
}

/// Sell `quantity` YES/NO pairs back for 100 cents each, out of the
/// market's collateral. The inverse of minting, and the reason a pair is
/// worth a dollar before the market resolves rather than only after.
pub async fn redeem_pair(
    State(state): Shared,
    Path(id): Path<String>,
    Json(body): Json<PairBody>,
) -> Result<Json<PairResult>, ApiError> {
    require_not_game(&id, "redeem")?;
    let mut ex = state.exchange.write().expect("lock");
    ex.redeem_pair(&body.account, &id, body.quantity)?;
    let result = pair_result(&ex, &body.account, &id, body.quantity);
    broadcast_book_and_market(&state, &ex, &id, Outcome::Yes);
    Ok(Json(result))
}

pub async fn get_order(
    State(state): Shared,
    Path(id): Path<u64>,
) -> Result<Json<exchangekit_engine::Order>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    ex.order(id)
        .cloned()
        .map(Json)
        .ok_or_else(|| not_found(format!("unknown order: {id}")))
}

#[derive(Deserialize)]
pub struct CancelQuery {
    account: String,
}

pub async fn cancel_order(
    State(state): Shared,
    Path(id): Path<u64>,
    Query(q): Query<CancelQuery>,
) -> Result<Json<exchangekit_engine::Order>, ApiError> {
    let mut ex = state.exchange.write().expect("lock");
    let order = ex.cancel_order(&q.account, id)?;
    broadcast_book_and_market(&state, &ex, &order.market, order.outcome);
    Ok(Json(order))
}

pub async fn get_account(
    State(state): Shared,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    let acct = ex
        .account(&id)
        .ok_or_else(|| not_found(format!("unknown account: {id}")))?;
    Ok(Json(json!({
        "id": acct.id,
        "balance": acct.balance,
        "locked": acct.locked_cash,
        "available": acct.available_cash(),
    })))
}

pub async fn get_positions(
    State(state): Shared,
    Path(id): Path<String>,
) -> Result<Json<Vec<PositionRow>>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    let acct = ex
        .account(&id)
        .ok_or_else(|| not_found(format!("unknown account: {id}")))?;
    let mut rows: Vec<PositionRow> = acct
        .positions
        .iter()
        .flat_map(|(market, mp)| {
            [Outcome::Yes, Outcome::No].into_iter().map(move |o| {
                let p = mp.get(o);
                PositionRow {
                    market: market.clone(),
                    outcome: o,
                    quantity: p.quantity,
                    locked: p.locked,
                }
            })
        })
        .filter(|r| r.quantity != 0 || r.locked != 0)
        .collect();
    rows.sort_by(|a, b| a.market.cmp(&b.market));
    Ok(Json(rows))
}

pub async fn get_open_orders(
    State(state): Shared,
    Path(id): Path<String>,
) -> Result<Json<Vec<exchangekit_engine::Order>>, ApiError> {
    let ex = state.exchange.read().expect("lock");
    if ex.account(&id).is_none() {
        return Err(not_found(format!("unknown account: {id}")));
    }
    Ok(Json(ex.open_orders(&id)))
}

// ---- game ----------------------------------------------------------------

#[derive(Deserialize)]
pub struct StartGameBody {
    tier: String,
    duration_secs: Option<u64>,
    /// Optional seed for a reproducible round; defaults to the clock.
    seed: Option<u64>,
}

pub async fn game_start(
    State(state): Shared,
    Json(body): Json<StartGameBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tier: Tier = Tier::parse(&body.tier)
        .ok_or_else(|| bad_request(format!("unknown tier: {}", body.tier)))?;
    let duration = body.duration_secs.unwrap_or(game::DEFAULT_DURATION);
    let seed = body
        .seed
        .unwrap_or_else(|| now_ms() ^ 0x5DEE_CE66_D3A8_3C1B);
    game::start(&state, tier, duration, seed);
    Ok(Json(game::state_json(&state)))
}

pub async fn game_state(State(state): Shared) -> Json<serde_json::Value> {
    Json(game::state_json(&state))
}

pub async fn game_scorecard(State(state): Shared) -> Json<serde_json::Value> {
    Json(game::scorecard_json(&state))
}

// ---- websocket -----------------------------------------------------------

pub async fn ws_upgrade(ws: WebSocketUpgrade, State(state): Shared) -> Response {
    ws.on_upgrade(move |socket| ws_session(socket, state))
}

async fn ws_session(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.events.subscribe();

    // Initial snapshot so clients can render immediately.
    let hello = {
        let ex = state.exchange.read().expect("lock");
        let ids: Vec<String> = ex
            .markets()
            .map(|m| m.id.clone())
            .filter(|id| !is_game_market(id))
            .collect();
        let markets: Vec<MarketSummary> = ids.iter().filter_map(|id| summarize(&ex, id)).collect();
        json!({ "type": "hello", "markets": markets }).to_string()
    };
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(msg) => {
                    if socket.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                // Slow consumer fell behind the broadcast buffer; resync
                // is REST's job, just keep streaming.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => continue, // pings and client chatter are ignored
                Some(Err(_)) => break,
            },
        }
    }
}
