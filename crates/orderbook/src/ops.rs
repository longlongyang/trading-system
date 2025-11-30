//! Order book operations: insert, remove, and update orders.
//!
//! This module provides the core operations for manipulating the order book.
//! The operations maintain consistency between the price level structures
//! and the order HashMap.

use std::cmp::Reverse;

use types::{Order, OrderId, OrderSide, OrderStatus, Quantity};

use crate::book::OrderBook;
use crate::price_level::PriceLevel;

// =============================================================================
// Error Type
// =============================================================================

/// Errors that can occur during order book operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderBookError {
    /// The order already exists in the book.
    OrderAlreadyExists,

    /// The order was not found in the book.
    OrderNotFound,

    /// The order belongs to a different market.
    WrongMarket,

    /// The price level is full (quantity overflow).
    PriceLevelFull,

    /// Market orders cannot be inserted into the book.
    MarketOrderCannotRest,

    /// The order is not in an active state.
    OrderNotActive,
}

impl std::fmt::Display for OrderBookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OrderAlreadyExists => write!(f, "order already exists in book"),
            Self::OrderNotFound => write!(f, "order not found in book"),
            Self::WrongMarket => write!(f, "order belongs to different market"),
            Self::PriceLevelFull => write!(f, "price level quantity overflow"),
            Self::MarketOrderCannotRest => write!(f, "market orders cannot rest in book"),
            Self::OrderNotActive => write!(f, "order is not in active state"),
        }
    }
}

impl std::error::Error for OrderBookError {}

/// Result type for order book operations.
pub type Result<T> = std::result::Result<T, OrderBookError>;

// =============================================================================
// Insert Operation
// =============================================================================

impl OrderBook {
    /// Inserts an order into the order book.
    ///
    /// The order is added to the appropriate price level based on its side
    /// and price. The order must be a limit order (market orders execute
    /// immediately and don't rest in the book).
    ///
    /// # Errors
    ///
    /// - [`OrderBookError::OrderAlreadyExists`] if an order with the same ID exists
    /// - [`OrderBookError::WrongMarket`] if the order's market doesn't match
    /// - [`OrderBookError::MarketOrderCannotRest`] if attempting to insert a market order
    /// - [`OrderBookError::OrderNotActive`] if the order is not in an active state
    /// - [`OrderBookError::PriceLevelFull`] if adding would overflow the price level
    ///
    /// # Example
    ///
    /// ```
    /// use orderbook::OrderBook;
    /// use types::{
    ///     Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity,
    ///     TimeInForce, Timestamp,
    /// };
    ///
    /// let market_id = MarketId::new(1);
    /// let mut book = OrderBook::new(market_id);
    ///
    /// let order = Order::new_limit(
    ///     OrderId::new(market_id, 1),
    ///     market_id,
    ///     Address::zero(),
    ///     OrderSide::Buy,
    ///     TimeInForce::GoodTilCancelled,
    ///     Price::from_whole(100),
    ///     Quantity::from_whole(10),
    ///     Nonce::new(1),
    ///     Timestamp::from_millis(1000),
    /// );
    ///
    /// book.insert_order(order).unwrap();
    /// assert_eq!(book.order_count(), 1);
    /// assert_eq!(book.best_bid_price(), Some(Price::from_whole(100)));
    /// ```
    pub fn insert_order(&mut self, order: Order) -> Result<()> {
        // Validate the order
        if order.market_id != self.market_id() {
            return Err(OrderBookError::WrongMarket);
        }

        if order.is_market() {
            return Err(OrderBookError::MarketOrderCannotRest);
        }

        if !order.status.is_active() {
            return Err(OrderBookError::OrderNotActive);
        }

        if self.contains_order(order.id) {
            return Err(OrderBookError::OrderAlreadyExists);
        }

        // Insert into the appropriate side
        let price = order.price;
        let quantity = order.remaining_quantity;

        match order.side {
            OrderSide::Buy => {
                let level = self
                    .bids_map_mut()
                    .entry(Reverse(price))
                    .or_insert_with(|| PriceLevel::new(price));

                if !level.push_back(order.id, quantity) {
                    return Err(OrderBookError::PriceLevelFull);
                }
            }
            OrderSide::Sell => {
                let level = self
                    .asks_map_mut()
                    .entry(price)
                    .or_insert_with(|| PriceLevel::new(price));

                if !level.push_back(order.id, quantity) {
                    return Err(OrderBookError::PriceLevelFull);
                }
            }
        }

        // Insert into the orders HashMap
        self.orders_map_mut().insert(order.id, order);

        Ok(())
    }
}

