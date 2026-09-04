use serde::{Deserialize, Serialize};

/// Order identifier, unique per exchange instance.
pub type OrderId = u64;
/// Trade identifier, unique per exchange instance.
pub type TradeId = u64;
/// Price of one share in play-money cents. Valid range is 1 to 99
/// because a binary contract settles at either 0 or 100 cents.
pub type Price = u32;
/// Quantity in whole shares.
pub type Qty = u64;
/// Cash amount in play-money cents.
pub type Cash = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Outcome {
    Yes,
    No,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Yes => "YES",
            Outcome::No => "NO",
        }
    }
}

impl std::str::FromStr for Outcome {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "YES" => Ok(Outcome::Yes),
            "NO" => Ok(Outcome::No),
            other => Err(format!("unknown outcome: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

impl std::str::FromStr for Side {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "BUY" => Ok(Side::Buy),
            "SELL" => Ok(Side::Sell),
            other => Err(format!("unknown side: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    pub description: String,
    /// Unix milliseconds.
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub account: String,
    pub market: String,
    pub outcome: Outcome,
    pub side: Side,
    /// Limit price in cents, 1 to 99.
    pub price: Price,
    pub quantity: Qty,
    pub filled: Qty,
    pub status: OrderStatus,
    /// Monotonic sequence number used for time priority.
    pub seq: u64,
    /// Unix milliseconds.
    pub created_at: u64,
}

impl Order {
    pub fn remaining(&self) -> Qty {
        self.quantity - self.filled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    pub market: String,
    pub outcome: Outcome,
    /// Execution price in cents (the resting order's price).
    pub price: Price,
    pub quantity: Qty,
    pub taker_side: Side,
    pub buyer: String,
    pub seller: String,
    pub buy_order: OrderId,
    pub sell_order: OrderId,
    /// Unix milliseconds.
    pub ts: u64,
}

/// One aggregated price level of an order book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    pub price: Price,
    pub quantity: Qty,
}

/// Aggregated view of one side of a book, best price first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookView {
    pub market: String,
    pub outcome: Outcome,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

/// Result of submitting an order: the (possibly filled) order plus any
/// trades it produced while crossing the book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceResult {
    pub order: Order,
    pub trades: Vec<Trade>,
}
