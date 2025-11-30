//! Matching engine implementation.
//!
//! The [`MatchingEngine`] is the main entry point for order processing.
//! It coordinates matching logic, order book updates, and handles different
//! time-in-force policies.

use orderbook::OrderBook;
use types::{Order, OrderId, OrderStatus, Quantity, TimeInForce};

use crate::error::MatchingError;
use crate::matcher::{self, available_quantity, available_quantity_market, would_cross};
use crate::result::{CancelResult, ExecutionReport, MatchOutcome, MatchResult};

// =============================================================================
// MatchingEngine
// =============================================================================

/// The core matching engine.
///
/// The matching engine processes incoming orders against an order book,
/// generating fills and updating book state. It implements price-time
/// priority matching with support for various time-in-force policies.
///
/// # Design
///
/// The engine is stateless - it operates on an external `OrderBook` reference.
/// This allows flexibility in how the order book is managed (single book,
/// multiple books, persistence, etc.).
///
/// # Example
///
/// ```
/// use matching::MatchingEngine;
/// use orderbook::OrderBook;
/// use types::{
///     Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity,
///     TimeInForce, Timestamp,
/// };
///
/// let market_id = MarketId::new(1);
/// let mut book = OrderBook::new(market_id);
/// let engine = MatchingEngine::new();
///
/// // Place a resting sell order
/// let sell = Order::new_limit(
///     OrderId::new(market_id, 1),
///     market_id,
///     Address::zero(),
///     OrderSide::Sell,
///     TimeInForce::GoodTilCancelled,
///     Price::from_whole(100),
///     Quantity::from_whole(10),
///     Nonce::new(1),
///     Timestamp::from_millis(1000),
/// );
/// let result = engine.place_order(&mut book, sell);
/// assert!(result.is_ok());
/// assert_eq!(book.order_count(), 1);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MatchingEngine {
    // The engine is stateless - all state is in the OrderBook.
    // This struct exists for API organization and future extensibility
    // (e.g., configuration, metrics).
    _private: (),
}

impl MatchingEngine {
    /// Creates a new matching engine.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Places an order into the order book, matching against existing orders.
    ///
    /// This is the main entry point for order processing. It handles:
    /// - Matching the order against the opposite side of the book
    /// - Applying time-in-force rules (GTC, IOC, FOK, `PostOnly`)
    /// - Updating the order book state
    /// - Generating an execution report
    ///
    /// # Arguments
    ///
    /// * `book` - The order book to place the order in
    /// * `order` - The order to place
    ///
    /// # Returns
    ///
    /// An `ExecutionReport` describing what happened, or an error.
    ///
    /// # Errors
    ///
    /// - [`MatchingError::WrongMarket`] if the order's market doesn't match
    /// - [`MatchingError::OrderAlreadyExists`] if the order ID is duplicate
    /// - [`MatchingError::PostOnlyWouldCross`] if a `PostOnly` order would match
    /// - [`MatchingError::FillOrKillNotFilled`] if a FOK order can't be fully filled
    pub fn place_order(&self, book: &mut OrderBook, order: Order) -> MatchResult {
        // Validate market
        if order.market_id != book.market_id() {
            return Err(MatchingError::WrongMarket { order_id: order.id });
        }

        // Check for duplicate
        if book.contains_order(order.id) {
            return Err(MatchingError::OrderAlreadyExists { order_id: order.id });
        }

        // Dispatch based on time-in-force
        match order.time_in_force {
            TimeInForce::GoodTilCancelled => Self::place_gtc(book, order),
            TimeInForce::ImmediateOrCancel => Self::place_ioc(book, order),
            TimeInForce::FillOrKill => Self::place_fok(book, order),
            TimeInForce::PostOnly => Self::place_post_only(book, order),
        }
    }

