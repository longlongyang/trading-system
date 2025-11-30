//! Order book snapshot for serialization.
//!
//! The [`OrderBookSnapshot`] provides a serializable representation of the
//! order book state. This is useful for:
//! - Persisting state to disk or a database
//! - Transmitting state over the network
//! - Reconstructing an order book from a checkpoint
//!
//! The snapshot captures the complete state needed to reconstruct the order
//! book, including all orders and their positions in the FIFO queues.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use types::{MarketId, Order, Price};

use crate::book::OrderBook;

// =============================================================================
// PriceLevelSnapshot
// =============================================================================

/// A serializable snapshot of a single price level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PriceLevelSnapshot {
    /// The price of this level.
    pub price: Price,

    /// Orders at this level in FIFO order.
    pub orders: Vec<Order>,
}

impl PriceLevelSnapshot {
    /// Creates a new price level snapshot.
    #[must_use]
    pub fn new(price: Price, orders: Vec<Order>) -> Self {
        Self { price, orders }
    }

    /// Returns `true` if this snapshot has no orders.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Returns the number of orders in this snapshot.
    #[must_use]
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }
}

// =============================================================================
// OrderBookSnapshot
// =============================================================================

/// A serializable snapshot of the complete order book state.
///
/// This captures all the information needed to reconstruct an order book:
/// - The market ID
/// - All bid price levels with their orders in FIFO order
/// - All ask price levels with their orders in FIFO order
///
/// # Example
///
/// ```
/// use orderbook::{OrderBook, OrderBookSnapshot};
/// use types::{
///     Address, MarketId, Nonce, Order, OrderSide, Price, Quantity,
///     TimeInForce, Timestamp,
/// };
///
/// let market_id = MarketId::new(1);
/// let mut book = OrderBook::new(market_id);
///
/// // Add some orders
/// let order = Order::new_limit(
///     types::OrderId::new(market_id, 1),
///     market_id,
///     Address::zero(),
///     OrderSide::Buy,
///     TimeInForce::GoodTilCancelled,
///     Price::from_whole(100),
///     Quantity::from_whole(10),
///     Nonce::new(1),
///     Timestamp::from_millis(1000),
/// );
/// book.insert_order(order).unwrap();
///
/// // Create a snapshot
/// let snapshot = OrderBookSnapshot::from_book(&book);
///
/// // Restore from snapshot
/// let restored = snapshot.to_book().unwrap();
/// assert_eq!(restored.order_count(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct OrderBookSnapshot {
    /// Market ID for this order book.
    pub market_id: MarketId,

    /// Bid price levels, from best (highest) to worst (lowest).
    pub bids: Vec<PriceLevelSnapshot>,

    /// Ask price levels, from best (lowest) to worst (highest).
    pub asks: Vec<PriceLevelSnapshot>,
}

impl OrderBookSnapshot {
    /// Creates a snapshot from an existing order book.
    #[must_use]
    pub fn from_book(book: &OrderBook) -> Self {
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        // Capture bid levels (already in descending price order due to Reverse<Price>)
        for level in book.bid_levels() {
            let orders = level
                .order_ids()
                .filter_map(|id| book.get_order(id).copied())
                .collect();

            bids.push(PriceLevelSnapshot {
                price: level.price(),
                orders,
            });
        }

        // Capture ask levels (already in ascending price order)
        for level in book.ask_levels() {
            let orders = level
                .order_ids()
                .filter_map(|id| book.get_order(id).copied())
                .collect();

            asks.push(PriceLevelSnapshot {
                price: level.price(),
                orders,
            });
        }

        Self {
            market_id: book.market_id(),
            bids,
            asks,
        }
    }

