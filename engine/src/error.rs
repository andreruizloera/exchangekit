use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EngineError {
    #[error("unknown market: {0}")]
    UnknownMarket(String),
    #[error("unknown account: {0}")]
    UnknownAccount(String),
    #[error("unknown order: {0}")]
    UnknownOrder(u64),
    #[error("market already exists: {0}")]
    MarketExists(String),
    #[error("account already exists: {0}")]
    AccountExists(String),
    #[error("price must be between 1 and 99 cents, got {0}")]
    InvalidPrice(u32),
    #[error("quantity must be positive")]
    InvalidQuantity,
    #[error("insufficient balance: need {need} cents, available {available}")]
    InsufficientBalance { need: i64, available: i64 },
    #[error("insufficient position: need {need} shares, available {available}")]
    InsufficientPosition { need: i64, available: i64 },
    #[error("order {0} is not open")]
    OrderNotOpen(u64),
    #[error("order {order} does not belong to account {account}")]
    NotOrderOwner { order: u64, account: String },
}
