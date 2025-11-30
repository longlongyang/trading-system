//! Result types for matching engine operations.
//!
//! This module defines the structures returned by matching operations:
//! - [`MatchFill`]: A single fill event from matching
//! - [`ExecutionReport`]: Complete report of order placement
//! - [`MatchResult`]: Result type for `place_order` operations
//! - [`CancelResult`]: Result type for cancel operations

use types::{Address, Order, OrderId, OrderSide, Price, Quantity, Timestamp};

use crate::error::MatchingError;

// =============================================================================
// MatchFill
// =============================================================================

/// A single fill generated when two orders match.
///
/// This is a lightweight fill structure focused on the matching result.
/// It does not include fee information (which is calculated in Stage 5).
///
/// The fill is always from the **taker's perspective** - the taker order
/// triggered this match by crossing the spread.
///
/// # Price Determination
///
/// The execution price is always the **maker's price** (the resting order's
/// price), following standard exchange semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchFill {
    /// The maker order ID (was resting in the book).
    pub maker_order_id: OrderId,

    /// The taker order ID (crossed the spread).
    pub taker_order_id: OrderId,

    /// Address of the maker.
    pub maker: Address,

    /// Address of the taker.
    pub taker: Address,

    /// Execution price (the maker's price).
    pub price: Price,

    /// Quantity filled.
    pub quantity: Quantity,

    /// Side of the taker order.
    ///
    /// If `Buy`, the taker bought (took from asks).
    /// If `Sell`, the taker sold (took from bids).
    pub taker_side: OrderSide,

    /// Timestamp when the fill occurred.
    pub timestamp: Timestamp,
}

impl MatchFill {
    /// Creates a new match fill.
    #[must_use]
    pub const fn new(
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker: Address,
        taker: Address,
        price: Price,
        quantity: Quantity,
        taker_side: OrderSide,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            maker_order_id,
            taker_order_id,
            maker,
            taker,
            price,
            quantity,
            taker_side,
            timestamp,
        }
    }

    /// Returns the notional value of this fill (price × quantity).
    ///
    /// Returns `None` on overflow.
    #[must_use]
    pub fn notional(&self) -> Option<Quantity> {
        self.quantity.checked_mul_price(self.price)
    }
}

// =============================================================================
// MatchOutcome
// =============================================================================

/// The outcome of attempting to match/place an order.
///
/// This enum describes what happened to the incoming order:
/// - Was it fully filled?
/// - Was it partially filled and resting?
/// - Is it now resting with no fills?
/// - Was it cancelled (IOC/FOK semantics)?
/// - Was it rejected (`PostOnly` crossed)?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOutcome {
    /// Order was completely filled. No remainder.
    Filled,

    /// Order was partially filled, remainder is resting in the book.
    PartiallyFilledResting,

    /// Order was partially filled, remainder was cancelled (IOC).
    PartiallyFilledCancelled,

    /// Order is resting in the book with no fills (non-crossing limit).
    Resting,

    /// Order was cancelled with no fills (IOC that didn't cross).
    Cancelled,

    /// Order was rejected (`PostOnly` that would have crossed).
    RejectedPostOnly,

    /// Order was rejected (FOK that couldn't be fully filled).
    RejectedFillOrKill,
}

impl MatchOutcome {
    /// Returns `true` if the order has any fills.
    #[must_use]
    pub const fn has_fills(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::PartiallyFilledResting | Self::PartiallyFilledCancelled
        )
    }

    /// Returns `true` if the order is now resting in the book.
    #[must_use]
    pub const fn is_resting(self) -> bool {
        matches!(self, Self::PartiallyFilledResting | Self::Resting)
    }

    /// Returns `true` if the order was rejected.
    #[must_use]
    pub const fn is_rejected(self) -> bool {
        matches!(self, Self::RejectedPostOnly | Self::RejectedFillOrKill)
    }

    /// Returns `true` if the order is in a terminal state (not resting).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled
                | Self::PartiallyFilledCancelled
                | Self::Cancelled
                | Self::RejectedPostOnly
                | Self::RejectedFillOrKill
        )
    }
}

