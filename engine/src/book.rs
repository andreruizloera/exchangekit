use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::types::{Level, OrderId, Price, Qty};

/// A single order book (one market outcome). Holds only order ids and
/// per-level FIFO queues; order details live in the exchange order table.
/// Price priority comes from the BTreeMap ordering, time priority from
/// the FIFO queue at each level.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Book {
    /// Buy orders. Best bid is the highest key.
    pub bids: BTreeMap<Price, VecDeque<OrderId>>,
    /// Sell orders. Best ask is the lowest key.
    pub asks: BTreeMap<Price, VecDeque<OrderId>>,
}

impl Book {
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    pub fn add_bid(&mut self, price: Price, id: OrderId) {
        self.bids.entry(price).or_default().push_back(id);
    }

    pub fn add_ask(&mut self, price: Price, id: OrderId) {
        self.asks.entry(price).or_default().push_back(id);
    }

    /// Remove an order id from a level, dropping the level if it empties.
    /// Returns true if the order was present.
    pub fn remove(&mut self, is_bid: bool, price: Price, id: OrderId) -> bool {
        let side = if is_bid {
            &mut self.bids
        } else {
            &mut self.asks
        };
        let Some(queue) = side.get_mut(&price) else {
            return false;
        };
        let Some(pos) = queue.iter().position(|&x| x == id) else {
            return false;
        };
        queue.remove(pos);
        if queue.is_empty() {
            side.remove(&price);
        }
        true
    }

    /// Aggregate levels with a resolver from order id to remaining quantity.
    pub fn levels<F: Fn(OrderId) -> Qty>(
        &self,
        depth: usize,
        remaining: F,
    ) -> (Vec<Level>, Vec<Level>) {
        let agg = |iter: &mut dyn Iterator<Item = (&Price, &VecDeque<OrderId>)>| {
            iter.map(|(price, queue)| Level {
                price: *price,
                quantity: queue.iter().map(|&id| remaining(id)).sum(),
            })
            .filter(|l| l.quantity > 0)
            .take(depth)
            .collect::<Vec<_>>()
        };
        let bids = agg(&mut self.bids.iter().rev());
        let asks = agg(&mut self.asks.iter());
        (bids, asks)
    }
}