    /// Cancels an order from the order book.
    ///
    /// # Arguments
    ///
    /// * `book` - The order book
    /// * `order_id` - The ID of the order to cancel
    ///
    /// # Returns
    ///
    /// A `CancelResult` describing what happened.
    pub fn cancel_order(&self, book: &mut OrderBook, order_id: OrderId) -> CancelResult {
        // Try to get the order
        let Some(order) = book.get_order(order_id).copied() else {
            return CancelResult::not_found(order_id);
        };

        // Check if already terminal
        if order.status.is_terminal() {
            return CancelResult::already_terminal(order_id, order);
        }

        let cancelled_quantity = order.remaining_quantity;

        // Remove from book
        match book.remove_order(order_id) {
            Ok(mut removed_order) => {
                removed_order.status = OrderStatus::Cancelled;
                CancelResult::cancelled(order_id, removed_order, cancelled_quantity)
            }
            Err(_) => CancelResult::not_found(order_id),
        }
    }

    // =========================================================================
    // Time-in-Force Handlers
    // =========================================================================

    /// Handles GTC (Good-til-Cancelled) orders.
    ///
    /// GTC orders match as much as possible against the book, then rest
    /// any remaining quantity.
    fn place_gtc(book: &mut OrderBook, mut order: Order) -> MatchResult {
        let timestamp = order.created_at;

        // For market orders, treat as IOC
        if order.is_market() {
            return Self::place_ioc(book, order);
        }

        // Try to match
        let match_result = matcher::match_order(book, &order, order.remaining_quantity, timestamp);

        // Apply fills to the book
        Self::apply_match_result(book, &match_result)?;

        // Calculate filled quantity
        let filled_quantity = order
            .remaining_quantity
            .checked_sub(match_result.remaining_quantity)
            .unwrap_or(Quantity::ZERO);

        // Update order state
        order.remaining_quantity = match_result.remaining_quantity;

        let outcome = if match_result.remaining_quantity.is_zero() {
            // Fully filled
            order.status = OrderStatus::Filled;
            MatchOutcome::Filled
        } else if !match_result.fills.is_empty() {
            // Partially filled, rest in book
            order.status = OrderStatus::PartiallyFilled;
            book.insert_order(order)?;
            MatchOutcome::PartiallyFilledResting
        } else {
            // No fills, rest in book
            book.insert_order(order)?;
            MatchOutcome::Resting
        };

        Ok(ExecutionReport::new(
            order,
            match_result.fills,
            filled_quantity,
            match_result.remaining_quantity,
            outcome,
            timestamp,
        ))
    }

    /// Handles IOC (Immediate-or-Cancel) orders.
    ///
    /// IOC orders match as much as possible, then cancel any remainder.
    /// They never rest in the book.
    fn place_ioc(book: &mut OrderBook, mut order: Order) -> MatchResult {
        let timestamp = order.created_at;

        // Try to match
        let max_qty = order.remaining_quantity;

        let match_result = matcher::match_order(book, &order, max_qty, timestamp);

        // Apply fills to the book
        Self::apply_match_result(book, &match_result)?;

        // Calculate filled quantity
        let filled_quantity = order
            .remaining_quantity
            .checked_sub(match_result.remaining_quantity)
            .unwrap_or(Quantity::ZERO);

        // Update order state - IOC orders are always terminal
        order.remaining_quantity = match_result.remaining_quantity;

        let outcome = if match_result.remaining_quantity.is_zero() && !match_result.fills.is_empty()
        {
            // Fully filled
            order.status = OrderStatus::Filled;
            MatchOutcome::Filled
        } else if !match_result.fills.is_empty() {
            // Partially filled, remainder cancelled
            order.status = OrderStatus::Cancelled;
            MatchOutcome::PartiallyFilledCancelled
        } else {
            // No fills, cancelled
            order.status = OrderStatus::Cancelled;
            MatchOutcome::Cancelled
        };

        Ok(ExecutionReport::new(
            order,
            match_result.fills,
            filled_quantity,
            match_result.remaining_quantity,
            outcome,
            timestamp,
        ))
    }