    /// Reconstructs an order book from this snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error string if reconstruction fails (e.g., invalid orders).
    pub fn to_book(&self) -> Result<OrderBook, String> {
        let mut book = OrderBook::new(self.market_id);

        // Restore bids
        for level_snapshot in &self.bids {
            for order in &level_snapshot.orders {
                book.insert_order(*order)
                    .map_err(|e| format!("failed to insert bid order: {e}"))?;
            }
        }

        // Restore asks
        for level_snapshot in &self.asks {
            for order in &level_snapshot.orders {
                book.insert_order(*order)
                    .map_err(|e| format!("failed to insert ask order: {e}"))?;
            }
        }

        Ok(book)
    }

    /// Returns `true` if this snapshot represents an empty order book.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Returns the total number of orders in this snapshot.
    #[must_use]
    pub fn order_count(&self) -> usize {
        let bid_count: usize = self.bids.iter().map(|l| l.orders.len()).sum();
        let ask_count: usize = self.asks.iter().map(|l| l.orders.len()).sum();
        bid_count + ask_count
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

    /// Returns the best bid price, if any.
    #[must_use]
    pub fn best_bid_price(&self) -> Option<Price> {
        self.bids.first().map(|l| l.price)
    }

    /// Returns the best ask price, if any.
    #[must_use]
    pub fn best_ask_price(&self) -> Option<Price> {
        self.asks.first().map(|l| l.price)
    }
}

// =============================================================================
// L2 Book Summary (Aggregated by Price)
// =============================================================================

/// A single level in an L2 (aggregated) order book view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Level {
    /// Price at this level.
    pub price: Price,

    /// Total quantity at this level.
    pub quantity: types::Quantity,

    /// Number of orders at this level.
    pub order_count: usize,
}

/// An L2 (aggregated by price) summary of the order book.
///
/// This is a common format for market data feeds, showing only
/// price and total quantity at each level without individual orders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2BookSummary {
    /// Market ID.
    pub market_id: MarketId,

    /// Bid levels from best to worst (descending price).
    pub bids: Vec<L2Level>,

    /// Ask levels from best to worst (ascending price).
    pub asks: Vec<L2Level>,
}

impl L2BookSummary {
    /// Creates an L2 summary from an order book.
    ///
    /// Optionally limits the depth to `max_levels` on each side.
    #[must_use]
    pub fn from_book(book: &OrderBook, max_levels: Option<usize>) -> Self {
        let max = max_levels.unwrap_or(usize::MAX);

        let bids: Vec<L2Level> = book
            .bid_levels()
            .take(max)
            .map(|level| L2Level {
                price: level.price(),
                quantity: level.total_quantity(),
                order_count: level.order_count(),
            })
            .collect();

        let asks: Vec<L2Level> = book
            .ask_levels()
            .take(max)
            .map(|level| L2Level {
                price: level.price(),
                quantity: level.total_quantity(),
                order_count: level.order_count(),
            })
            .collect();

        Self {
            market_id: book.market_id(),
            bids,
            asks,
        }
    }

    /// Creates an L2 summary from a snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: &OrderBookSnapshot, max_levels: Option<usize>) -> Self {
        let max = max_levels.unwrap_or(usize::MAX);

        let bids: Vec<L2Level> = snapshot
            .bids
            .iter()
            .take(max)
            .map(|level| {
                let total_qty = level.orders.iter().fold(types::Quantity::ZERO, |acc, o| {
                    acc.checked_add(o.remaining_quantity).unwrap_or(acc)
                });

                L2Level {
                    price: level.price,
                    quantity: total_qty,
                    order_count: level.orders.len(),
                }
            })
            .collect();

        let asks: Vec<L2Level> = snapshot
            .asks
            .iter()
            .take(max)
            .map(|level| {
                let total_qty = level.orders.iter().fold(types::Quantity::ZERO, |acc, o| {
                    acc.checked_add(o.remaining_quantity).unwrap_or(acc)
                });

                L2Level {
                    price: level.price,
                    quantity: total_qty,
                    order_count: level.orders.len(),
                }
            })
            .collect();

