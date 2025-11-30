//! Trade and fill types.
//!
//! This module defines the structures representing executed trades:
//! - [`Fill`]: A single fill event when an order matches
//! - [`Trade`]: A complete trade record with both sides
//!
//! # Trade vs Fill
//!
//! A [`Fill`] represents one side of a trade from a single order's perspective.
//! A [`Trade`] represents the complete match between two orders.
//!
//! When a taker order matches against multiple resting orders, it produces
//! one fill per match (taker perspective) but multiple fills from the makers'
//! perspectives.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Address, MarketId, OrderId, OrderSide, Price, Quantity, Timestamp};

// =============================================================================
// TradeId
// =============================================================================

/// Unique identifier for a trade.
///
/// Trade IDs are assigned sequentially per market, similar to order IDs.
/// The upper 64 bits encode the market ID, the lower 64 bits encode the
/// sequence number within that market.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct TradeId(u128);

impl TradeId {
    /// Creates a new `TradeId` from market ID and sequence number.
    #[must_use]
    pub const fn new(market_id: MarketId, sequence: u64) -> Self {
        let market_bits = (market_id.as_u64() as u128) << 64;
        Self(market_bits | (sequence as u128))
    }

    /// Returns the market ID component.
    #[must_use]
    pub const fn market_id(self) -> MarketId {
        MarketId::new((self.0 >> 64) as u64)
    }

    /// Returns the sequence number component.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.0 as u64
    }

    /// Returns the raw 128-bit value.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for TradeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}-{}", self.market_id().as_u64(), self.sequence())
    }
}

// =============================================================================
// FillRole
// =============================================================================

/// The role of a party in a fill: maker or taker.
///
/// - `Maker`: The order was resting in the book and provided liquidity
/// - `Taker`: The order crossed the spread and took liquidity
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
pub enum FillRole {
    /// Maker: provided liquidity (order was resting in book).
    Maker = 0,
    /// Taker: took liquidity (order crossed the spread).
    Taker = 1,
}

impl FillRole {
    /// Returns `true` if this is the maker role.
    #[must_use]
    pub const fn is_maker(self) -> bool {
        matches!(self, Self::Maker)
    }

    /// Returns `true` if this is the taker role.
    #[must_use]
    pub const fn is_taker(self) -> bool {
        matches!(self, Self::Taker)
    }
}

impl fmt::Display for FillRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Maker => write!(f, "Maker"),
            Self::Taker => write!(f, "Taker"),
        }
    }
}

// =============================================================================
// Fill
// =============================================================================

/// A fill event from a single order's perspective.
///
/// When an order matches against another, a fill is generated for each party.
/// Fills track the quantity executed, price, fees, and the role (maker/taker).
///
/// # Fee Calculation
///
/// Fees are calculated based on the role and market configuration:
/// - Makers typically receive a rebate (negative fee)
/// - Takers pay a fee
/// Fees are always in the quote currency.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Fill {
    /// The trade this fill is part of.
    pub trade_id: TradeId,

    /// The order that received this fill.
    pub order_id: OrderId,

    /// The owner of the order.
    pub owner: Address,

    /// The counterparty order.
    pub counterparty_order_id: OrderId,

    /// Whether this fill was as maker or taker.
    pub role: FillRole,

    /// The side of the order (Buy/Sell).
    pub side: OrderSide,

    /// Execution price.
    pub price: Price,

    /// Quantity filled.
    pub quantity: Quantity,

    /// Fee charged (positive) or rebate received (use signed representation).
    ///
    /// Stored as Quantity but should be interpreted with sign from `is_rebate`.
    pub fee: Quantity,

    /// If `true`, the fee is actually a rebate (maker receives funds).
    pub is_rebate: bool,

    /// Timestamp when the fill occurred.
    pub timestamp: Timestamp,
}

