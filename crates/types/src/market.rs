//! Market configuration types.
//!
//! This module defines [`MarketConfig`], which contains all the parameters
//! that define a trading market (pair), including:
//! - Token addresses (base and quote)
//! - Fee structure (maker rebate, taker fee)
//! - Size limits (min order size, tick/lot sizes)
//! - Market status (active, halted, etc.)
//!
//! # Fee Model
//!
//! The system uses a maker/taker fee model:
//! - **Maker**: Provides liquidity (resting orders), receives rebate
//! - **Taker**: Takes liquidity (crossing orders), pays fee
//!
//! Fees are stored in basis points (1 bp = 0.01% = 0.0001).
//! A negative fee indicates a rebate.
//!
//! Per the architecture decisions:
//! - Maker fee: -2 bp (-0.02% = rebate)
//! - Taker fee: +10 bp (+0.10%)

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Address, MarketId, Price, Quantity};

// =============================================================================
// Constants
// =============================================================================

/// One basis point (0.01% = 0.0001).
pub const BASIS_POINT: i32 = 1;

/// Default maker fee in basis points (-2 = -0.02% rebate).
pub const DEFAULT_MAKER_FEE_BPS: i32 = -2;

/// Default taker fee in basis points (10 = 0.10%).
pub const DEFAULT_TAKER_FEE_BPS: i32 = 10;

// =============================================================================
// MarketStatus
// =============================================================================

/// Current status of a market.
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
    Default,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum MarketStatus {
    /// Market is active and accepting orders.
    #[default]
    Active = 0,

    /// Market is temporarily halted (no new orders, no matching).
    Halted = 1,

    /// Market is in cancel-only mode (only cancellations allowed).
    CancelOnly = 2,

    /// Market is closed permanently.
    Closed = 3,
}

impl MarketStatus {
    /// Returns `true` if new orders can be placed.
    #[must_use]
    pub const fn accepts_new_orders(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns `true` if orders can be cancelled.
    #[must_use]
    pub const fn allows_cancellations(self) -> bool {
        matches!(self, Self::Active | Self::Halted | Self::CancelOnly)
    }

    /// Returns `true` if matching is enabled.
    #[must_use]
    pub const fn allows_matching(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns `true` if the market is closed.
    #[must_use]
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl fmt::Display for MarketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Halted => write!(f, "Halted"),
            Self::CancelOnly => write!(f, "CancelOnly"),
            Self::Closed => write!(f, "Closed"),
        }
    }
}

// =============================================================================
// FeeConfig
// =============================================================================

/// Fee configuration for a market.
///
/// Fees are stored in basis points. A negative value indicates a rebate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct FeeConfig {
    /// Maker fee in basis points (negative = rebate).
    pub maker_fee_bps: i32,

    /// Taker fee in basis points.
    pub taker_fee_bps: i32,
}

impl FeeConfig {
    /// Creates a new fee configuration.
    #[must_use]
    pub const fn new(maker_fee_bps: i32, taker_fee_bps: i32) -> Self {
        Self {
            maker_fee_bps,
            taker_fee_bps,
        }
    }

    /// Returns the default fee configuration.
    ///
    /// - Maker: -2 bps (-0.02% rebate)
    /// - Taker: +10 bps (+0.10%)
    #[must_use]
    pub const fn default_config() -> Self {
        Self::new(DEFAULT_MAKER_FEE_BPS, DEFAULT_TAKER_FEE_BPS)
    }

    /// Returns `true` if the maker receives a rebate.
    #[must_use]
    pub const fn maker_has_rebate(self) -> bool {
        self.maker_fee_bps < 0
    }

    /// Calculates the maker fee/rebate for a given notional value.
    ///
    /// Returns `(amount, is_rebate)` where `is_rebate` is true if the maker
    /// should receive the amount rather than pay it.
    #[must_use]
    pub fn calculate_maker_fee(self, notional: Quantity) -> (Quantity, bool) {
        let bps_abs = self.maker_fee_bps.unsigned_abs();
        let fee = Self::calculate_fee_amount(notional, bps_abs);
        (fee, self.maker_fee_bps < 0)
    }

