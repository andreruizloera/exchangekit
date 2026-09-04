use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use exchangekit_engine::{EngineError, Exchange, Outcome, Side};
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
    /// Price estimates in cents (last trade, else book midpoint).
    yes_price: Option<u32>,
    no_price: Option<u32>,
    volume: u64,
}

fn summarize(ex: &Exchange, id: &str) -> Option<MarketSummary> {
    let m = ex.market(id)?;
    Some(MarketSummary {
        id: m.id.clone(),
        question: m.question.clone(),
        description: m.description.clone(),
        created_at: m.created_at,
        yes_price: ex.price_estimate(id, Outcome::Yes),
        no_price: ex.price_estimate(id, Outcome::No),
        volume: ex.volume(id),
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

#[derive(Deserialize)]
pub struct PlaceOrderBody {
    account: String,
    market: String,
    outcome: String,
    side: String,
    /// Limit price in cents, 1 to 99.
    price: u32,
    quantity: u64,
}

pub async fn place_order(
    State(state): Shared,
    Json(body): Json<PlaceOrderBody>,
) -> Result<Json<exchangekit_engine::PlaceResult>, ApiError> {
    let outcome: Outcome = body.outcome.parse().map_err(bad_request)?;
    let side: Side = body.side.parse().map_err(bad_request)?;
    let mut ex = state.exchange.write().expect("lock");
    let result = ex.place_order(
        &body.account,
        &body.market,
        outcome,
        side,
        body.price,
        body.quantity,
        now_ms(),
    )?;
    for trade in &result.trades {
        broadcast(
            &state,
            json!({ "type": "trade", "trade": serde_json::to_value(trade).unwrap() }),
        );
    }
    broadcast_book_and_market(&state, &ex, &body.market, outcome);
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
