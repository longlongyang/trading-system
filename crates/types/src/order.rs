//! Order types and related structures.
//!
//! This module defines the core order representation including:
//! - [`OrderSide`]: Buy or Sell direction
//! - [`OrderType`]: Limit or Market orders
//! - [`TimeInForce`]: Order execution policies (GTC, IOC, FOK, `PostOnly`)
//! - [`Order`]: The complete order struct
//! - [`OrderStatus`]: Current state of an order
//!
//! # Order Lifecycle
//!
//! Orders flow through the following states:
//! 1. `Open` - Active in the order book, may be partially filled
//! 2. `PartiallyFilled` - Some quantity executed, remainder still open
//! 3. `Filled` - Fully executed
//! 4. `Cancelled` - Cancelled by user or system
//! 5. `Expired` - Time-in-force expired (for IOC/FOK orders that weren't filled)

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Address, MarketId, Nonce, OrderId, Price, Quantity, Timestamp};

// =============================================================================
// OrderSide
// =============================================================================

/// The side of an order: Buy or Sell.
///
/// - `Buy`: Taking liquidity from asks, providing bids
/// - `Sell`: Taking liquidity from bids, providing asks
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum OrderSide {
    /// Buy order (bid). Attempts to match against asks.
    Buy = 0,
    /// Sell order (ask). Attempts to match against bids.
    Sell = 1,
}

impl OrderSide {
    /// Returns the opposite side.
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }

    /// Returns `true` if this is a buy order.
    #[must_use]
    pub const fn is_buy(self) -> bool {
        matches!(self, Self::Buy)
    }

    /// Returns `true` if this is a sell order.
    #[must_use]
    pub const fn is_sell(self) -> bool {
        matches!(self, Self::Sell)
    }
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buy => write!(f, "Buy"),
            Self::Sell => write!(f, "Sell"),
        }
    }
}

// =============================================================================
// OrderType
// =============================================================================

/// The type of order: Limit or Market.
///
/// - `Limit`: Specifies a maximum (for buys) or minimum (for sells) price
/// - `Market`: Executes immediately at best available price
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum OrderType {
    /// Limit order with a specified price.
    Limit = 0,
    /// Market order that executes at best available price.
    Market = 1,
}

impl OrderType {
    /// Returns `true` if this is a limit order.
    #[must_use]
    pub const fn is_limit(self) -> bool {
        matches!(self, Self::Limit)
    }

    /// Returns `true` if this is a market order.
    #[must_use]
    pub const fn is_market(self) -> bool {
        matches!(self, Self::Market)
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit => write!(f, "Limit"),
            Self::Market => write!(f, "Market"),
        }
    }
}

// =============================================================================
// TimeInForce
// =============================================================================

/// Time-in-force policy controlling order execution and cancellation.
///
/// - `GoodTilCancelled`: Remains active until explicitly cancelled
/// - `ImmediateOrCancel`: Execute immediately (partial ok), cancel remainder
/// - `FillOrKill`: Execute entirely or cancel completely
/// - `PostOnly`: Only add liquidity (maker), reject if would cross
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum TimeInForce {
    /// Good-til-cancelled. Order remains open until filled or cancelled.
    #[default]
    GoodTilCancelled = 0,

    /// Immediate-or-cancel. Execute immediately, cancel any unfilled portion.
    ImmediateOrCancel = 1,

    /// Fill-or-kill. Must execute entirely in one transaction or be cancelled.
    FillOrKill = 2,

    /// Post-only (maker-only). Rejected if it would cross the spread.
    PostOnly = 3,
}

impl TimeInForce {
    /// Returns `true` if this is a GTC order.
    #[must_use]
    pub const fn is_gtc(self) -> bool {
        matches!(self, Self::GoodTilCancelled)
    }

    /// Returns `true` if this order can rest in the book.
    ///
    /// GTC and `PostOnly` orders can rest; IOC and FOK cannot.
    #[must_use]
    pub const fn can_rest(self) -> bool {
        matches!(self, Self::GoodTilCancelled | Self::PostOnly)
    }

    /// Returns `true` if partial fills are acceptable.
    ///
    /// FOK requires full fill; all others accept partial.
    #[must_use]
    pub const fn allows_partial_fill(self) -> bool {
        !matches!(self, Self::FillOrKill)
    }

    /// Returns `true` if this is a maker-only order.
    #[must_use]
    pub const fn is_post_only(self) -> bool {
        matches!(self, Self::PostOnly)
    }
}

impl fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GoodTilCancelled => write!(f, "GTC"),
            Self::ImmediateOrCancel => write!(f, "IOC"),
            Self::FillOrKill => write!(f, "FOK"),
            Self::PostOnly => write!(f, "PostOnly"),
        }
    }
}