impl Fill {
    /// Creates a new fill.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        trade_id: TradeId,
        order_id: OrderId,
        owner: Address,
        counterparty_order_id: OrderId,
        role: FillRole,
        side: OrderSide,
        price: Price,
        quantity: Quantity,
        fee: Quantity,
        is_rebate: bool,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            trade_id,
            order_id,
            owner,
            counterparty_order_id,
            role,
            side,
            price,
            quantity,
            fee,
            is_rebate,
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

    /// Returns `true` if this fill is from the maker's perspective.
    #[must_use]
    pub const fn is_maker(&self) -> bool {
        self.role.is_maker()
    }

    /// Returns `true` if this fill is from the taker's perspective.
    #[must_use]
    pub const fn is_taker(&self) -> bool {
        self.role.is_taker()
    }
}

impl fmt::Display for Fill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fee_str = if self.is_rebate {
            format!("-{}", self.fee) // Rebate shown as negative
        } else {
            format!("+{}", self.fee)
        };
        write!(
            f,
            "Fill({} {} {} {} @ {} fee={})",
            self.trade_id, self.role, self.side, self.quantity, self.price, fee_str
        )
    }
}

// =============================================================================
// Trade
// =============================================================================

/// A complete trade record representing a match between two orders.
///
/// Trades are the authoritative record of execution and are used for:
/// - Settlement on-chain
/// - Trade history and reporting
/// - Market data (price, volume)
///
/// Each trade has exactly one maker and one taker.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct Trade {
    /// Unique trade identifier.
    pub id: TradeId,

    /// Market where the trade occurred.
    pub market_id: MarketId,

    /// The maker order (was resting in book).
    pub maker_order_id: OrderId,

    /// The taker order (crossed the spread).
    pub taker_order_id: OrderId,

    /// Address of the maker.
    pub maker: Address,

    /// Address of the taker.
    pub taker: Address,

    /// Execution price (always the maker's price).
    pub price: Price,

    /// Quantity traded.
    pub quantity: Quantity,

    /// Side of the taker order.
    ///
    /// If `TakerSide::Buy`, the taker bought from the maker (maker was selling).
    pub taker_side: OrderSide,

    /// Fee paid by or rebate received by the maker.
    pub maker_fee: Quantity,

    /// Whether maker_fee is a rebate (true) or charge (false).
    pub maker_is_rebate: bool,

    /// Fee paid by the taker.
    pub taker_fee: Quantity,

    /// Timestamp when the trade was executed.
    pub timestamp: Timestamp,
}

impl Trade {
    /// Creates a new trade record.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: TradeId,
        market_id: MarketId,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker: Address,
        taker: Address,
        price: Price,
        quantity: Quantity,
        taker_side: OrderSide,
        maker_fee: Quantity,
        maker_is_rebate: bool,
        taker_fee: Quantity,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            id,
            market_id,
            maker_order_id,
            taker_order_id,
            maker,
            taker,
            price,
            quantity,
            taker_side,
            maker_fee,
            maker_is_rebate,
            taker_fee,
            timestamp,
        }
    }

    /// Returns the notional value of this trade.
    #[must_use]
    pub fn notional(&self) -> Option<Quantity> {
        self.quantity.checked_mul_price(self.price)
    }

    /// Returns the maker's side (opposite of taker's side).
    #[must_use]
    pub const fn maker_side(&self) -> OrderSide {
        self.taker_side.opposite()
    }

    /// Creates fill records for both maker and taker.
    #[must_use]
    pub fn to_fills(&self) -> (Fill, Fill) {
        let maker_fill = Fill::new(
            self.id,
            self.maker_order_id,
            self.maker,
            self.taker_order_id,
            FillRole::Maker,
            self.maker_side(),
            self.price,
            self.quantity,
            self.maker_fee,
            self.maker_is_rebate,
            self.timestamp,
        );

        let taker_fill = Fill::new(
            self.id,
            self.taker_order_id,
            self.taker,
            self.maker_order_id,
            FillRole::Taker,
            self.taker_side,
            self.price,
            self.quantity,
            self.taker_fee,
            false, // Taker always pays fee
            self.timestamp,
        );

        (maker_fill, taker_fill)
    }
}