// =============================================================================
// Remove Operation
// =============================================================================

impl OrderBook {
    /// Removes an order from the order book by its ID.
    ///
    /// The order is removed from both the price level and the orders HashMap.
    /// If removing the order empties the price level, the level is removed.
    ///
    /// # Errors
    ///
    /// - [`OrderBookError::OrderNotFound`] if the order doesn't exist
    ///
    /// # Example
    ///
    /// ```
    /// use orderbook::OrderBook;
    /// use types::{
    ///     Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity,
    ///     TimeInForce, Timestamp,
    /// };
    ///
    /// let market_id = MarketId::new(1);
    /// let mut book = OrderBook::new(market_id);
    ///
    /// let order_id = OrderId::new(market_id, 1);
    /// let order = Order::new_limit(
    ///     order_id,
    ///     market_id,
    ///     Address::zero(),
    ///     OrderSide::Buy,
    ///     TimeInForce::GoodTilCancelled,
    ///     Price::from_whole(100),
    ///     Quantity::from_whole(10),
    ///     Nonce::new(1),
    ///     Timestamp::from_millis(1000),
    /// );
    ///
    /// book.insert_order(order).unwrap();
    /// let removed = book.remove_order(order_id).unwrap();
    ///
    /// assert_eq!(removed.id, order_id);
    /// assert!(book.is_empty());
    /// ```
    pub fn remove_order(&mut self, order_id: OrderId) -> Result<Order> {
        // Remove from orders HashMap first to get the order details
        let order = self
            .orders_map_mut()
            .remove(&order_id)
            .ok_or(OrderBookError::OrderNotFound)?;

        let price = order.price;

        // Remove from the appropriate price level
        match order.side {
            OrderSide::Buy => {
                if let Some(level) = self.bids_map_mut().get_mut(&Reverse(price)) {
                    level.remove(order_id);

                    // Remove empty price levels
                    if level.is_empty() {
                        self.bids_map_mut().remove(&Reverse(price));
                    }
                }
            }
            OrderSide::Sell => {
                if let Some(level) = self.asks_map_mut().get_mut(&price) {
                    level.remove(order_id);

                    // Remove empty price levels
                    if level.is_empty() {
                        self.asks_map_mut().remove(&price);
                    }
                }
            }
        }

        Ok(order)
    }

    /// Removes all orders from the book, returning them as a Vec.
    pub fn clear(&mut self) -> Vec<Order> {
        self.bids_map_mut().clear();
        self.asks_map_mut().clear();
        self.orders_map_mut().drain().map(|(_, o)| o).collect()
    }
}

// =============================================================================
// Update Operations
// =============================================================================

impl OrderBook {
    /// Updates the remaining quantity of an order.
    ///
    /// This is typically used after a partial fill. The order remains in
    /// its current position in the FIFO queue (no loss of time priority).
    ///
    /// # Errors
    ///
    /// - [`OrderBookError::OrderNotFound`] if the order doesn't exist
    /// - [`OrderBookError::PriceLevelFull`] if the new quantity would overflow
    ///
    /// # Example
    ///
    /// ```
    /// use orderbook::OrderBook;
    /// use types::{
    ///     Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity,
    ///     TimeInForce, Timestamp,
    /// };
    ///
    /// let market_id = MarketId::new(1);
    /// let mut book = OrderBook::new(market_id);
    ///
    /// let order_id = OrderId::new(market_id, 1);
    /// let order = Order::new_limit(
    ///     order_id,
    ///     market_id,
    ///     Address::zero(),
    ///     OrderSide::Buy,
    ///     TimeInForce::GoodTilCancelled,
    ///     Price::from_whole(100),
    ///     Quantity::from_whole(10),
    ///     Nonce::new(1),
    ///     Timestamp::from_millis(1000),
    /// );
    ///
    /// book.insert_order(order).unwrap();
    /// book.update_order_quantity(order_id, Quantity::from_whole(5)).unwrap();
    ///
    /// let updated = book.get_order(order_id).unwrap();
    /// assert_eq!(updated.remaining_quantity, Quantity::from_whole(5));
    /// ```
    pub fn update_order_quantity(
        &mut self,
        order_id: OrderId,
        new_quantity: Quantity,
    ) -> Result<Quantity> {
        // Get the order to find its side and price
        let order = self
            .get_order(order_id)
            .ok_or(OrderBookError::OrderNotFound)?;

        let side = order.side;
        let price = order.price;
        let old_quantity = order.remaining_quantity;

        // Update the price level
        match side {
            OrderSide::Buy => {
                let level = self
                    .bids_map_mut()
                    .get_mut(&Reverse(price))
                    .ok_or(OrderBookError::OrderNotFound)?;

                level
                    .update_quantity(order_id, new_quantity)
                    .ok_or(OrderBookError::PriceLevelFull)?;
            }
            OrderSide::Sell => {
                let level = self
                    .asks_map_mut()
                    .get_mut(&price)
                    .ok_or(OrderBookError::OrderNotFound)?;

                level
                    .update_quantity(order_id, new_quantity)
                    .ok_or(OrderBookError::PriceLevelFull)?;
            }
        }

        // Update the order in the HashMap
        if let Some(order) = self.orders_map_mut().get_mut(&order_id) {
            order.remaining_quantity = new_quantity;

            // Update status if fully filled
            if new_quantity.is_zero() {
                order.status = OrderStatus::Filled;
            } else if new_quantity < order.quantity {
                order.status = OrderStatus::PartiallyFilled;
            }
        }

        Ok(old_quantity)
    }