// =============================================================================
// ExecutionReport
// =============================================================================

/// Complete report of an order placement/matching attempt.
///
/// This structure captures everything that happened when processing an order:
/// - The fills generated (if any)
/// - The final outcome
/// - How much was filled vs. remaining
/// - Whether the order is resting in the book
///
/// # Example
///
/// ```
/// use matching::{ExecutionReport, MatchOutcome};
/// use types::Quantity;
///
/// // A report for a fully filled order
/// fn process_report(report: ExecutionReport) {
///     match report.outcome {
///         MatchOutcome::Filled => {
///             println!("Fully filled with {} trades", report.fills.len());
///         }
///         MatchOutcome::PartiallyFilledResting => {
///             println!("Partially filled, {} remaining", report.remaining_quantity);
///         }
///         _ => {}
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    /// The order that was processed.
    pub order: Order,

    /// Fills generated by this order (empty if no matches).
    pub fills: Vec<MatchFill>,

    /// Total quantity filled across all fills.
    pub filled_quantity: Quantity,

    /// Remaining unfilled quantity.
    pub remaining_quantity: Quantity,

    /// The outcome of the order placement.
    pub outcome: MatchOutcome,

    /// Timestamp when processing completed.
    pub timestamp: Timestamp,
}

impl ExecutionReport {
    /// Creates a new execution report.
    #[must_use]
    pub fn new(
        order: Order,
        fills: Vec<MatchFill>,
        filled_quantity: Quantity,
        remaining_quantity: Quantity,
        outcome: MatchOutcome,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            order,
            fills,
            filled_quantity,
            remaining_quantity,
            outcome,
            timestamp,
        }
    }

    /// Returns the number of fills.
    #[must_use]
    pub fn fill_count(&self) -> usize {
        self.fills.len()
    }

    /// Returns `true` if there were any fills.
    #[must_use]
    pub fn has_fills(&self) -> bool {
        !self.fills.is_empty()
    }

    /// Returns `true` if the order is fully filled.
    #[must_use]
    pub fn is_fully_filled(&self) -> bool {
        self.outcome == MatchOutcome::Filled
    }

    /// Returns `true` if the order is now resting in the book.
    #[must_use]
    pub fn is_resting(&self) -> bool {
        self.outcome.is_resting()
    }

    /// Returns the average fill price, if there were fills.
    ///
    /// Computed as total notional / total quantity.
    #[must_use]
    pub fn average_price(&self) -> Option<Price> {
        if self.fills.is_empty() {
            return None;
        }

        // Sum up weighted price (price * qty for each fill, but we need to be
        // careful about overflow since both are scaled by 10^18)
        //
        // Average price = sum(price_i * qty_i) / sum(qty_i)
        //
        // Since price and qty are both scaled by SCALE, we compute:
        // sum((price_i / SCALE) * qty_i) / sum(qty_i) * SCALE
        // which avoids overflow in the multiplication.
        //
        // Alternatively, we can do: sum(price_i * (qty_i / SCALE)) / sum(qty_i / SCALE)
        // but this loses precision. A better approach is to sum price * qty / SCALE
        // for each fill, then divide by total qty / SCALE.

        let scale = types::SCALE;
        let mut weighted_sum: u128 = 0;
        let mut total_qty: u128 = 0;

        for fill in &self.fills {
            // price * qty / SCALE to get notional in scaled form
            let price_scaled = fill.price.as_scaled();
            let qty_scaled = fill.quantity.as_scaled();

            // (price * qty) / SCALE - use checked mul that handles this
            if let Some(notional) = fill.quantity.checked_mul_price(fill.price) {
                weighted_sum = weighted_sum.checked_add(notional.as_scaled())?;
            } else {
                // Overflow in multiplication, try alternative calculation
                // price * (qty / SCALE) - loses precision but won't overflow
                let qty_whole = qty_scaled / scale;
                let product = price_scaled.checked_mul(qty_whole)?;
                weighted_sum = weighted_sum.checked_add(product)?;
            }

            total_qty = total_qty.checked_add(qty_scaled)?;
        }

        if total_qty == 0 {
            return None;
        }

        // Average price = weighted_sum * SCALE / total_qty
        // weighted_sum is in scaled units (notional), total_qty is in scaled units
        // So avg price = weighted_sum / total_qty * SCALE
        let avg = weighted_sum.checked_mul(scale)?.checked_div(total_qty)?;

        Some(Price::from_scaled(avg))
    }
}