    /// Calculates the taker fee for a given notional value.
    #[must_use]
    pub fn calculate_taker_fee(self, notional: Quantity) -> Quantity {
        let bps_abs = self.taker_fee_bps.unsigned_abs();
        Self::calculate_fee_amount(notional, bps_abs)
    }

    /// Helper to calculate fee from notional and basis points.
    fn calculate_fee_amount(notional: Quantity, bps: u32) -> Quantity {
        // fee = notional * bps / 10000
        // We need to be careful about overflow
        let scaled = notional.as_scaled();
        let fee_scaled = scaled.checked_mul(u128::from(bps)).map_or(0, |v| v / 10000);
        Quantity::from_scaled(fee_scaled)
    }
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

impl fmt::Display for FeeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "maker={}bps, taker={}bps",
            self.maker_fee_bps, self.taker_fee_bps
        )
    }
}

// =============================================================================
// MarketConfig
// =============================================================================

/// Complete configuration for a trading market.
///
/// A market is defined by a trading pair (base/quote tokens) and various
/// parameters controlling trading behavior.
///
/// # Example
///
/// ```
/// use types::{MarketId, MarketConfig, FeeConfig, MarketStatus, Price, Quantity, Address};
///
/// let config = MarketConfig::new(
///     MarketId::new(1),
///     "ETH/USDC".to_string(),
///     Address::new([1u8; 20]), // Base token (ETH)
///     Address::new([2u8; 20]), // Quote token (USDC)
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MarketConfig {
    /// Unique market identifier.
    pub id: MarketId,

    /// Human-readable market symbol (e.g., "ETH/USDC").
    pub symbol: String,

    /// Address of the base token contract.
    pub base_token: Address,

    /// Address of the quote token contract.
    pub quote_token: Address,

    /// Fee configuration.
    pub fees: FeeConfig,

    /// Current market status.
    pub status: MarketStatus,

    /// Minimum tick size for prices.
    ///
    /// All prices must be multiples of this value.
    pub tick_size: Price,

    /// Minimum lot size for quantities.
    ///
    /// All order quantities must be multiples of this value.
    pub lot_size: Quantity,

    /// Minimum order size in quote currency.
    pub min_order_value: Quantity,

    /// Maximum order size in base currency (0 = no limit).
    pub max_order_size: Quantity,

    /// Number of decimals for the base token.
    pub base_decimals: u8,

    /// Number of decimals for the quote token.
    pub quote_decimals: u8,
}

impl MarketConfig {
    /// Creates a new market configuration with default settings.
    #[must_use]
    pub fn new(id: MarketId, symbol: String, base_token: Address, quote_token: Address) -> Self {
        Self {
            id,
            symbol,
            base_token,
            quote_token,
            fees: FeeConfig::default(),
            status: MarketStatus::default(),
            tick_size: Price::from_scaled(1),   // Minimum tick
            lot_size: Quantity::from_scaled(1), // Minimum lot
            min_order_value: Quantity::ZERO,
            max_order_size: Quantity::ZERO, // No limit
            base_decimals: 18,
            quote_decimals: 18,
        }
    }

    /// Builder method to set fee configuration.
    #[must_use]
    pub fn with_fees(mut self, fees: FeeConfig) -> Self {
        self.fees = fees;
        self
    }

    /// Builder method to set market status.
    #[must_use]
    pub fn with_status(mut self, status: MarketStatus) -> Self {
        self.status = status;
        self
    }

    /// Builder method to set tick size.
    #[must_use]
    pub fn with_tick_size(mut self, tick_size: Price) -> Self {
        self.tick_size = tick_size;
        self
    }

    /// Builder method to set lot size.
    #[must_use]
    pub fn with_lot_size(mut self, lot_size: Quantity) -> Self {
        self.lot_size = lot_size;
        self
    }

    /// Builder method to set minimum order value.
    #[must_use]
    pub fn with_min_order_value(mut self, min_value: Quantity) -> Self {
        self.min_order_value = min_value;
        self
    }

    /// Builder method to set maximum order size.
    #[must_use]
    pub fn with_max_order_size(mut self, max_size: Quantity) -> Self {
        self.max_order_size = max_size;
        self
    }

    /// Builder method to set token decimals.
    #[must_use]
    pub fn with_decimals(mut self, base: u8, quote: u8) -> Self {
        self.base_decimals = base;
        self.quote_decimals = quote;
        self
    }