    /// Handles FOK (Fill-or-Kill) orders.
    ///
    /// FOK orders must be completely filled or they are rejected entirely.
    /// No partial fills, no resting.
    fn place_fok(book: &mut OrderBook, mut order: Order) -> MatchResult {
        let timestamp = order.created_at;
        let required_quantity = order.remaining_quantity;

        // Check if we can fill the entire order
        let available = if order.is_market() {
            available_quantity_market(book, order.side)
        } else {
            available_quantity(book, order.side, order.price)
        };

        if available < required_quantity {
            // Cannot fill entirely - reject
            return Err(MatchingError::FillOrKillNotFilled {
                requested: required_quantity,
                available,
            });
        }

        // Try to match - should fill entirely
        let match_result = matcher::match_order(book, &order, required_quantity, timestamp);

        // Verify we actually got full fill
        if !match_result.remaining_quantity.is_zero() {
            // This shouldn't happen if available_quantity was correct
            // (could happen due to self-trade prevention)
            return Err(MatchingError::FillOrKillNotFilled {
                requested: required_quantity,
                available: required_quantity
                    .checked_sub(match_result.remaining_quantity)
                    .unwrap_or(Quantity::ZERO),
            });
        }

        // Apply fills to the book
        Self::apply_match_result(book, &match_result)?;

        // Update order state
        order.remaining_quantity = Quantity::ZERO;
        order.status = OrderStatus::Filled;

        Ok(ExecutionReport::new(
            order,
            match_result.fills,
            required_quantity,
            Quantity::ZERO,
            MatchOutcome::Filled,
            timestamp,
        ))
    }

    /// Handles `PostOnly` (maker-only) orders.
    ///
    /// `PostOnly` orders are rejected if they would immediately match.
    /// They can only add liquidity (rest in the book).
    fn place_post_only(book: &mut OrderBook, order: Order) -> MatchResult {
        let timestamp = order.created_at;

        // Market orders cannot be PostOnly
        if order.is_market() {
            return Err(MatchingError::MarketOrderPostOnly);
        }

        // Check if order would cross
        if would_cross(book, order.side, order.price) {
            return Err(MatchingError::PostOnlyWouldCross);
        }

        // No crossing - add to book
        book.insert_order(order)?;

        Ok(ExecutionReport::new(
            order,
            vec![],
            Quantity::ZERO,
            order.remaining_quantity,
            MatchOutcome::Resting,
            timestamp,
        ))
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Applies the result of matching to the order book.
    ///
    /// This updates or removes maker orders that were matched.
    fn apply_match_result(
        book: &mut OrderBook,
        result: &matcher::MatchResult,
    ) -> Result<(), MatchingError> {
        // Remove fully consumed orders
        for order_id in &result.consumed_orders {
            book.remove_order(*order_id)?;
        }

        // Update partially filled orders
        for (order_id, new_quantity) in &result.partial_fills {
            book.update_order_quantity(*order_id, *new_quantity)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::CancelOutcome;
    use types::{Address, MarketId, Nonce, OrderSide, Price, Quantity, Timestamp};

    fn market_id() -> MarketId {
        MarketId::new(1)
    }

    fn maker_address() -> Address {
        Address::new([1u8; 20])
    }

    fn taker_address() -> Address {
        Address::new([2u8; 20])
    }

    fn make_limit_order(
        seq: u64,
        side: OrderSide,
        price: u64,
        qty: u64,
        tif: TimeInForce,
        owner: Address,
    ) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            owner,
            side,
            tif,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    fn make_market_order(seq: u64, side: OrderSide, qty: u64, owner: Address) -> Order {
        Order::new_market(
            OrderId::new(market_id(), seq),
            market_id(),
            owner,
            side,
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    // =========================================================================
    // GTC Tests
    // =========================================================================

    #[test]
    fn gtc_resting_order_no_match() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let result = engine.place_order(&mut book, sell).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Resting);
        assert!(result.fills.is_empty());
        assert_eq!(result.remaining_quantity, Quantity::from_whole(10));
        assert_eq!(book.order_count(), 1);
    }

    #[test]
    fn gtc_full_fill_single_order() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Place crossing buy
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Filled);
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.filled_quantity, Quantity::from_whole(10));
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(book.order_count(), 0); // Both orders consumed
    }

