use serde::{Deserialize, Serialize};

use crate::settlement::Resolution;

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

    /// The other side of the same question. A YES share and a NO share of
    /// one market always settle for 100 cents between them, whichever way
    /// the market resolves, which is what lets the two books trade against
    /// each other.
    pub fn complement(&self) -> Outcome {
        match self {
            Outcome::Yes => Outcome::No,
            Outcome::No => Outcome::Yes,
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
    /// Removed from the book because the market resolved underneath it.
    /// Distinct from `Cancelled`, which is something an account chose.
    Voided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    pub description: String,
    /// Unix milliseconds.
    pub created_at: u64,
    /// `None` while the market trades; `Some` once it has settled. Old
    /// snapshots predate this field and load as an open market.
    #[serde(default)]
    pub resolution: Option<Resolution>,
}

impl Market {
    pub fn is_resolved(&self) -> bool {
        self.resolution.is_some()
    }
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

/// How the shares in a trade came to change hands.
///
/// Every trade names a buyer and a seller of one outcome at one price,
/// because that is what a trade is economically. The kind says where the
/// shares physically came from, which is not the same question.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    /// An ordinary cross inside one outcome's own book. The seller handed
    /// over shares they already held and the buyer received them.
    #[default]
    Match,
    /// A complementary cross between two buyers on opposite outcomes: the
    /// pair they bought between them was minted against 100 cents of
    /// collateral, which the two of them funded in full. `seller` is the
    /// account that bought the complementary outcome, because buying NO
    /// at `100 - p` is selling YES at `p`. It never held a YES share.
    Mint,
    /// A complementary cross between two sellers on opposite outcomes: the
    /// pair they gave up was burned and 100 cents of collateral released
    /// to pay them. `buyer` is the account that sold the complementary
    /// outcome. It never received a share of this one.
    Burn,
}

impl TradeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeKind::Match => "match",
            TradeKind::Mint => "mint",
            TradeKind::Burn => "burn",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    pub market: String,
    /// The outcome this trade is priced in: the taker's side of it.
    pub outcome: Outcome,
    /// Execution price in cents (the resting order's price, in the frame
    /// of `outcome`).
    pub price: Price,
    pub quantity: Qty,
    pub taker_side: Side,
    pub buyer: String,
    pub seller: String,
    pub buy_order: OrderId,
    pub sell_order: OrderId,
    /// Whether the shares came from the seller, from a fresh mint, or were
    /// burned. Absent from snapshots taken before complementary matching
    /// existed, which load as [`TradeKind::Match`].
    #[serde(default)]
    pub kind: TradeKind,
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
