//! exchangekit-gateway: HTTP + WebSocket front door for the matching
//! engine. Play money only; no custody or payment code exists here.

mod handlers;
mod seed;

use std::env;
use std::sync::{Arc, RwLock};

use axum::routing::{delete, get, post};
use axum::Router;
use exchangekit_engine::Exchange;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub exchange: RwLock<Exchange>,
    /// Fanout channel: every mutation publishes JSON events that all
    /// connected WebSocket clients receive.
    pub events: broadcast::Sender<String>,
    pub snapshot_path: Option<String>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as u64
}

fn load_or_seed(snapshot_path: Option<&str>) -> Exchange {
    if let Some(path) = snapshot_path {
        match std::fs::read_to_string(path) {
            Ok(json) => match Exchange::from_snapshot(&json) {
                Ok(ex) => {
                    println!("loaded snapshot from {path}");
                    return ex;
                }
                Err(e) => eprintln!("ignoring unreadable snapshot {path}: {e}"),
            },
            Err(_) => println!("no snapshot at {path}, seeding fresh state"),
        }
    }
    seed::seed_exchange()
}

fn save_snapshot(state: &AppState) {
    if let Some(path) = &state.snapshot_path {
        let json = state.exchange.read().expect("lock").to_snapshot();
        if let Err(e) = std::fs::write(path, json) {
            eprintln!("failed to write snapshot {path}: {e}");
        }
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let snapshot_path = env::var("EXCHANGEKIT_SNAPSHOT").ok();

    let (events, _) = broadcast::channel(1024);
    let state = Arc::new(AppState {
        exchange: RwLock::new(load_or_seed(snapshot_path.as_deref())),
        events,
        snapshot_path,
    });

    let app = Router::new()
        .route("/api/health", get(handlers::health))
        .route("/api/markets", get(handlers::list_markets))
        .route("/api/markets/{id}", get(handlers::get_market))
        .route("/api/markets/{id}/book", get(handlers::get_book))
        .route("/api/markets/{id}/trades", get(handlers::get_trades))
        .route("/api/orders", post(handlers::place_order))
        .route("/api/orders/{id}", get(handlers::get_order))
        .route("/api/orders/{id}", delete(handlers::cancel_order))
        .route("/api/accounts/{id}", get(handlers::get_account))
        .route("/api/accounts/{id}/positions", get(handlers::get_positions))
        .route("/api/accounts/{id}/orders", get(handlers::get_open_orders))
        .route("/ws", get(handlers::ws_upgrade))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Periodic snapshot (only when EXCHANGEKIT_SNAPSHOT is set).
    if state.snapshot_path.is_some() {
        let snap_state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                save_snapshot(&snap_state);
            }
        });
    }

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    println!("exchangekit gateway listening on http://{addr} (play money only)");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await
        .expect("server error");
}

async fn shutdown_signal(state: Arc<AppState>) {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await.ok();
    save_snapshot(&state);
    println!("shutting down");
}
