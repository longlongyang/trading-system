//! Order book structure with bids and asks.
//!
//! The [`OrderBook`] is the central data structure that organizes orders
//! by price level and provides efficient access to the best prices.
//!
//! # Structure
//!
//! - **Bids** (buy orders): Stored in a `BTreeMap` with descending price order
//!   (highest price = best bid)
//! - **Asks** (sell orders): Stored in a `BTreeMap` with ascending price order
//!   (lowest price = best ask)
//! - **Orders**: A `HashMap` for O(1) order lookup by ID
//!
//! # Price Ordering
//!
//! We use `Reverse<Price>` for bids so that the `BTreeMap`'s natural ordering
//! puts the highest price first (best bid). For asks, we use `Price` directly
//! so the lowest price comes first (best ask).

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

use types::{MarketId, Order, OrderId, OrderSide, Price, Quantity};

use crate::price_level::PriceLevel;

// =============================================================================
// OrderBook
// =============================================================================

/// An order book for a single market.
///
/// Maintains bids and asks organized by price level with FIFO ordering
/// within each level. Also maintains a `HashMap` for O(1) order lookup.
///
/// # Example
///
/// ```
/// use orderbook::OrderBook;
/// use types::MarketId;
///
/// let mut book = OrderBook::new(MarketId::new(1));
/// assert!(book.is_empty());
/// assert_eq!(book.best_bid_price(), None);
/// assert_eq!(book.best_ask_price(), None);
/// ```
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// Market ID this order book belongs to.
    market_id: MarketId,

    /// Bid (buy) price levels, ordered by descending price.
    /// Uses `Reverse<Price>` so `BTreeMap` iteration gives highest price first.
    bids: BTreeMap<Reverse<Price>, PriceLevel>,

    /// Ask (sell) price levels, ordered by ascending price.
    /// Natural `BTreeMap` order gives lowest price first.
    asks: BTreeMap<Price, PriceLevel>,

    /// All orders indexed by ID for O(1) lookup.
    orders: HashMap<OrderId, Order>,
}

impl OrderBook {
    /// Creates a new empty order book for the given market.
    #[must_use]
    pub fn new(market_id: MarketId) -> Self {
        Self {
            market_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
        }
    }

    /// Creates a new order book with pre-allocated capacity for orders.
    #[must_use]
    pub fn with_capacity(market_id: MarketId, capacity: usize) -> Self {
        Self {
            market_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::with_capacity(capacity),
        }
    }

    /// Returns the market ID for this order book.
    #[must_use]
    pub const fn market_id(&self) -> MarketId {
        self.market_id
    }

    /// Returns `true` if the order book has no orders.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Returns the total number of orders in the book.
    #[must_use]
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    /// Returns the number of bid price levels.
    #[must_use]
    pub fn bid_level_count(&self) -> usize {
        self.bids.len()
    }

    /// Returns the number of ask price levels.
    #[must_use]
    pub fn ask_level_count(&self) -> usize {
        self.asks.len()
    }

    // =========================================================================
    // Best Price Access
    // =========================================================================

    /// Returns the best (highest) bid price, if any bids exist.
    #[must_use]
    pub fn best_bid_price(&self) -> Option<Price> {
        self.bids.first_key_value().map(|(k, _)| k.0)
    }

    /// Returns the best (lowest) ask price, if any asks exist.
    #[must_use]
    pub fn best_ask_price(&self) -> Option<Price> {
        self.asks.first_key_value().map(|(k, _)| *k)
    }

    /// Returns the best bid price level (highest price), if any.
    #[must_use]
    pub fn best_bid_level(&self) -> Option<&PriceLevel> {
        self.bids.first_key_value().map(|(_, level)| level)
    }

    /// Returns the best ask price level (lowest price), if any.
    #[must_use]
    pub fn best_ask_level(&self) -> Option<&PriceLevel> {
        self.asks.first_key_value().map(|(_, level)| level)
    }

    /// Returns a mutable reference to the best bid price level.
    pub fn best_bid_level_mut(&mut self) -> Option<&mut PriceLevel> {
        self.bids
            .first_entry()
            .map(std::collections::btree_map::OccupiedEntry::into_mut)
    }