// =============================================================================
// OrderStatus
// =============================================================================

/// Current status of an order.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum OrderStatus {
    /// Order is open and active in the order book.
    Open = 0,

    /// Order has been partially filled, remainder is still open.
    PartiallyFilled = 1,

    /// Order has been completely filled.
    Filled = 2,

    /// Order was cancelled (by user, system, or self-trade prevention).
    Cancelled = 3,

    /// Order expired without being filled (IOC/FOK that couldn't execute).
    Expired = 4,

    /// Order was rejected (validation failure, insufficient funds, etc.).
    Rejected = 5,
}

impl OrderStatus {
    /// Returns `true` if the order is still active (can be matched or cancelled).
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Open | Self::PartiallyFilled)
    }

    /// Returns `true` if the order is in a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::Cancelled | Self::Expired | Self::Rejected
        )
    }

    /// Returns `true` if the order was successfully executed (partially or fully).
    #[must_use]
    pub const fn has_fills(self) -> bool {
        matches!(self, Self::PartiallyFilled | Self::Filled)
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::PartiallyFilled => write!(f, "PartiallyFilled"),
            Self::Filled => write!(f, "Filled"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Expired => write!(f, "Expired"),
            Self::Rejected => write!(f, "Rejected"),
        }
    }
}

// =============================================================================
// Order
// =============================================================================

/// A complete order in the trading system.
///
/// Orders are immutable once created. State changes (fills, cancellation)
/// are tracked separately and the order's `remaining_quantity` is updated.
///
/// # Fields
///
/// - `id`: Unique identifier encoding market and sequence number
/// - `market_id`: The market this order belongs to
/// - `owner`: The address that submitted the order
/// - `side`: Buy or Sell
/// - `order_type`: Limit or Market
/// - `time_in_force`: Execution policy
/// - `price`: Limit price (ignored for market orders)
/// - `quantity`: Original order size
/// - `remaining_quantity`: Unfilled portion
/// - `status`: Current order state
/// - `nonce`: User nonce for replay protection
/// - `created_at`: Timestamp when order was created
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Order {
    /// Unique order identifier (encodes `market_id` and sequence).
    pub id: OrderId,

    /// Market this order is placed in.
    pub market_id: MarketId,

    /// Address of the order owner.
    pub owner: Address,

    /// Order side (Buy/Sell).
    pub side: OrderSide,

    /// Order type (Limit/Market).
    pub order_type: OrderType,

    /// Time-in-force policy.
    pub time_in_force: TimeInForce,

    /// Limit price. For market orders, this is `Price::ZERO` (ignored).
    pub price: Price,

    /// Original order quantity.
    pub quantity: Quantity,

    /// Remaining unfilled quantity.
    pub remaining_quantity: Quantity,

    /// Current order status.
    pub status: OrderStatus,

    /// User nonce for replay protection.
    pub nonce: Nonce,

    /// Timestamp when the order was created.
    pub created_at: Timestamp,
}

