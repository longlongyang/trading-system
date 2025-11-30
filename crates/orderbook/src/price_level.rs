//! Price level implementation with FIFO order queue.
//!
//! A [`PriceLevel`] represents all orders at a single price point. Orders at
//! the same price are stored in a FIFO queue (`VecDeque`) to maintain time priority.

use std::collections::VecDeque;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use types::{OrderId, Price, Quantity};

// =============================================================================
// PriceLevel
// =============================================================================

/// A single price level in the order book.
///
/// Contains all orders at a specific price, stored in FIFO order for
/// time priority during matching.
///
/// # Example
///
/// ```
/// use orderbook::PriceLevel;
/// use types::{MarketId, OrderId, Price, Quantity};
///
/// let price = Price::from_whole(100);
/// let mut level = PriceLevel::new(price);
///
/// let order_id = OrderId::new(MarketId::new(1), 1);
/// let qty = Quantity::from_whole(10);
///
/// level.push_back(order_id, qty);
/// assert_eq!(level.order_count(), 1);
/// assert_eq!(level.total_quantity(), qty);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PriceLevel {
    /// The price for this level.
    price: Price,

    /// Order IDs in FIFO order (front = oldest, back = newest).
    order_ids: VecDeque<OrderId>,

    /// Quantity for each order (parallel to `order_ids`).
    /// We store quantities here for O(1) total quantity tracking.
    quantities: VecDeque<Quantity>,

    /// Total quantity at this price level (cached for efficiency).
    total_quantity: Quantity,
}

impl PriceLevel {
    /// Creates a new empty price level at the given price.
    #[must_use]
    pub fn new(price: Price) -> Self {
        Self {
            price,
            order_ids: VecDeque::new(),
            quantities: VecDeque::new(),
            total_quantity: Quantity::ZERO,
        }
    }