    /// Returns a mutable reference to the best ask price level.
    pub fn best_ask_level_mut(&mut self) -> Option<&mut PriceLevel> {
        self.asks
            .first_entry()
            .map(std::collections::btree_map::OccupiedEntry::into_mut)
    }

    /// Returns the spread (best ask - best bid), if both exist.
    #[must_use]
    pub fn spread(&self) -> Option<Price> {
        let bid = self.best_bid_price()?;
        let ask = self.best_ask_price()?;
        ask.checked_sub(bid)
    }

    /// Returns the mid price ((best bid + best ask) / 2), if both exist.
    #[must_use]
    pub fn mid_price(&self) -> Option<Price> {
        let bid = self.best_bid_price()?;
        let ask = self.best_ask_price()?;
        let sum = bid.checked_add(ask)?;
        sum.checked_div_scalar(2)
    }

    // =========================================================================
    // Price Level Access
    // =========================================================================

    /// Returns the bid price level at the given price, if it exists.
    #[must_use]
    pub fn bid_level(&self, price: Price) -> Option<&PriceLevel> {
        self.bids.get(&Reverse(price))
    }

    /// Returns the ask price level at the given price, if it exists.
    #[must_use]
    pub fn ask_level(&self, price: Price) -> Option<&PriceLevel> {
        self.asks.get(&price)
    }

    /// Returns a mutable reference to the bid level at the given price.
    pub fn bid_level_mut(&mut self, price: Price) -> Option<&mut PriceLevel> {
        self.bids.get_mut(&Reverse(price))
    }

    /// Returns a mutable reference to the ask level at the given price.
    pub fn ask_level_mut(&mut self, price: Price) -> Option<&mut PriceLevel> {
        self.asks.get_mut(&price)
    }

    /// Returns the price level for the given side and price.
    #[must_use]
    pub fn level(&self, side: OrderSide, price: Price) -> Option<&PriceLevel> {
        match side {
            OrderSide::Buy => self.bid_level(price),
            OrderSide::Sell => self.ask_level(price),
        }
    }

    // =========================================================================
    // Order Access
    // =========================================================================

    /// Returns a reference to the order with the given ID.
    #[must_use]
    pub fn get_order(&self, order_id: OrderId) -> Option<&Order> {
        self.orders.get(&order_id)
    }

    /// Returns a mutable reference to the order with the given ID.
    pub fn get_order_mut(&mut self, order_id: OrderId) -> Option<&mut Order> {
        self.orders.get_mut(&order_id)
    }

    /// Returns `true` if the order book contains an order with the given ID.
    #[must_use]
    pub fn contains_order(&self, order_id: OrderId) -> bool {
        self.orders.contains_key(&order_id)
    }

    // =========================================================================
    // Iteration
    // =========================================================================

    /// Returns an iterator over bid price levels from best (highest) to worst (lowest).
    pub fn bid_levels(&self) -> impl Iterator<Item = &PriceLevel> {
        self.bids.values()
    }

    /// Returns an iterator over ask price levels from best (lowest) to worst (highest).
    pub fn ask_levels(&self) -> impl Iterator<Item = &PriceLevel> {
        self.asks.values()
    }

    /// Returns an iterator over all orders.
    pub fn orders(&self) -> impl Iterator<Item = &Order> {
        self.orders.values()
    }

    /// Returns an iterator over (price, level) pairs for bids.
    ///
    /// Iterates from best (highest) to worst (lowest) price.
    pub fn bids(&self) -> impl Iterator<Item = (Reverse<Price>, &PriceLevel)> {
        self.bids.iter().map(|(k, v)| (*k, v))
    }

    /// Returns an iterator over (price, level) pairs for asks.
    ///
    /// Iterates from best (lowest) to worst (highest) price.
    pub fn asks(&self) -> impl Iterator<Item = (Price, &PriceLevel)> {
        self.asks.iter().map(|(k, v)| (*k, v))
    }

    /// Returns the best bid (highest bid price), if any.
    ///
    /// Alias for `best_bid_price()`.
    #[must_use]
    pub fn best_bid(&self) -> Option<Price> {
        self.best_bid_price()
    }