impl fmt::Display for Trade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Trade({} {} {} @ {} maker={} taker={})",
            self.id,
            self.taker_side,
            self.quantity,
            self.price,
            self.maker_order_id,
            self.taker_order_id,
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

    fn test_address(seed: u8) -> Address {
        Address::new([seed; 20])
    }

    fn test_market_id() -> MarketId {
        MarketId::new(1)
    }

    fn test_order_id(seq: u64) -> OrderId {
        OrderId::new(test_market_id(), seq)
    }

    fn test_trade_id(seq: u64) -> TradeId {
        TradeId::new(test_market_id(), seq)
    }

    mod trade_id_tests {
        use super::*;

        #[test]
        fn test_new_and_components() {
            let market = MarketId::new(42);
            let id = TradeId::new(market, 12345);

            assert_eq!(id.market_id(), market);
            assert_eq!(id.sequence(), 12345);
        }

        #[test]
        fn test_display() {
            let id = TradeId::new(MarketId::new(1), 100);
            assert_eq!(format!("{id}"), "T1-100");
        }

        #[test]
        fn test_ordering() {
            let t1 = TradeId::new(MarketId::new(1), 1);
            let t2 = TradeId::new(MarketId::new(1), 2);
            assert!(t1 < t2);
        }

        #[test]
        fn test_borsh_roundtrip() {
            let id = TradeId::new(MarketId::new(99), 888);
            let bytes = borsh::to_vec(&id).unwrap();
            let decoded: TradeId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(id, decoded);
        }
    }

    mod fill_role_tests {
        use super::*;

        #[test]
        fn test_is_maker_taker() {
            assert!(FillRole::Maker.is_maker());
            assert!(!FillRole::Maker.is_taker());
            assert!(FillRole::Taker.is_taker());
            assert!(!FillRole::Taker.is_maker());
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", FillRole::Maker), "Maker");
            assert_eq!(format!("{}", FillRole::Taker), "Taker");
        }
    }

    mod fill_tests {
        use super::*;

        #[test]
        fn test_new_fill() {
            let fill = Fill::new(
                test_trade_id(1),
                test_order_id(10),
                test_address(1),
                test_order_id(20),
                FillRole::Taker,
                OrderSide::Buy,
                Price::from_whole(100),
                Quantity::from_whole(5),
                Quantity::from_scaled(SCALE / 100), // 0.01 fee
                false,
                Timestamp::from_millis(1000),
            );

            assert!(fill.is_taker());
            assert!(!fill.is_maker());
            assert_eq!(fill.side, OrderSide::Buy);
        }

        #[test]
        fn test_notional() {
            // Use values that won't overflow when multiplied
            // Price = SCALE (representing 1.0)
            // Quantity = 5 * SCALE (representing 5.0)
            // Notional = (5 * SCALE * SCALE) / SCALE = 5 * SCALE
            let fill = Fill::new(
                test_trade_id(1),
                test_order_id(10),
                test_address(1),
                test_order_id(20),
                FillRole::Taker,
                OrderSide::Buy,
                Price::from_scaled(SCALE),        // Price = 1.0
                Quantity::from_scaled(5 * SCALE), // Quantity = 5.0
                Quantity::ZERO,
                false,
                Timestamp::from_millis(1000),
            );

            let notional = fill.notional().unwrap();
            assert_eq!(notional, Quantity::from_scaled(5 * SCALE)); // 1.0 * 5.0 = 5.0
        }

        #[test]
        fn test_display_with_fee() {
            let fill = Fill::new(
                test_trade_id(1),
                test_order_id(10),
                test_address(1),
                test_order_id(20),
                FillRole::Taker,
                OrderSide::Buy,
                Price::from_whole(100),
                Quantity::from_whole(5),
                Quantity::from_scaled(SCALE / 10), // 0.1 fee
                false,
                Timestamp::from_millis(1000),
            );

            let display = format!("{fill}");
            assert!(display.contains("Taker"));
            assert!(display.contains("Buy"));
        }

        #[test]
        fn test_display_with_rebate() {
            let fill = Fill::new(
                test_trade_id(1),
                test_order_id(10),
                test_address(1),
                test_order_id(20),
                FillRole::Maker,
                OrderSide::Sell,
                Price::from_whole(100),
                Quantity::from_whole(5),
                Quantity::from_scaled(SCALE / 100), // 0.01 rebate
                true,
                Timestamp::from_millis(1000),
            );

            let display = format!("{fill}");
            assert!(display.contains("Maker"));
            assert!(display.contains("-")); // Rebate shown as negative
        }

        #[test]
        fn test_borsh_roundtrip() {
            let fill = Fill::new(
                test_trade_id(42),
                test_order_id(10),
                test_address(5),
                test_order_id(20),
                FillRole::Maker,
                OrderSide::Sell,
                Price::from_scaled(12345 * SCALE),
                Quantity::from_scaled(67890 * SCALE),
                Quantity::from_scaled(123),
                true,
                Timestamp::from_millis(1700000000000),
            );

            let bytes = borsh::to_vec(&fill).unwrap();
            let decoded: Fill = borsh::from_slice(&bytes).unwrap();
            assert_eq!(fill, decoded);
        }
    }

    mod trade_tests {
        use super::*;

        fn sample_trade() -> Trade {
            Trade::new(
                test_trade_id(1),
                test_market_id(),
                test_order_id(100),                 // maker
                test_order_id(200),                 // taker
                test_address(1),                    // maker address
                test_address(2),                    // taker address
                Price::from_scaled(2 * SCALE),      // Price = 2.0
                Quantity::from_scaled(10 * SCALE),  // Quantity = 10.0
                OrderSide::Buy,                     // taker is buying
                Quantity::from_scaled(SCALE / 100), // 0.01 maker rebate
                true,                               // is rebate
                Quantity::from_scaled(SCALE / 10),  // 0.1 taker fee
                Timestamp::from_millis(1700000000000),
            )
        }

        #[test]
        fn test_maker_side() {
            let trade = sample_trade();
            assert_eq!(trade.taker_side, OrderSide::Buy);
            assert_eq!(trade.maker_side(), OrderSide::Sell);
        }

        #[test]
        fn test_notional() {
            let trade = sample_trade();
            let notional = trade.notional().unwrap();
            assert_eq!(notional, Quantity::from_scaled(20 * SCALE)); // 2.0 * 10.0 = 20.0
        }

        #[test]
        fn test_to_fills() {
            let trade = sample_trade();
            let (maker_fill, taker_fill) = trade.to_fills();

            // Verify maker fill
            assert_eq!(maker_fill.order_id, trade.maker_order_id);
            assert_eq!(maker_fill.owner, trade.maker);
            assert_eq!(maker_fill.role, FillRole::Maker);
            assert_eq!(maker_fill.side, OrderSide::Sell); // Opposite of taker
            assert!(maker_fill.is_rebate);

            // Verify taker fill
            assert_eq!(taker_fill.order_id, trade.taker_order_id);
            assert_eq!(taker_fill.owner, trade.taker);
            assert_eq!(taker_fill.role, FillRole::Taker);
            assert_eq!(taker_fill.side, OrderSide::Buy);
            assert!(!taker_fill.is_rebate);

            // Both should reference the same trade
            assert_eq!(maker_fill.trade_id, taker_fill.trade_id);
            assert_eq!(maker_fill.price, taker_fill.price);
            assert_eq!(maker_fill.quantity, taker_fill.quantity);
        }

        #[test]
        fn test_display() {
            let trade = sample_trade();
            let display = format!("{trade}");
            assert!(display.contains("Trade("));
            assert!(display.contains("Buy"));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let trade = sample_trade();
            let bytes = borsh::to_vec(&trade).unwrap();
            let decoded: Trade = borsh::from_slice(&bytes).unwrap();
            assert_eq!(trade, decoded);
        }
    }
}
