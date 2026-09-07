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
    /// Refused by its own terms before it traded or rested: a fill-or-kill
    /// that could not fill in full, or a post-only that would have taken.
    /// Distinct from `Cancelled`, which is an order that did exist.
    Rejected,
}

/// How long an order may live, and whether it is allowed to take.
///
/// The default is the ordinary limit order every other part of this engine
/// assumes: fill what crosses now, rest the remainder.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeInForce {
    /// Fill whatever crosses and rest the remainder in the book until it
    /// fills, is cancelled, or the market resolves.
    #[default]
    GoodTillCancelled,
    /// Fill whatever crosses right now and cancel the rest. Never rests, so
    /// it never gives anyone else the option of trading against it later.
    ImmediateOrCancel,
    /// Fill the whole quantity right now or do nothing at all. Rejected if
    /// the books cannot supply all of it at the limit price.
    FillOrKill,
    /// Never take. Rest in the book, or be rejected if any part of the
    /// order would have crossed. This is how a maker guarantees it is
    /// quoting rather than paying the spread.
    PostOnly,
}

impl TimeInForce {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeInForce::GoodTillCancelled => "gtc",
            TimeInForce::ImmediateOrCancel => "ioc",
            TimeInForce::FillOrKill => "fok",
            TimeInForce::PostOnly => "post_only",
        }
    }
}

impl std::str::FromStr for TimeInForce {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "gtc" | "good_till_cancelled" | "good_til_cancelled" => {
                Ok(TimeInForce::GoodTillCancelled)
            }
            "ioc" | "immediate_or_cancel" => Ok(TimeInForce::ImmediateOrCancel),
            "fok" | "fill_or_kill" => Ok(TimeInForce::FillOrKill),
            "post_only" | "postonly" => Ok(TimeInForce::PostOnly),
            other => Err(format!("unknown time in force: {other}")),
        }
    }
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
    /// Absent from snapshots taken before order types existed, which load
    /// as ordinary limit orders.
    #[serde(default)]
    pub time_in_force: TimeInForce,
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

/// Everything one order submission says. Built by
/// [`OrderRequest::limit`] and adjusted with [`OrderRequest::tif`].
#[derive(Debug, Clone, Copy)]
pub struct OrderRequest<'a> {
    pub account: &'a str,
    pub market: &'a str,
    pub outcome: Outcome,
    pub side: Side,
    /// Limit price in cents, 1 to 99.
    pub price: Price,
    pub quantity: Qty,
    pub time_in_force: TimeInForce,
    /// Unix milliseconds.
    pub now_ms: u64,
}

impl<'a> OrderRequest<'a> {
    /// An ordinary limit order: fill what crosses, rest the remainder.
    pub fn limit(
        account: &'a str,
        market: &'a str,
        outcome: Outcome,
        side: Side,
        price: Price,
        quantity: Qty,
        now_ms: u64,
    ) -> Self {
        Self {
            account,
            market,
            outcome,
            side,
            price,
            quantity,
            time_in_force: TimeInForce::default(),
            now_ms,
        }
    }

    pub fn tif(mut self, time_in_force: TimeInForce) -> Self {
        self.time_in_force = time_in_force;
        self
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