    /// Creates a price level with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(price: Price, capacity: usize) -> Self {
        Self {
            price,
            order_ids: VecDeque::with_capacity(capacity),
            quantities: VecDeque::with_capacity(capacity),
            total_quantity: Quantity::ZERO,
        }
    }

    /// Returns the price of this level.
    #[must_use]
    pub const fn price(&self) -> Price {
        self.price
    }

    /// Returns the total quantity at this price level.
    #[must_use]
    pub const fn total_quantity(&self) -> Quantity {
        self.total_quantity
    }

    /// Returns the number of orders at this price level.
    #[must_use]
    pub fn order_count(&self) -> usize {
        self.order_ids.len()
    }

    /// Returns `true` if this price level has no orders.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order_ids.is_empty()
    }

    /// Adds an order to the back of the queue (newest).
    ///
    /// Returns `true` if the order was added successfully, `false` if the
    /// quantity addition would overflow (order not added in this case).
    pub fn push_back(&mut self, order_id: OrderId, quantity: Quantity) -> bool {
        if let Some(new_total) = self.total_quantity.checked_add(quantity) {
            self.order_ids.push_back(order_id);
            self.quantities.push_back(quantity);
            self.total_quantity = new_total;
            true
        } else {
            false
        }
    }

    /// Removes and returns the order at the front of the queue (oldest).
    ///
    /// Returns `None` if the level is empty.
    #[must_use]
    pub fn pop_front(&mut self) -> Option<(OrderId, Quantity)> {
        let order_id = self.order_ids.pop_front()?;
        let quantity = self.quantities.pop_front()?;
        // Safe: we only add valid quantities, so subtraction won't underflow
        self.total_quantity = self
            .total_quantity
            .checked_sub(quantity)
            .unwrap_or(Quantity::ZERO);
        Some((order_id, quantity))
    }

    /// Peeks at the order at the front of the queue without removing it.
    ///
    /// Returns `None` if the level is empty.
    #[must_use]
    pub fn front(&self) -> Option<(OrderId, Quantity)> {
        let order_id = *self.order_ids.front()?;
        let quantity = *self.quantities.front()?;
        Some((order_id, quantity))
    }

    /// Removes a specific order from the queue by its ID.
    ///
    /// This is O(n) where n is the number of orders at this level.
    /// Returns the quantity of the removed order, or `None` if not found.
    pub fn remove(&mut self, order_id: OrderId) -> Option<Quantity> {
        let index = self.order_ids.iter().position(|&id| id == order_id)?;

        self.order_ids.remove(index);
        let quantity = self.quantities.remove(index)?;
        self.total_quantity = self
            .total_quantity
            .checked_sub(quantity)
            .unwrap_or(Quantity::ZERO);

        Some(quantity)
    }

    /// Updates the quantity of an order at this level.
    ///
    /// Returns the old quantity if the order was found and updated,
    /// or `None` if the order wasn't found.
    pub fn update_quantity(
        &mut self,
        order_id: OrderId,
        new_quantity: Quantity,
    ) -> Option<Quantity> {
        let index = self.order_ids.iter().position(|&id| id == order_id)?;

        let old_quantity = self.quantities[index];

        // Update total: subtract old, add new
        self.total_quantity = self
            .total_quantity
            .checked_sub(old_quantity)
            .unwrap_or(Quantity::ZERO);

        if let Some(new_total) = self.total_quantity.checked_add(new_quantity) {
            self.quantities[index] = new_quantity;
            self.total_quantity = new_total;
            Some(old_quantity)
        } else {
            // Overflow, restore old state
            self.total_quantity = self
                .total_quantity
                .checked_add(old_quantity)
                .unwrap_or(old_quantity);
            None
        }
    }

    /// Returns an iterator over the order IDs at this level (front to back).
    pub fn order_ids(&self) -> impl Iterator<Item = OrderId> + '_ {
        self.order_ids.iter().copied()
    }

    /// Returns an iterator over (`order_id`, quantity) pairs at this level.
    pub fn orders(&self) -> impl Iterator<Item = (OrderId, Quantity)> + '_ {
        self.order_ids
            .iter()
            .copied()
            .zip(self.quantities.iter().copied())
    }

    /// Returns `true` if this level contains the given order ID.
    #[must_use]
    pub fn contains(&self, order_id: OrderId) -> bool {
        self.order_ids.contains(&order_id)
    }

    /// Returns the quantity for a specific order, if present.
    #[must_use]
    pub fn get_quantity(&self, order_id: OrderId) -> Option<Quantity> {
        let index = self.order_ids.iter().position(|&id| id == order_id)?;
        Some(self.quantities[index])
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use types::MarketId;

    fn make_order_id(seq: u64) -> OrderId {
        OrderId::new(MarketId::new(1), seq)
    }

    fn price(value: u64) -> Price {
        Price::from_whole(value)
    }

    fn qty(value: u64) -> Quantity {
        Quantity::from_whole(value)
    }

    // =========================================================================
    // Construction Tests
    // =========================================================================

    #[test]
    fn new_creates_empty_level() {
        let level = PriceLevel::new(price(100));

        assert_eq!(level.price(), price(100));
        assert_eq!(level.order_count(), 0);
        assert!(level.is_empty());
        assert_eq!(level.total_quantity(), Quantity::ZERO);
    }

    #[test]
    fn with_capacity_creates_empty_level() {
        let level = PriceLevel::with_capacity(price(100), 10);

        assert_eq!(level.price(), price(100));
        assert!(level.is_empty());
    }

    // =========================================================================
    // Push/Pop Tests
    // =========================================================================

    #[test]
    fn push_back_adds_order() {
        let mut level = PriceLevel::new(price(100));

        assert!(level.push_back(make_order_id(1), qty(10)));

        assert_eq!(level.order_count(), 1);
        assert_eq!(level.total_quantity(), qty(10));
        assert!(!level.is_empty());
    }

    #[test]
    fn push_back_multiple_orders() {
        let mut level = PriceLevel::new(price(100));

        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));
        level.push_back(make_order_id(3), qty(30));

        assert_eq!(level.order_count(), 3);
        assert_eq!(level.total_quantity(), qty(60));
    }

    #[test]
    fn pop_front_returns_oldest_first() {
        let mut level = PriceLevel::new(price(100));

        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));
        level.push_back(make_order_id(3), qty(30));

        let (id1, q1) = level.pop_front().unwrap();
        assert_eq!(id1, make_order_id(1));
        assert_eq!(q1, qty(10));

        let (id2, q2) = level.pop_front().unwrap();
        assert_eq!(id2, make_order_id(2));
        assert_eq!(q2, qty(20));

        assert_eq!(level.order_count(), 1);
        assert_eq!(level.total_quantity(), qty(30));
    }

    #[test]
    fn pop_front_empty_returns_none() {
        let mut level = PriceLevel::new(price(100));
        assert!(level.pop_front().is_none());
    }

    #[test]
    fn front_peeks_without_removing() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let (id, q) = level.front().unwrap();
        assert_eq!(id, make_order_id(1));
        assert_eq!(q, qty(10));

        // Still there
        assert_eq!(level.order_count(), 2);
    }

    #[test]
    fn front_empty_returns_none() {
        let level = PriceLevel::new(price(100));
        assert!(level.front().is_none());
    }

    // =========================================================================
    // Remove Tests
    // =========================================================================

    #[test]
    fn remove_from_middle() {
        let mut level = PriceLevel::new(price(100));

        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));
        level.push_back(make_order_id(3), qty(30));

        let removed = level.remove(make_order_id(2));
        assert_eq!(removed, Some(qty(20)));
        assert_eq!(level.order_count(), 2);
        assert_eq!(level.total_quantity(), qty(40));

        // Order 1 and 3 should remain in FIFO order
        let (id1, _) = level.pop_front().unwrap();
        assert_eq!(id1, make_order_id(1));

        let (id3, _) = level.pop_front().unwrap();
        assert_eq!(id3, make_order_id(3));
    }

    #[test]
    fn remove_front() {
        let mut level = PriceLevel::new(price(100));

        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let removed = level.remove(make_order_id(1));
        assert_eq!(removed, Some(qty(10)));

        let (id, _) = level.front().unwrap();
        assert_eq!(id, make_order_id(2));
    }

    #[test]
    fn remove_back() {
        let mut level = PriceLevel::new(price(100));

        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let removed = level.remove(make_order_id(2));
        assert_eq!(removed, Some(qty(20)));
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn remove_not_found() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        let removed = level.remove(make_order_id(999));
        assert!(removed.is_none());
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn remove_last_order_makes_empty() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        level.remove(make_order_id(1));
        assert!(level.is_empty());
        assert_eq!(level.total_quantity(), Quantity::ZERO);
    }

    // =========================================================================
    // Update Quantity Tests
    // =========================================================================

    #[test]
    fn update_quantity_increases() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        let old = level.update_quantity(make_order_id(1), qty(25));
        assert_eq!(old, Some(qty(10)));
        assert_eq!(level.total_quantity(), qty(25));
        assert_eq!(level.get_quantity(make_order_id(1)), Some(qty(25)));
    }

    #[test]
    fn update_quantity_decreases() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        let old = level.update_quantity(make_order_id(1), qty(5));
        assert_eq!(old, Some(qty(10)));
        assert_eq!(level.total_quantity(), qty(5));
    }

    #[test]
    fn update_quantity_not_found() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        let result = level.update_quantity(make_order_id(999), qty(20));
        assert!(result.is_none());
        assert_eq!(level.total_quantity(), qty(10));
    }

    // =========================================================================
    // Query Tests
    // =========================================================================

    #[test]
    fn contains_returns_true_for_existing() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));

        assert!(level.contains(make_order_id(1)));
        assert!(!level.contains(make_order_id(2)));
    }

    #[test]
    fn get_quantity_returns_correct_value() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        assert_eq!(level.get_quantity(make_order_id(1)), Some(qty(10)));
        assert_eq!(level.get_quantity(make_order_id(2)), Some(qty(20)));
        assert_eq!(level.get_quantity(make_order_id(3)), None);
    }

    // =========================================================================
    // Iterator Tests
    // =========================================================================

    #[test]
    fn order_ids_iterates_in_fifo_order() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));
        level.push_back(make_order_id(3), qty(30));

        let ids: Vec<_> = level.order_ids().collect();
        assert_eq!(
            ids,
            vec![make_order_id(1), make_order_id(2), make_order_id(3)]
        );
    }

    #[test]
    fn orders_iterates_pairs() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let pairs: Vec<_> = level.orders().collect();
        assert_eq!(
            pairs,
            vec![(make_order_id(1), qty(10)), (make_order_id(2), qty(20)),]
        );
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn borsh_round_trip() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let bytes = borsh::to_vec(&level).unwrap();
        let decoded: PriceLevel = borsh::from_slice(&bytes).unwrap();

        assert_eq!(decoded, level);
    }

    #[test]
    fn serde_json_round_trip() {
        let mut level = PriceLevel::new(price(100));
        level.push_back(make_order_id(1), qty(10));
        level.push_back(make_order_id(2), qty(20));

        let json = serde_json::to_string(&level).unwrap();
        let decoded: PriceLevel = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, level);
    }
}