impl Order {
    /// Creates a new limit order.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new_limit(
        id: OrderId,
        market_id: MarketId,
        owner: Address,
        side: OrderSide,
        time_in_force: TimeInForce,
        price: Price,
        quantity: Quantity,
        nonce: Nonce,
        created_at: Timestamp,
    ) -> Self {
        Self {
            id,
            market_id,
            owner,
            side,
            order_type: OrderType::Limit,
            time_in_force,
            price,
            quantity,
            remaining_quantity: quantity,
            status: OrderStatus::Open,
            nonce,
            created_at,
        }
    }

    /// Creates a new market order.
    #[must_use]
    pub const fn new_market(
        id: OrderId,
        market_id: MarketId,
        owner: Address,
        side: OrderSide,
        quantity: Quantity,
        nonce: Nonce,
        created_at: Timestamp,
    ) -> Self {
        Self {
            id,
            market_id,
            owner,
            side,
            order_type: OrderType::Market,
            time_in_force: TimeInForce::ImmediateOrCancel, // Market orders are always IOC
            price: Price::ZERO,
            quantity,
            remaining_quantity: quantity,
            status: OrderStatus::Open,
            nonce,
            created_at,
        }
    }

    /// Returns `true` if the order is a buy order.
    #[must_use]
    pub const fn is_buy(&self) -> bool {
        self.side.is_buy()
    }

    /// Returns `true` if the order is a sell order.
    #[must_use]
    pub const fn is_sell(&self) -> bool {
        self.side.is_sell()
    }

    /// Returns `true` if the order is a limit order.
    #[must_use]
    pub const fn is_limit(&self) -> bool {
        self.order_type.is_limit()
    }

    /// Returns `true` if the order is a market order.
    #[must_use]
    pub const fn is_market(&self) -> bool {
        self.order_type.is_market()
    }

    /// Returns `true` if the order is still active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Returns `true` if the order has been fully filled.
    #[must_use]
    pub const fn is_filled(&self) -> bool {
        matches!(self.status, OrderStatus::Filled)
    }

    /// Returns the filled quantity.
    #[must_use]
    pub fn filled_quantity(&self) -> Quantity {
        self.quantity
            .checked_sub(self.remaining_quantity)
            .unwrap_or(Quantity::ZERO)
    }

    /// Returns the fill ratio as a percentage (0-100).
    ///
    /// Returns 0 if the original quantity is zero.
    #[must_use]
    pub fn fill_percentage(&self) -> u8 {
        if self.quantity.is_zero() {
            return 0;
        }
        let filled = self.filled_quantity().as_scaled();
        let total = self.quantity.as_scaled();
        // Safe because filled <= total, so ratio is in [0, 100]
        #[allow(clippy::cast_possible_truncation)]
        let percentage = ((filled * 100) / total) as u8;
        percentage
    }

    /// Applies a fill to this order, reducing remaining quantity.
    ///
    /// Updates the status to `PartiallyFilled` or `Filled` as appropriate.
    ///
    /// # Panics
    ///
    /// Panics if `fill_quantity` exceeds `remaining_quantity`.
    pub fn apply_fill(&mut self, fill_quantity: Quantity) {
        self.remaining_quantity = self
            .remaining_quantity
            .checked_sub(fill_quantity)
            .expect("fill quantity exceeds remaining");

        self.status = if self.remaining_quantity.is_zero() {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
    }

    /// Cancels this order.
    ///
    /// # Panics
    ///
    /// Panics if the order is already in a terminal state.
    pub fn cancel(&mut self) {
        assert!(
            !self.status.is_terminal(),
            "cannot cancel order in terminal state"
        );
        self.status = OrderStatus::Cancelled;
    }

    /// Marks this order as expired.
    pub fn expire(&mut self) {
        if !self.status.is_terminal() {
            self.status = OrderStatus::Expired;
        }
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Order({} {} {} {} @ {} [{}/{}] {})",
            self.id,
            self.side,
            self.order_type,
            self.time_in_force,
            self.price,
            self.filled_quantity(),
            self.quantity,
            self.status,
        )
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SCALE;

    fn test_address() -> Address {
        Address::new([1u8; 20])
    }

    fn test_order_id(seq: u64) -> OrderId {
        OrderId::new(MarketId::new(1), seq)
    }

    mod order_side_tests {
        use super::*;

        #[test]
        fn test_opposite() {
            assert_eq!(OrderSide::Buy.opposite(), OrderSide::Sell);
            assert_eq!(OrderSide::Sell.opposite(), OrderSide::Buy);
        }

        #[test]
        fn test_is_buy_sell() {
            assert!(OrderSide::Buy.is_buy());
            assert!(!OrderSide::Buy.is_sell());
            assert!(OrderSide::Sell.is_sell());
            assert!(!OrderSide::Sell.is_buy());
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", OrderSide::Buy), "Buy");
            assert_eq!(format!("{}", OrderSide::Sell), "Sell");
        }

        #[test]
        fn test_borsh_roundtrip() {
            for side in [OrderSide::Buy, OrderSide::Sell] {
                let bytes = borsh::to_vec(&side).unwrap();
                let decoded: OrderSide = borsh::from_slice(&bytes).unwrap();
                assert_eq!(side, decoded);
            }
        }
    }

    mod order_type_tests {
        use super::*;

        #[test]
        fn test_is_limit_market() {
            assert!(OrderType::Limit.is_limit());
            assert!(!OrderType::Limit.is_market());
            assert!(OrderType::Market.is_market());
            assert!(!OrderType::Market.is_limit());
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", OrderType::Limit), "Limit");
            assert_eq!(format!("{}", OrderType::Market), "Market");
        }
    }

    mod time_in_force_tests {
        use super::*;

        #[test]
        fn test_default() {
            assert_eq!(TimeInForce::default(), TimeInForce::GoodTilCancelled);
        }

        #[test]
        fn test_can_rest() {
            assert!(TimeInForce::GoodTilCancelled.can_rest());
            assert!(TimeInForce::PostOnly.can_rest());
            assert!(!TimeInForce::ImmediateOrCancel.can_rest());
            assert!(!TimeInForce::FillOrKill.can_rest());
        }

        #[test]
        fn test_allows_partial_fill() {
            assert!(TimeInForce::GoodTilCancelled.allows_partial_fill());
            assert!(TimeInForce::ImmediateOrCancel.allows_partial_fill());
            assert!(TimeInForce::PostOnly.allows_partial_fill());
            assert!(!TimeInForce::FillOrKill.allows_partial_fill());
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", TimeInForce::GoodTilCancelled), "GTC");
            assert_eq!(format!("{}", TimeInForce::ImmediateOrCancel), "IOC");
            assert_eq!(format!("{}", TimeInForce::FillOrKill), "FOK");
            assert_eq!(format!("{}", TimeInForce::PostOnly), "PostOnly");
        }
    }

    mod order_status_tests {
        use super::*;

        #[test]
        fn test_is_active() {
            assert!(OrderStatus::Open.is_active());
            assert!(OrderStatus::PartiallyFilled.is_active());
            assert!(!OrderStatus::Filled.is_active());
            assert!(!OrderStatus::Cancelled.is_active());
            assert!(!OrderStatus::Expired.is_active());
            assert!(!OrderStatus::Rejected.is_active());
        }

        #[test]
        fn test_is_terminal() {
            assert!(!OrderStatus::Open.is_terminal());
            assert!(!OrderStatus::PartiallyFilled.is_terminal());
            assert!(OrderStatus::Filled.is_terminal());
            assert!(OrderStatus::Cancelled.is_terminal());
            assert!(OrderStatus::Expired.is_terminal());
            assert!(OrderStatus::Rejected.is_terminal());
        }

        #[test]
        fn test_has_fills() {
            assert!(!OrderStatus::Open.has_fills());
            assert!(OrderStatus::PartiallyFilled.has_fills());
            assert!(OrderStatus::Filled.has_fills());
            assert!(!OrderStatus::Cancelled.has_fills());
        }
    }

    mod order_tests {
        use super::*;

        #[test]
        fn test_new_limit_order() {
            let order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            assert!(order.is_buy());
            assert!(order.is_limit());
            assert!(order.is_active());
            assert_eq!(order.status, OrderStatus::Open);
            assert_eq!(order.quantity, order.remaining_quantity);
            assert_eq!(order.filled_quantity(), Quantity::ZERO);
            assert_eq!(order.fill_percentage(), 0);
        }

        #[test]
        fn test_new_market_order() {
            let order = Order::new_market(
                test_order_id(2),
                MarketId::new(1),
                test_address(),
                OrderSide::Sell,
                Quantity::from_whole(5),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            assert!(order.is_sell());
            assert!(order.is_market());
            assert_eq!(order.price, Price::ZERO);
            assert_eq!(order.time_in_force, TimeInForce::ImmediateOrCancel);
        }

        #[test]
        fn test_apply_fill_partial() {
            let mut order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            order.apply_fill(Quantity::from_whole(3));

            assert_eq!(order.status, OrderStatus::PartiallyFilled);
            assert_eq!(order.remaining_quantity, Quantity::from_whole(7));
            assert_eq!(order.filled_quantity(), Quantity::from_whole(3));
            assert_eq!(order.fill_percentage(), 30);
        }

        #[test]
        fn test_apply_fill_complete() {
            let mut order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            order.apply_fill(Quantity::from_whole(10));

            assert_eq!(order.status, OrderStatus::Filled);
            assert!(order.remaining_quantity.is_zero());
            assert_eq!(order.fill_percentage(), 100);
            assert!(order.is_filled());
        }

        #[test]
        fn test_cancel() {
            let mut order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            order.cancel();
            assert_eq!(order.status, OrderStatus::Cancelled);
        }

        #[test]
        #[should_panic(expected = "cannot cancel order in terminal state")]
        fn test_cancel_already_filled() {
            let mut order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            order.apply_fill(Quantity::from_whole(10));
            order.cancel(); // Should panic
        }

        #[test]
        fn test_expire() {
            let mut order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::ImmediateOrCancel,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            order.expire();
            assert_eq!(order.status, OrderStatus::Expired);
        }

        #[test]
        fn test_display() {
            let order = Order::new_limit(
                test_order_id(1),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(100),
                Quantity::from_whole(10),
                Nonce::new(0),
                Timestamp::from_millis(1000),
            );

            let display = format!("{order}");
            assert!(display.contains("Buy"));
            assert!(display.contains("Limit"));
            assert!(display.contains("GTC"));
            assert!(display.contains("Open"));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let order = Order::new_limit(
                test_order_id(42),
                MarketId::new(1),
                test_address(),
                OrderSide::Buy,
                TimeInForce::PostOnly,
                Price::from_scaled(123 * SCALE),
                Quantity::from_scaled(456 * SCALE),
                Nonce::new(99),
                Timestamp::from_millis(1_700_000_000_000),
            );

            let bytes = borsh::to_vec(&order).unwrap();
            let decoded: Order = borsh::from_slice(&bytes).unwrap();
            assert_eq!(order, decoded);
        }
    }
}