    /// Marks an order as cancelled without removing it from the book.
    ///
    /// This updates the order's status but keeps it in the data structure.
    /// To fully remove it, call `remove_order` after this.
    ///
    /// # Errors
    ///
    /// - [`OrderBookError::OrderNotFound`] if the order doesn't exist
    pub fn cancel_order(&mut self, order_id: OrderId) -> Result<()> {
        let order = self
            .orders_map_mut()
            .get_mut(&order_id)
            .ok_or(OrderBookError::OrderNotFound)?;

        order.status = OrderStatus::Cancelled;
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use types::{Address, MarketId, Nonce, Price, Quantity, TimeInForce, Timestamp};

    fn market_id() -> MarketId {
        MarketId::new(1)
    }

    fn make_buy_order(seq: u64, price: u64, qty: u64) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            Address::zero(),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    fn make_sell_order(seq: u64, price: u64, qty: u64) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            Address::zero(),
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    // =========================================================================
    // Insert Tests
    // =========================================================================

    #[test]
    fn insert_single_buy_order() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);

        book.insert_order(order).unwrap();

        assert_eq!(book.order_count(), 1);
        assert_eq!(book.bid_level_count(), 1);
        assert_eq!(book.ask_level_count(), 0);
        assert_eq!(book.best_bid_price(), Some(Price::from_whole(100)));
    }

    #[test]
    fn insert_single_sell_order() {
        let mut book = OrderBook::new(market_id());
        let order = make_sell_order(1, 100, 10);

        book.insert_order(order).unwrap();

        assert_eq!(book.order_count(), 1);
        assert_eq!(book.bid_level_count(), 0);
        assert_eq!(book.ask_level_count(), 1);
        assert_eq!(book.best_ask_price(), Some(Price::from_whole(100)));
    }

    #[test]
    fn insert_multiple_buy_orders_different_prices() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();
        book.insert_order(make_buy_order(3, 95, 15)).unwrap();

        assert_eq!(book.order_count(), 3);
        assert_eq!(book.bid_level_count(), 3);

        // Best bid should be highest price
        assert_eq!(book.best_bid_price(), Some(Price::from_whole(105)));
    }

    #[test]
    fn insert_multiple_sell_orders_different_prices() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_sell_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 105, 20)).unwrap();
        book.insert_order(make_sell_order(3, 95, 15)).unwrap();

        assert_eq!(book.order_count(), 3);
        assert_eq!(book.ask_level_count(), 3);

        // Best ask should be lowest price
        assert_eq!(book.best_ask_price(), Some(Price::from_whole(95)));
    }

    #[test]
    fn insert_multiple_orders_same_price() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 100, 20)).unwrap();
        book.insert_order(make_buy_order(3, 100, 30)).unwrap();

        assert_eq!(book.order_count(), 3);
        assert_eq!(book.bid_level_count(), 1);

        let level = book.best_bid_level().unwrap();
        assert_eq!(level.order_count(), 3);
        assert_eq!(level.total_quantity(), Quantity::from_whole(60));
    }

    #[test]
    fn insert_duplicate_order_fails() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);

        book.insert_order(order).unwrap();
        let result = book.insert_order(order);

        assert_eq!(result, Err(OrderBookError::OrderAlreadyExists));
        assert_eq!(book.order_count(), 1);
    }

    #[test]
    fn insert_wrong_market_fails() {
        let mut book = OrderBook::new(market_id());

        let wrong_market = MarketId::new(999);
        let order = Order::new_limit(
            OrderId::new(wrong_market, 1),
            wrong_market,
            Address::zero(),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );

        let result = book.insert_order(order);
        assert_eq!(result, Err(OrderBookError::WrongMarket));
    }

    #[test]
    fn insert_market_order_fails() {
        let mut book = OrderBook::new(market_id());

        let order = Order::new_market(
            OrderId::new(market_id(), 1),
            market_id(),
            Address::zero(),
            OrderSide::Buy,
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );

        let result = book.insert_order(order);
        assert_eq!(result, Err(OrderBookError::MarketOrderCannotRest));
    }

    #[test]
    fn insert_cancelled_order_fails() {
        let mut book = OrderBook::new(market_id());

        let mut order = make_buy_order(1, 100, 10);
        order.status = OrderStatus::Cancelled;

        let result = book.insert_order(order);
        assert_eq!(result, Err(OrderBookError::OrderNotActive));
    }

    // =========================================================================
    // Remove Tests
    // =========================================================================

    #[test]
    fn remove_order_success() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);
        let order_id = order.id;

        book.insert_order(order).unwrap();
        let removed = book.remove_order(order_id).unwrap();

        assert_eq!(removed.id, order_id);
        assert!(book.is_empty());
        assert_eq!(book.bid_level_count(), 0);
    }

    #[test]
    fn remove_order_not_found() {
        let mut book = OrderBook::new(market_id());
        let order_id = OrderId::new(market_id(), 999);

        let result = book.remove_order(order_id);
        assert_eq!(result, Err(OrderBookError::OrderNotFound));
    }

    #[test]
    fn remove_order_from_multi_order_level() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 100, 20)).unwrap();
        book.insert_order(make_buy_order(3, 100, 30)).unwrap();

        // Remove middle order
        let order_id = OrderId::new(market_id(), 2);
        book.remove_order(order_id).unwrap();

        assert_eq!(book.order_count(), 2);
        assert_eq!(book.bid_level_count(), 1);

        let level = book.best_bid_level().unwrap();
        assert_eq!(level.order_count(), 2);
        assert_eq!(level.total_quantity(), Quantity::from_whole(40));
    }

    #[test]
    fn remove_last_order_removes_level() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();

        // Remove the order at price 100
        let order_id = OrderId::new(market_id(), 1);
        book.remove_order(order_id).unwrap();

        assert_eq!(book.order_count(), 1);
        assert_eq!(book.bid_level_count(), 1);
        assert_eq!(book.best_bid_price(), Some(Price::from_whole(105)));
    }

    // =========================================================================
    // Clear Tests
    // =========================================================================

    #[test]
    fn clear_removes_all_orders() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();
        book.insert_order(make_sell_order(3, 110, 15)).unwrap();

        let removed = book.clear();

        assert_eq!(removed.len(), 3);
        assert!(book.is_empty());
        assert_eq!(book.bid_level_count(), 0);
        assert_eq!(book.ask_level_count(), 0);
    }

    // =========================================================================
    // Update Quantity Tests
    // =========================================================================

    #[test]
    fn update_quantity_success() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);
        let order_id = order.id;

        book.insert_order(order).unwrap();

        let old_qty = book
            .update_order_quantity(order_id, Quantity::from_whole(5))
            .unwrap();

        assert_eq!(old_qty, Quantity::from_whole(10));

        let updated = book.get_order(order_id).unwrap();
        assert_eq!(updated.remaining_quantity, Quantity::from_whole(5));
        assert_eq!(updated.status, OrderStatus::PartiallyFilled);
    }

    #[test]
    fn update_quantity_to_zero_marks_filled() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);
        let order_id = order.id;

        book.insert_order(order).unwrap();
        book.update_order_quantity(order_id, Quantity::ZERO)
            .unwrap();

        let updated = book.get_order(order_id).unwrap();
        assert_eq!(updated.status, OrderStatus::Filled);
    }

    #[test]
    fn update_quantity_not_found() {
        let mut book = OrderBook::new(market_id());
        let order_id = OrderId::new(market_id(), 999);

        let result = book.update_order_quantity(order_id, Quantity::from_whole(5));
        assert_eq!(result, Err(OrderBookError::OrderNotFound));
    }

    #[test]
    fn update_quantity_updates_level_total() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 100, 20)).unwrap();

        let order_id = OrderId::new(market_id(), 1);
        book.update_order_quantity(order_id, Quantity::from_whole(5))
            .unwrap();

        let level = book.best_bid_level().unwrap();
        assert_eq!(level.total_quantity(), Quantity::from_whole(25));
    }

    // =========================================================================
    // Cancel Order Tests
    // =========================================================================

    #[test]
    fn cancel_order_success() {
        let mut book = OrderBook::new(market_id());
        let order = make_buy_order(1, 100, 10);
        let order_id = order.id;

        book.insert_order(order).unwrap();
        book.cancel_order(order_id).unwrap();

        let cancelled = book.get_order(order_id).unwrap();
        assert_eq!(cancelled.status, OrderStatus::Cancelled);

        // Order still in book until removed
        assert_eq!(book.order_count(), 1);
    }

    #[test]
    fn cancel_order_not_found() {
        let mut book = OrderBook::new(market_id());
        let order_id = OrderId::new(market_id(), 999);

        let result = book.cancel_order(order_id);
        assert_eq!(result, Err(OrderBookError::OrderNotFound));
    }

    // =========================================================================
    // Spread and Mid Price Tests
    // =========================================================================

    #[test]
    fn spread_calculation() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 105, 10)).unwrap();

        let spread = book.spread().unwrap();
        assert_eq!(spread, Price::from_whole(5));
    }

    #[test]
    fn mid_price_calculation() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 102, 10)).unwrap();

        let mid = book.mid_price().unwrap();
        assert_eq!(mid, Price::from_whole(101));
    }

    // =========================================================================
    // Total Quantity Tests
    // =========================================================================

    #[test]
    fn total_bid_quantity() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();
        book.insert_order(make_buy_order(3, 100, 30)).unwrap();

        assert_eq!(book.total_bid_quantity(), Quantity::from_whole(60));
    }

    #[test]
    fn total_ask_quantity() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_sell_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 105, 20)).unwrap();

        assert_eq!(book.total_ask_quantity(), Quantity::from_whole(30));
    }

    // =========================================================================
    // Price Level Access Tests
    // =========================================================================

    #[test]
    fn get_specific_level() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();

        let level = book.bid_level(Price::from_whole(100)).unwrap();
        assert_eq!(level.price(), Price::from_whole(100));
        assert_eq!(level.total_quantity(), Quantity::from_whole(10));
    }

    #[test]
    fn level_by_side() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 105, 20)).unwrap();

        let bid_level = book.level(OrderSide::Buy, Price::from_whole(100)).unwrap();
        assert_eq!(bid_level.total_quantity(), Quantity::from_whole(10));

        let ask_level = book.level(OrderSide::Sell, Price::from_whole(105)).unwrap();
        assert_eq!(ask_level.total_quantity(), Quantity::from_whole(20));
    }

    // =========================================================================
    // Iteration Tests
    // =========================================================================

    #[test]
    fn bid_levels_iterate_descending() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 105, 20)).unwrap();
        book.insert_order(make_buy_order(3, 95, 15)).unwrap();

        let prices: Vec<Price> = book.bid_levels().map(|l| l.price()).collect();
        assert_eq!(
            prices,
            vec![
                Price::from_whole(105),
                Price::from_whole(100),
                Price::from_whole(95),
            ]
        );
    }

    #[test]
    fn ask_levels_iterate_ascending() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_sell_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 105, 20)).unwrap();
        book.insert_order(make_sell_order(3, 95, 15)).unwrap();

        let prices: Vec<Price> = book.ask_levels().map(|l| l.price()).collect();
        assert_eq!(
            prices,
            vec![
                Price::from_whole(95),
                Price::from_whole(100),
                Price::from_whole(105),
            ]
        );
    }

    // =========================================================================
    // FIFO Order Tests
    // =========================================================================

    #[test]
    fn fifo_order_maintained() {
        let mut book = OrderBook::new(market_id());

        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 100, 20)).unwrap();
        book.insert_order(make_buy_order(3, 100, 30)).unwrap();

        let level = book.best_bid_level().unwrap();
        let order_ids: Vec<OrderId> = level.order_ids().collect();

        assert_eq!(
            order_ids,
            vec![
                OrderId::new(market_id(), 1),
                OrderId::new(market_id(), 2),
                OrderId::new(market_id(), 3),
            ]
        );
    }
}