// =============================================================================
// OrderPlacement
// =============================================================================

/// Information about a successfully placed/resting order.
///
/// This is returned when an order (or its remainder) is added to the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderPlacement {
    /// The order ID.
    pub order_id: OrderId,

    /// The price at which the order is resting.
    pub price: Price,

    /// The quantity resting in the book.
    pub quantity: Quantity,

    /// The side of the order.
    pub side: OrderSide,

    /// Timestamp when the order was placed.
    pub timestamp: Timestamp,
}

// =============================================================================
// Result Types
// =============================================================================

/// Result type for order placement operations.
pub type MatchResult = Result<ExecutionReport, MatchingError>;

// =============================================================================
// Cancel Types
// =============================================================================

/// The outcome of a cancel request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// Order was successfully cancelled.
    Cancelled,

    /// Order was already in a terminal state (filled, cancelled, etc.).
    AlreadyTerminal,

    /// Order was not found.
    NotFound,
}

/// Result of a cancel operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelResult {
    /// The order ID that was cancelled (or attempted).
    pub order_id: OrderId,

    /// The outcome of the cancel attempt.
    pub outcome: CancelOutcome,

    /// The order, if it was found (even if already terminal).
    pub order: Option<Order>,

    /// Remaining quantity that was cancelled (0 if not found or already terminal).
    pub cancelled_quantity: Quantity,
}

impl CancelResult {
    /// Creates a successful cancel result.
    #[must_use]
    pub fn cancelled(order_id: OrderId, order: Order, cancelled_quantity: Quantity) -> Self {
        Self {
            order_id,
            outcome: CancelOutcome::Cancelled,
            order: Some(order),
            cancelled_quantity,
        }
    }

    /// Creates a result for an already-terminal order.
    #[must_use]
    pub fn already_terminal(order_id: OrderId, order: Order) -> Self {
        Self {
            order_id,
            outcome: CancelOutcome::AlreadyTerminal,
            order: Some(order),
            cancelled_quantity: Quantity::ZERO,
        }
    }

    /// Creates a result for a not-found order.
    #[must_use]
    pub fn not_found(order_id: OrderId) -> Self {
        Self {
            order_id,
            outcome: CancelOutcome::NotFound,
            order: None,
            cancelled_quantity: Quantity::ZERO,
        }
    }

    /// Returns `true` if the cancel was successful.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.outcome == CancelOutcome::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{MarketId, Nonce, TimeInForce};

    fn make_order() -> Order {
        Order::new_limit(
            OrderId::new(MarketId::new(1), 1),
            MarketId::new(1),
            Address::zero(),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(10), // Use smaller values
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        )
    }

    #[test]
    fn match_fill_notional() {
        let fill = MatchFill::new(
            OrderId::new(MarketId::new(1), 1),
            OrderId::new(MarketId::new(1), 2),
            Address::zero(),
            Address::zero(),
            Price::from_whole(2), // Use smaller values to avoid overflow
            Quantity::from_whole(5),
            OrderSide::Buy,
            Timestamp::from_millis(1000),
        );

        // 2 * 5 = 10
        let notional = fill.notional().unwrap();
        assert_eq!(notional, Quantity::from_whole(10));
    }