        Self {
            market_id: snapshot.market_id,
            bids,
            asks,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use types::{Address, Nonce, OrderId, OrderSide, Quantity, TimeInForce, Timestamp};

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

    fn setup_book() -> OrderBook {
        let mut book = OrderBook::new(market_id());

        // Add bids at prices 100, 99, 98
        book.insert_order(make_buy_order(1, 100, 10)).unwrap();
        book.insert_order(make_buy_order(2, 100, 20)).unwrap(); // Same price
        book.insert_order(make_buy_order(3, 99, 15)).unwrap();
        book.insert_order(make_buy_order(4, 98, 25)).unwrap();

        // Add asks at prices 101, 102, 103
        book.insert_order(make_sell_order(5, 101, 10)).unwrap();
        book.insert_order(make_sell_order(6, 102, 20)).unwrap();
        book.insert_order(make_sell_order(7, 102, 30)).unwrap(); // Same price
        book.insert_order(make_sell_order(8, 103, 25)).unwrap();

        book
    }

    // =========================================================================
    // Snapshot Creation Tests
    // =========================================================================

    #[test]
    fn snapshot_empty_book() {
        let book = OrderBook::new(market_id());
        let snapshot = OrderBookSnapshot::from_book(&book);

        assert_eq!(snapshot.market_id, market_id());
        assert!(snapshot.is_empty());
        assert_eq!(snapshot.order_count(), 0);
    }

    #[test]
    fn snapshot_captures_all_orders() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        assert_eq!(snapshot.order_count(), 8);
        assert_eq!(snapshot.bid_level_count(), 3);
        assert_eq!(snapshot.ask_level_count(), 3);
    }

    #[test]
    fn snapshot_preserves_price_ordering() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        // Bids should be descending
        let bid_prices: Vec<u128> = snapshot.bids.iter().map(|l| l.price.as_scaled()).collect();
        assert_eq!(
            bid_prices,
            vec![
                Price::from_whole(100).as_scaled(),
                Price::from_whole(99).as_scaled(),
                Price::from_whole(98).as_scaled(),
            ]
        );