    /// Returns the best ask (lowest ask price), if any.
    ///
    /// Alias for `best_ask_price()`.
    #[must_use]
    pub fn best_ask(&self) -> Option<Price> {
        self.best_ask_price()
    }

    // =========================================================================
    // Internal Helpers (used by ops module)
    // =========================================================================

    /// Returns a reference to the internal bids map.
    #[allow(dead_code)]
    pub(crate) fn bids_map(&self) -> &BTreeMap<Reverse<Price>, PriceLevel> {
        &self.bids
    }

    /// Returns a reference to the internal asks map.
    #[allow(dead_code)]
    pub(crate) fn asks_map(&self) -> &BTreeMap<Price, PriceLevel> {
        &self.asks
    }

    /// Returns a mutable reference to the internal bids map.
    pub(crate) fn bids_map_mut(&mut self) -> &mut BTreeMap<Reverse<Price>, PriceLevel> {
        &mut self.bids
    }

    /// Returns a mutable reference to the internal asks map.
    pub(crate) fn asks_map_mut(&mut self) -> &mut BTreeMap<Price, PriceLevel> {
        &mut self.asks
    }

    /// Returns a reference to the internal orders map.
    #[allow(dead_code)]
    pub(crate) fn orders_map(&self) -> &HashMap<OrderId, Order> {
        &self.orders
    }

    /// Returns a mutable reference to the internal orders map.
    pub(crate) fn orders_map_mut(&mut self) -> &mut HashMap<OrderId, Order> {
        &mut self.orders
    }

    // =========================================================================
    // Depth / Volume Summary
    // =========================================================================

    /// Returns the total bid quantity at all price levels.
    #[must_use]
    pub fn total_bid_quantity(&self) -> Quantity {
        self.bids.values().fold(Quantity::ZERO, |acc, level| {
            acc.checked_add(level.total_quantity()).unwrap_or(acc)
        })
    }

    /// Returns the total ask quantity at all price levels.
    #[must_use]
    pub fn total_ask_quantity(&self) -> Quantity {
        self.asks.values().fold(Quantity::ZERO, |acc, level| {
            acc.checked_add(level.total_quantity()).unwrap_or(acc)
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn market_id() -> MarketId {
        MarketId::new(1)
    }

    #[test]
    fn new_creates_empty_book() {
        let book = OrderBook::new(market_id());

        assert_eq!(book.market_id(), market_id());
        assert!(book.is_empty());
        assert_eq!(book.order_count(), 0);
        assert_eq!(book.bid_level_count(), 0);
        assert_eq!(book.ask_level_count(), 0);
    }

    #[test]
    fn with_capacity_creates_empty_book() {
        let book = OrderBook::with_capacity(market_id(), 100);

        assert!(book.is_empty());
        assert_eq!(book.market_id(), market_id());
    }

    #[test]
    fn best_prices_empty_returns_none() {
        let book = OrderBook::new(market_id());

        assert_eq!(book.best_bid_price(), None);
        assert_eq!(book.best_ask_price(), None);
        assert_eq!(book.spread(), None);
        assert_eq!(book.mid_price(), None);
    }

    #[test]
    fn get_order_not_found_returns_none() {
        let book = OrderBook::new(market_id());
        let order_id = OrderId::new(market_id(), 1);

        assert!(!book.contains_order(order_id));
        assert!(book.get_order(order_id).is_none());
    }

    #[test]
    fn bid_level_not_found_returns_none() {
        let book = OrderBook::new(market_id());
        let price = Price::from_whole(100);

        assert!(book.bid_level(price).is_none());
    }

    #[test]
    fn ask_level_not_found_returns_none() {
        let book = OrderBook::new(market_id());
        let price = Price::from_whole(100);

        assert!(book.ask_level(price).is_none());
    }

    #[test]
    fn total_quantities_empty_book() {
        let book = OrderBook::new(market_id());

        assert_eq!(book.total_bid_quantity(), Quantity::ZERO);
        assert_eq!(book.total_ask_quantity(), Quantity::ZERO);
    }
}