    #[test]
    fn match_outcome_has_fills() {
        assert!(MatchOutcome::Filled.has_fills());
        assert!(MatchOutcome::PartiallyFilledResting.has_fills());
        assert!(MatchOutcome::PartiallyFilledCancelled.has_fills());
        assert!(!MatchOutcome::Resting.has_fills());
        assert!(!MatchOutcome::Cancelled.has_fills());
        assert!(!MatchOutcome::RejectedPostOnly.has_fills());
        assert!(!MatchOutcome::RejectedFillOrKill.has_fills());
    }

    #[test]
    fn match_outcome_is_resting() {
        assert!(!MatchOutcome::Filled.is_resting());
        assert!(MatchOutcome::PartiallyFilledResting.is_resting());
        assert!(!MatchOutcome::PartiallyFilledCancelled.is_resting());
        assert!(MatchOutcome::Resting.is_resting());
        assert!(!MatchOutcome::Cancelled.is_resting());
    }

    #[test]
    fn match_outcome_is_rejected() {
        assert!(!MatchOutcome::Filled.is_rejected());
        assert!(MatchOutcome::RejectedPostOnly.is_rejected());
        assert!(MatchOutcome::RejectedFillOrKill.is_rejected());
    }

    #[test]
    fn execution_report_average_price_single_fill() {
        let order = make_order();
        let fills = vec![MatchFill::new(
            OrderId::new(MarketId::new(1), 1),
            OrderId::new(MarketId::new(1), 2),
            Address::zero(),
            Address::zero(),
            Price::from_whole(10), // Use smaller values to avoid overflow
            Quantity::from_whole(10),
            OrderSide::Buy,
            Timestamp::from_millis(1000),
        )];

        let report = ExecutionReport::new(
            order,
            fills,
            Quantity::from_whole(10),
            Quantity::ZERO,
            MatchOutcome::Filled,
            Timestamp::from_millis(1000),
        );

        assert_eq!(report.average_price(), Some(Price::from_whole(10)));
    }

    #[test]
    fn execution_report_average_price_multiple_fills() {
        let order = make_order();
        let fills = vec![
            MatchFill::new(
                OrderId::new(MarketId::new(1), 1),
                OrderId::new(MarketId::new(1), 3),
                Address::zero(),
                Address::zero(),
                Price::from_whole(10), // 5 @ 10 = 50
                Quantity::from_whole(5),
                OrderSide::Buy,
                Timestamp::from_millis(1000),
            ),
            MatchFill::new(
                OrderId::new(MarketId::new(1), 2),
                OrderId::new(MarketId::new(1), 3),
                Address::zero(),
                Address::zero(),
                Price::from_whole(12), // 5 @ 12 = 60
                Quantity::from_whole(5),
                OrderSide::Buy,
                Timestamp::from_millis(1000),
            ),
        ];

        // Total: 10 qty, 110 notional -> avg = 11
        let report = ExecutionReport::new(
            order,
            fills,
            Quantity::from_whole(10),
            Quantity::ZERO,
            MatchOutcome::Filled,
            Timestamp::from_millis(1000),
        );

        assert_eq!(report.average_price(), Some(Price::from_whole(11)));
    }

    #[test]
    fn execution_report_no_fills() {
        let order = make_order();
        let report = ExecutionReport::new(
            order,
            vec![],
            Quantity::ZERO,
            Quantity::from_whole(10),
            MatchOutcome::Resting,
            Timestamp::from_millis(1000),
        );

        assert!(!report.has_fills());
        assert_eq!(report.average_price(), None);
    }

    #[test]
    fn cancel_result_success() {
        let order = make_order();
        let result = CancelResult::cancelled(order.id, order, Quantity::from_whole(10));

        assert!(result.is_cancelled());
        assert_eq!(result.cancelled_quantity, Quantity::from_whole(10));
    }

    #[test]
    fn cancel_result_not_found() {
        let order_id = OrderId::new(MarketId::new(1), 999);
        let result = CancelResult::not_found(order_id);

        assert!(!result.is_cancelled());
        assert_eq!(result.outcome, CancelOutcome::NotFound);
        assert!(result.order.is_none());
    }
}