    #[test]
    fn gtc_partial_fill_resting() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell for 5
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Place crossing buy for 10 - should fill 5, rest 5
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::PartiallyFilledResting);
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.filled_quantity, Quantity::from_whole(5));
        assert_eq!(result.remaining_quantity, Quantity::from_whole(5));
        assert_eq!(book.order_count(), 1); // Buy order resting
    }

    #[test]
    fn gtc_multi_level_fill() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place multiple resting sells at different prices
        let sell1 = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let sell2 = make_limit_order(
            2,
            OrderSide::Sell,
            101,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell1).unwrap();
        engine.place_order(&mut book, sell2).unwrap();

        // Buy 10 at 101 - should fill both levels
        let buy = make_limit_order(
            3,
            OrderSide::Buy,
            101,
            10,
            TimeInForce::GoodTilCancelled,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Filled);
        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.filled_quantity, Quantity::from_whole(10));

        // First fill at 100, second at 101
        assert_eq!(result.fills[0].price, Price::from_whole(100));
        assert_eq!(result.fills[1].price, Price::from_whole(101));
    }

    #[test]
    fn gtc_non_crossing_order() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell at 100
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Place buy at 99 - should not cross
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            99,
            10,
            TimeInForce::GoodTilCancelled,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Resting);
        assert!(result.fills.is_empty());
        assert_eq!(book.order_count(), 2);
    }

    // =========================================================================
    // IOC Tests
    // =========================================================================

    #[test]
    fn ioc_full_fill() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // IOC buy that fully fills
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::ImmediateOrCancel,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Filled);
        assert_eq!(result.fills.len(), 1);
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn ioc_partial_fill_cancelled() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell for 5
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // IOC buy for 10 - fills 5, cancels 5
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::ImmediateOrCancel,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::PartiallyFilledCancelled);
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.filled_quantity, Quantity::from_whole(5));
        assert_eq!(result.remaining_quantity, Quantity::from_whole(5));
        assert_eq!(book.order_count(), 0); // IOC doesn't rest
    }

    #[test]
    fn ioc_no_fill_cancelled() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell at 100
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // IOC buy at 99 - no cross, cancelled
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            99,
            10,
            TimeInForce::ImmediateOrCancel,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Cancelled);
        assert!(result.fills.is_empty());
        assert_eq!(book.order_count(), 1); // Only the sell remains
    }

    // =========================================================================
    // FOK Tests
    // =========================================================================

    #[test]
    fn fok_full_fill() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // FOK buy that can fill
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::FillOrKill,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Filled);
        assert_eq!(result.fills.len(), 1);
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn fok_not_enough_liquidity() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell for 5
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // FOK buy for 10 - not enough liquidity
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::FillOrKill,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy);

        assert!(matches!(
            result,
            Err(MatchingError::FillOrKillNotFilled { .. })
        ));
        assert_eq!(book.order_count(), 1); // Sell still resting
    }

    #[test]
    fn fok_no_cross_rejected() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell at 100
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // FOK buy at 99 - can't fill
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            99,
            10,
            TimeInForce::FillOrKill,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy);

        assert!(matches!(
            result,
            Err(MatchingError::FillOrKillNotFilled { .. })
        ));
    }

    // =========================================================================
    // PostOnly Tests
    // =========================================================================

    #[test]
    fn post_only_resting() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell at 100
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // PostOnly buy at 99 - doesn't cross, rests
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            99,
            10,
            TimeInForce::PostOnly,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Resting);
        assert!(result.fills.is_empty());
        assert_eq!(book.order_count(), 2);
    }

    #[test]
    fn post_only_would_cross_rejected() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell at 100
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // PostOnly buy at 100 - would cross, rejected
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::PostOnly,
            taker_address(),
        );
        let result = engine.place_order(&mut book, buy);

        assert!(matches!(result, Err(MatchingError::PostOnlyWouldCross)));
        assert_eq!(book.order_count(), 1); // Only sell remains
    }

    // =========================================================================
    // Market Order Tests
    // =========================================================================

    #[test]
    fn market_order_full_fill() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sells
        let sell1 = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let sell2 = make_limit_order(
            2,
            OrderSide::Sell,
            101,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell1).unwrap();
        engine.place_order(&mut book, sell2).unwrap();

        // Market buy for 10
        let buy = make_market_order(3, OrderSide::Buy, 10, taker_address());
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Filled);
        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.filled_quantity, Quantity::from_whole(10));
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn market_order_partial_fill() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place resting sell for 5
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Market buy for 10 - only 5 available
        let buy = make_market_order(2, OrderSide::Buy, 10, taker_address());
        let result = engine.place_order(&mut book, buy).unwrap();

        // Market orders don't rest, remainder is cancelled
        assert_eq!(result.outcome, MatchOutcome::PartiallyFilledCancelled);
        assert_eq!(result.filled_quantity, Quantity::from_whole(5));
        assert_eq!(result.remaining_quantity, Quantity::from_whole(5));
    }

    #[test]
    fn market_order_no_liquidity() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Market buy with no asks
        let buy = make_market_order(1, OrderSide::Buy, 10, taker_address());
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.outcome, MatchOutcome::Cancelled);
        assert!(result.fills.is_empty());
    }

    // =========================================================================
    // Cancel Tests
    // =========================================================================

    #[test]
    fn cancel_resting_order() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        let result = engine.cancel_order(&mut book, OrderId::new(market_id(), 1));

        assert!(result.is_cancelled());
        assert_eq!(result.cancelled_quantity, Quantity::from_whole(10));
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn cancel_not_found() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        let result = engine.cancel_order(&mut book, OrderId::new(market_id(), 999));

        assert_eq!(result.outcome, CancelOutcome::NotFound);
    }

    // =========================================================================
    // Self-Trade Prevention Tests
    // =========================================================================

    #[test]
    fn self_trade_prevention_skips_own_orders() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place sell and buy from same owner
        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Same owner tries to buy
        let buy = make_limit_order(
            2,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        // No fills, both orders rest
        assert!(result.fills.is_empty());
        assert_eq!(result.outcome, MatchOutcome::Resting);
        assert_eq!(book.order_count(), 2);
    }

    #[test]
    fn self_trade_prevention_matches_other_orders() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        // Place sells from two owners
        let sell1 = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let sell2 = make_limit_order(
            2,
            OrderSide::Sell,
            100,
            5,
            TimeInForce::GoodTilCancelled,
            taker_address(),
        );
        engine.place_order(&mut book, sell1).unwrap();
        engine.place_order(&mut book, sell2).unwrap();

        // Maker buys - should skip own sell, match taker's sell
        let buy = make_limit_order(
            3,
            OrderSide::Buy,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.filled_quantity, Quantity::from_whole(5));
        assert_eq!(result.outcome, MatchOutcome::PartiallyFilledResting);

        // maker_address sell (seq 1) still in book
        // taker_address sell (seq 2) consumed
        // maker_address buy (seq 3) partially resting
        assert_eq!(book.order_count(), 2);
    }

    // =========================================================================
    // Error Cases
    // =========================================================================

    #[test]
    fn wrong_market_rejected() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        let wrong_market = MarketId::new(999);
        let order = Order::new_limit(
            OrderId::new(wrong_market, 1),
            wrong_market,
            maker_address(),
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );

        let result = engine.place_order(&mut book, order);
        assert!(matches!(result, Err(MatchingError::WrongMarket { .. })));
    }

    #[test]
    fn duplicate_order_rejected() {
        let mut book = OrderBook::new(market_id());
        let engine = MatchingEngine::new();

        let sell = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Try to place same order again
        let sell2 = make_limit_order(
            1,
            OrderSide::Sell,
            100,
            10,
            TimeInForce::GoodTilCancelled,
            maker_address(),
        );
        let result = engine.place_order(&mut book, sell2);

        assert!(matches!(
            result,
            Err(MatchingError::OrderAlreadyExists { .. })
        ));
    }
}