    /// Returns `true` if the market accepts new orders.
    #[must_use]
    pub const fn accepts_new_orders(&self) -> bool {
        self.status.accepts_new_orders()
    }

    /// Returns `true` if matching is enabled.
    #[must_use]
    pub const fn allows_matching(&self) -> bool {
        self.status.allows_matching()
    }

    /// Validates that a price conforms to the tick size.
    ///
    /// Returns `true` if the price is a valid multiple of the tick size.
    #[must_use]
    pub fn is_valid_price(&self, price: Price) -> bool {
        if self.tick_size.is_zero() {
            return true; // No tick constraint
        }
        price.as_scaled().is_multiple_of(self.tick_size.as_scaled())
    }

    /// Validates that a quantity conforms to the lot size.
    ///
    /// Returns `true` if the quantity is a valid multiple of the lot size.
    #[must_use]
    pub fn is_valid_quantity(&self, quantity: Quantity) -> bool {
        if self.lot_size.is_zero() {
            return true; // No lot constraint
        }
        quantity
            .as_scaled()
            .is_multiple_of(self.lot_size.as_scaled())
    }

    /// Rounds a price down to the nearest tick.
    #[must_use]
    pub fn round_price_down(&self, price: Price) -> Price {
        if self.tick_size.is_zero() {
            return price;
        }
        let tick = self.tick_size.as_scaled();
        let rounded = (price.as_scaled() / tick) * tick;
        Price::from_scaled(rounded)
    }

    /// Rounds a quantity down to the nearest lot.
    #[must_use]
    pub fn round_quantity_down(&self, quantity: Quantity) -> Quantity {
        if self.lot_size.is_zero() {
            return quantity;
        }
        let lot = self.lot_size.as_scaled();
        let rounded = (quantity.as_scaled() / lot) * lot;
        Quantity::from_scaled(rounded)
    }
}