        // Asks should be ascending
        let ask_prices: Vec<u128> = snapshot.asks.iter().map(|l| l.price.as_scaled()).collect();
        assert_eq!(
            ask_prices,
            vec![
                Price::from_whole(101).as_scaled(),
                Price::from_whole(102).as_scaled(),
                Price::from_whole(103).as_scaled(),
            ]
        );
    }

    #[test]
    fn snapshot_preserves_fifo_within_level() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        // At price 100, orders 1 and 2 should be in FIFO order
        let level_100 = &snapshot.bids[0];
        assert_eq!(level_100.orders.len(), 2);
        assert_eq!(level_100.orders[0].id, OrderId::new(market_id(), 1));
        assert_eq!(level_100.orders[1].id, OrderId::new(market_id(), 2));
    }

    // =========================================================================
    // Snapshot Restoration Tests
    // =========================================================================

    #[test]
    fn snapshot_round_trip() {
        let original = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&original);
        let restored = snapshot.to_book().unwrap();

        // Same number of orders
        assert_eq!(restored.order_count(), original.order_count());
        assert_eq!(restored.bid_level_count(), original.bid_level_count());
        assert_eq!(restored.ask_level_count(), original.ask_level_count());

        // Same best prices
        assert_eq!(restored.best_bid_price(), original.best_bid_price());
        assert_eq!(restored.best_ask_price(), original.best_ask_price());

        // Same total quantities
        assert_eq!(restored.total_bid_quantity(), original.total_bid_quantity());
        assert_eq!(restored.total_ask_quantity(), original.total_ask_quantity());
    }

    #[test]
    fn snapshot_round_trip_preserves_orders() {
        let original = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&original);
        let restored = snapshot.to_book().unwrap();

        // Check each order exists and matches
        for order in original.orders() {
            let restored_order = restored.get_order(order.id).expect("order should exist");
            assert_eq!(restored_order.id, order.id);
            assert_eq!(restored_order.price, order.price);
            assert_eq!(restored_order.quantity, order.quantity);
            assert_eq!(restored_order.side, order.side);
        }
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn snapshot_borsh_round_trip() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        let bytes = borsh::to_vec(&snapshot).unwrap();
        let decoded: OrderBookSnapshot = borsh::from_slice(&bytes).unwrap();

        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn snapshot_json_round_trip() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: OrderBookSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, snapshot);
    }

    // =========================================================================
    // L2 Summary Tests
    // =========================================================================

    #[test]
    fn l2_summary_from_book() {
        let book = setup_book();
        let summary = L2BookSummary::from_book(&book, None);

        assert_eq!(summary.market_id, market_id());
        assert_eq!(summary.bids.len(), 3);
        assert_eq!(summary.asks.len(), 3);

        // Check aggregated quantities at price 100 (orders 1: 10 + order 2: 20 = 30)
        assert_eq!(summary.bids[0].price, Price::from_whole(100));
        assert_eq!(summary.bids[0].quantity, Quantity::from_whole(30));
        assert_eq!(summary.bids[0].order_count, 2);

        // Check aggregated quantities at price 102 (orders 6: 20 + order 7: 30 = 50)
        assert_eq!(summary.asks[1].price, Price::from_whole(102));
        assert_eq!(summary.asks[1].quantity, Quantity::from_whole(50));
        assert_eq!(summary.asks[1].order_count, 2);
    }

    #[test]
    fn l2_summary_with_depth_limit() {
        let book = setup_book();
        let summary = L2BookSummary::from_book(&book, Some(2));

        assert_eq!(summary.bids.len(), 2);
        assert_eq!(summary.asks.len(), 2);

        // Should only have best 2 levels
        assert_eq!(summary.bids[0].price, Price::from_whole(100));
        assert_eq!(summary.bids[1].price, Price::from_whole(99));

        assert_eq!(summary.asks[0].price, Price::from_whole(101));
        assert_eq!(summary.asks[1].price, Price::from_whole(102));
    }

    #[test]
    fn l2_summary_from_snapshot() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);
        let summary = L2BookSummary::from_snapshot(&snapshot, None);

        assert_eq!(summary.bids.len(), 3);
        assert_eq!(summary.asks.len(), 3);

        // Same aggregation as from_book
        assert_eq!(summary.bids[0].quantity, Quantity::from_whole(30));
    }

    // =========================================================================
    // Best Price Tests
    // =========================================================================

    #[test]
    fn snapshot_best_prices() {
        let book = setup_book();
        let snapshot = OrderBookSnapshot::from_book(&book);

        assert_eq!(snapshot.best_bid_price(), Some(Price::from_whole(100)));
        assert_eq!(snapshot.best_ask_price(), Some(Price::from_whole(101)));
    }

    #[test]
    fn snapshot_best_prices_empty() {
        let book = OrderBook::new(market_id());
        let snapshot = OrderBookSnapshot::from_book(&book);

        assert_eq!(snapshot.best_bid_price(), None);
        assert_eq!(snapshot.best_ask_price(), None);
    }

    // =========================================================================
    // PriceLevelSnapshot Tests
    // =========================================================================

    #[test]
    fn price_level_snapshot_new() {
        let orders = vec![make_buy_order(1, 100, 10), make_buy_order(2, 100, 20)];
        let snapshot = PriceLevelSnapshot::new(Price::from_whole(100), orders);

        assert_eq!(snapshot.price, Price::from_whole(100));
        assert_eq!(snapshot.order_count(), 2);
        assert!(!snapshot.is_empty());
    }

    #[test]
    fn price_level_snapshot_empty() {
        let snapshot = PriceLevelSnapshot::new(Price::from_whole(100), vec![]);

        assert!(snapshot.is_empty());
        assert_eq!(snapshot.order_count(), 0);
    }
}
