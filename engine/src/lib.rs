//! exchangekit-engine: a price-time priority matching engine for binary
//! prediction markets. Play money only; there is no custody, payment,
//! or settlement of real funds anywhere in this crate.
//!
//! The engine is deliberately synchronous and in-memory. All state lives
//! in [`Exchange`], which is serializable so callers can snapshot and
//! restore it. Concurrency is the caller's concern (the gateway wraps it
//! in an `RwLock`).

mod book;
mod error;
mod exchange;
pub mod game;
mod settlement;
mod types;

pub use book::Book;
pub use error::EngineError;
pub use exchange::{Account, Exchange, MarketPosition, Position};
pub use settlement::{
    compute_payouts, unbacked, Holding, Payout, Resolution, Settlement, SHARE_PAYOUT,
};
pub use types::{
    BookView, Cash, Level, Market, Order, OrderId, OrderStatus, Outcome, PlaceResult, Price, Qty,
    Side, Trade, TradeId,
};