impl fmt::Display for MarketConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Market({} [{}] {})", self.symbol, self.id, self.status)
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

    mod market_status_tests {
        use super::*;

        #[test]
        fn test_accepts_new_orders() {
            assert!(MarketStatus::Active.accepts_new_orders());
            assert!(!MarketStatus::Halted.accepts_new_orders());
            assert!(!MarketStatus::CancelOnly.accepts_new_orders());
            assert!(!MarketStatus::Closed.accepts_new_orders());
        }

        #[test]
        fn test_allows_cancellations() {
            assert!(MarketStatus::Active.allows_cancellations());
            assert!(MarketStatus::Halted.allows_cancellations());
            assert!(MarketStatus::CancelOnly.allows_cancellations());
            assert!(!MarketStatus::Closed.allows_cancellations());
        }

        #[test]
        fn test_allows_matching() {
            assert!(MarketStatus::Active.allows_matching());
            assert!(!MarketStatus::Halted.allows_matching());
            assert!(!MarketStatus::CancelOnly.allows_matching());
            assert!(!MarketStatus::Closed.allows_matching());
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", MarketStatus::Active), "Active");
            assert_eq!(format!("{}", MarketStatus::Halted), "Halted");
        }

        #[test]
        fn test_borsh_roundtrip() {
            for status in [
                MarketStatus::Active,
                MarketStatus::Halted,
                MarketStatus::CancelOnly,
                MarketStatus::Closed,
            ] {
                let bytes = borsh::to_vec(&status).unwrap();
                let decoded: MarketStatus = borsh::from_slice(&bytes).unwrap();
                assert_eq!(status, decoded);
            }
        }
    }

    mod fee_config_tests {
        use super::*;

        #[test]
        fn test_default() {
            let fees = FeeConfig::default();
            assert_eq!(fees.maker_fee_bps, -2);
            assert_eq!(fees.taker_fee_bps, 10);
            assert!(fees.maker_has_rebate());
        }

        #[test]
        fn test_calculate_taker_fee() {
            let fees = FeeConfig::default();
            let notional = Quantity::from_whole(10000); // $10,000

            let fee = fees.calculate_taker_fee(notional);
            // 10 bps = 0.1% = $10
            assert_eq!(fee, Quantity::from_whole(10));
        }

        #[test]
        fn test_calculate_maker_fee() {
            let fees = FeeConfig::default();
            let notional = Quantity::from_whole(10000); // $10,000

            let (fee, is_rebate) = fees.calculate_maker_fee(notional);
            // -2 bps = -0.02% = $2 rebate
            assert_eq!(fee, Quantity::from_whole(2));
            assert!(is_rebate);
        }

        #[test]
        fn test_calculate_maker_fee_positive() {
            let fees = FeeConfig::new(5, 10); // Both positive
            let notional = Quantity::from_whole(10000);

            let (fee, is_rebate) = fees.calculate_maker_fee(notional);
            // 5 bps = 0.05% = $5
            assert_eq!(fee, Quantity::from_whole(5));
            assert!(!is_rebate);
        }

        #[test]
        fn test_display() {
            let fees = FeeConfig::default();
            assert_eq!(format!("{fees}"), "maker=-2bps, taker=10bps");
        }
    }

    mod market_config_tests {
        use super::*;

        fn sample_market() -> MarketConfig {
            MarketConfig::new(
                MarketId::new(1),
                "ETH/USDC".to_string(),
                test_address(1),
                test_address(2),
            )
        }

        #[test]
        fn test_new() {
            let market = sample_market();
            assert_eq!(market.id, MarketId::new(1));
            assert_eq!(market.symbol, "ETH/USDC");
            assert_eq!(market.status, MarketStatus::Active);
            assert!(market.accepts_new_orders());
        }

        #[test]
        fn test_builders() {
            let market = sample_market()
                .with_fees(FeeConfig::new(-5, 15))
                .with_tick_size(Price::from_scaled(SCALE / 100)) // 0.01
                .with_lot_size(Quantity::from_scaled(SCALE / 1000)) // 0.001
                .with_min_order_value(Quantity::from_whole(10))
                .with_max_order_size(Quantity::from_whole(1000))
                .with_decimals(18, 6);

            assert_eq!(market.fees.maker_fee_bps, -5);
            assert_eq!(market.fees.taker_fee_bps, 15);
            assert_eq!(market.base_decimals, 18);
            assert_eq!(market.quote_decimals, 6);
        }

        #[test]
        fn test_is_valid_price() {
            let market = sample_market().with_tick_size(Price::from_whole(1)); // Tick = 1.0

            assert!(market.is_valid_price(Price::from_whole(100)));
            assert!(market.is_valid_price(Price::from_whole(1)));
            assert!(!market.is_valid_price(Price::from_scaled(SCALE + SCALE / 2)));
            // 1.5
        }

        #[test]
        fn test_is_valid_quantity() {
            let market = sample_market().with_lot_size(Quantity::from_scaled(SCALE / 10)); // Lot = 0.1

            assert!(market.is_valid_quantity(Quantity::from_whole(1)));
            assert!(market.is_valid_quantity(Quantity::from_scaled(SCALE / 10))); // 0.1
            assert!(!market.is_valid_quantity(Quantity::from_scaled(SCALE / 20)));
            // 0.05
        }

        #[test]
        fn test_round_price_down() {
            let market = sample_market().with_tick_size(Price::from_whole(10)); // Tick = 10

            let price = Price::from_whole(123);
            let rounded = market.round_price_down(price);
            assert_eq!(rounded, Price::from_whole(120));
        }

        #[test]
        fn test_round_quantity_down() {
            let market = sample_market().with_lot_size(Quantity::from_scaled(SCALE / 10)); // Lot = 0.1

            let qty = Quantity::from_scaled(SCALE / 4); // 0.25
            let rounded = market.round_quantity_down(qty);
            assert_eq!(rounded, Quantity::from_scaled(2 * SCALE / 10)); // 0.2
        }

        #[test]
        fn test_display() {
            let market = sample_market();
            let display = format!("{market}");
            assert!(display.contains("ETH/USDC"));
            assert!(display.contains("Active"));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let market = sample_market()
                .with_fees(FeeConfig::new(-3, 12))
                .with_tick_size(Price::from_scaled(SCALE / 100))
                .with_lot_size(Quantity::from_scaled(SCALE / 1000));

            let bytes = borsh::to_vec(&market).unwrap();
            let decoded: MarketConfig = borsh::from_slice(&bytes).unwrap();
            assert_eq!(market, decoded);
        }
    }
}
