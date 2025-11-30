//! Numeric primitive types for the trading system.
//!
//! This module defines fixed-point numeric types used for prices and quantities:
//! - [`Price`]: A fixed-point price with 18 decimal places
//! - [`Quantity`]: A fixed-point quantity with 18 decimal places
//! - [`Timestamp`]: Unix timestamp in milliseconds
//! - [`Nonce`]: Monotonically increasing counter for replay protection
//!
//! # Fixed-Point Representation
//!
//! Both [`Price`] and [`Quantity`] use 18 decimal places to match ERC-20 token
//! standards. This means:
//!
//! - `1.0` is represented as `1_000_000_000_000_000_000` (10^18)
//! - `0.5` is represented as `500_000_000_000_000_000`
//! - Minimum non-zero value is `1` (representing 10^-18)
//!
//! All arithmetic operations use checked math to prevent overflow and return
//! `Option<T>` or `Result<T, E>` to make error handling explicit.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Number of decimal places for fixed-point representation.
///
/// We use 18 decimals to match the standard ERC-20 token decimals.
pub const DECIMALS: u32 = 18;

/// The scaling factor for fixed-point conversion (10^18).
pub const SCALE: u128 = 1_000_000_000_000_000_000;

// =============================================================================
// Price
// =============================================================================

/// A fixed-point price with 18 decimal places.
///
/// Prices are stored as `u128` integers where the value represents the price
/// multiplied by 10^18. This ensures deterministic arithmetic across all
/// platforms.
///
/// # Example
///
/// ```
/// use types::Price;
///
/// // Create a price of 1.5
/// let price = Price::from_scaled(1_500_000_000_000_000_000);
///
/// // Or from a whole number
/// let price2 = Price::from_whole(100); // 100.0
///
/// // Arithmetic
/// let doubled = price.checked_mul_scalar(2).unwrap();
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Price(u128);

impl Price {
    /// The zero price.
    pub const ZERO: Self = Self(0);

    /// The maximum representable price.
    pub const MAX: Self = Self(u128::MAX);

    /// Creates a `Price` from an already-scaled value.
    ///
    /// The value should already be multiplied by 10^18.
    #[must_use]
    pub const fn from_scaled(scaled: u128) -> Self {
        Self(scaled)
    }

    /// Creates a `Price` from a whole number (no decimals).
    ///
    /// # Panics
    ///
    /// Panics if the value would overflow when scaled. Use [`Price::try_from_whole`]
    /// for a non-panicking version.
    #[must_use]
    pub const fn from_whole(whole: u64) -> Self {
        // This is safe because u64::MAX * SCALE fits in u128
        Self((whole as u128) * SCALE)
    }

    /// Tries to create a `Price` from a whole number.
    ///
    /// Returns `None` if the scaling would overflow.
    #[must_use]
    pub const fn try_from_whole(whole: u128) -> Option<Self> {
        match whole.checked_mul(SCALE) {
            Some(scaled) => Some(Self(scaled)),
            None => None,
        }
    }

    /// Returns the raw scaled value.
    #[must_use]
    pub const fn as_scaled(self) -> u128 {
        self.0
    }

    /// Returns `true` if the price is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Checked addition. Returns `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked subtraction. Returns `None` on underflow.
    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked multiplication by a scalar (integer).
    ///
    /// Multiplies the price by a whole number without changing the decimal places.
    #[must_use]
    pub const fn checked_mul_scalar(self, scalar: u64) -> Option<Self> {
        match self.0.checked_mul(scalar as u128) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked division by a scalar (integer).
    ///
    /// Divides the price by a whole number. Returns `None` if divisor is zero.
    #[must_use]
    pub const fn checked_div_scalar(self, divisor: u64) -> Option<Self> {
        if divisor == 0 {
            return None;
        }
        Some(Self(self.0 / divisor as u128))
    }

    /// Saturating addition. Clamps to `Price::MAX` on overflow.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction. Clamps to zero on underflow.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Returns the minimum of two prices.
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        if self.0 <= other.0 {
            self
        } else {
            other
        }
    }

    /// Returns the maximum of two prices.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 {
            self
        } else {
            other
        }
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / SCALE;
        let frac = self.0 % SCALE;
        if frac == 0 {
            write!(f, "{whole}.0")
        } else {
            // Format with up to 18 decimal places, trimming trailing zeros
            let frac_str = format!("{frac:018}");
            let trimmed = frac_str.trim_end_matches('0');
            write!(f, "{whole}.{trimmed}")
        }
    }
}

// =============================================================================
// Quantity
// =============================================================================

/// A fixed-point quantity with 18 decimal places.
///
/// Quantities represent amounts of assets (base or quote tokens).
/// Like [`Price`], they use 18 decimal places to match ERC-20 standards.
///
/// # Example
///
/// ```
/// use types::Quantity;
///
/// let qty = Quantity::from_whole(100); // 100 tokens
/// let half = qty.checked_div_scalar(2).unwrap(); // 50 tokens
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Quantity(u128);

impl Quantity {
    /// The zero quantity.
    pub const ZERO: Self = Self(0);

    /// The maximum representable quantity.
    pub const MAX: Self = Self(u128::MAX);

    /// Creates a `Quantity` from an already-scaled value.
    #[must_use]
    pub const fn from_scaled(scaled: u128) -> Self {
        Self(scaled)
    }

    /// Creates a `Quantity` from a whole number.
    #[must_use]
    pub const fn from_whole(whole: u64) -> Self {
        Self((whole as u128) * SCALE)
    }

    /// Tries to create a `Quantity` from a whole number.
    #[must_use]
    pub const fn try_from_whole(whole: u128) -> Option<Self> {
        match whole.checked_mul(SCALE) {
            Some(scaled) => Some(Self(scaled)),
            None => None,
        }
    }

    /// Returns the raw scaled value.
    #[must_use]
    pub const fn as_scaled(self) -> u128 {
        self.0
    }

    /// Returns `true` if the quantity is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Checked addition.
    #[must_use]
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked subtraction.
    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked multiplication by a scalar.
    #[must_use]
    pub const fn checked_mul_scalar(self, scalar: u64) -> Option<Self> {
        match self.0.checked_mul(scalar as u128) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked division by a scalar.
    #[must_use]
    pub const fn checked_div_scalar(self, divisor: u64) -> Option<Self> {
        if divisor == 0 {
            return None;
        }
        Some(Self(self.0 / divisor as u128))
    }

    /// Multiplies quantity by price to get notional value (in quote currency).
    ///
    /// This performs: `(quantity * price) / SCALE` to maintain correct decimals.
    /// Returns `None` on overflow.
    #[must_use]
    pub const fn checked_mul_price(self, price: Price) -> Option<Self> {
        // To avoid overflow, we use: (a * b) / SCALE
        // We need to be careful about intermediate overflow
        // Using u128, max safe product before division is u128::MAX
        match self.0.checked_mul(price.as_scaled()) {
            Some(product) => Some(Self(product / SCALE)),
            None => None,
        }
    }

    /// Saturating addition.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// Saturating subtraction.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Returns the minimum of two quantities.
    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        if self.0 <= other.0 {
            self
        } else {
            other
        }
    }

    /// Returns the maximum of two quantities.
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 {
            self
        } else {
            other
        }
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / SCALE;
        let frac = self.0 % SCALE;
        if frac == 0 {
            write!(f, "{whole}.0")
        } else {
            let frac_str = format!("{frac:018}");
            let trimmed = frac_str.trim_end_matches('0');
            write!(f, "{whole}.{trimmed}")
        }
    }
}

// =============================================================================
// Timestamp
// =============================================================================

/// Unix timestamp in milliseconds.
///
/// Used for order timestamps, expiry times, and event ordering.
/// Millisecond precision is sufficient for trading purposes and allows
/// for fine-grained time-priority ordering.
///
/// # Example
///
/// ```
/// use types::Timestamp;
///
/// let ts = Timestamp::from_millis(1700000000000);
/// assert_eq!(ts.as_millis(), 1700000000000);
/// assert_eq!(ts.as_secs(), 1700000000);
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Creates a `Timestamp` from milliseconds since Unix epoch.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Creates a `Timestamp` from seconds since Unix epoch.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs * 1000)
    }

    /// Returns the timestamp in milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Returns the timestamp in seconds (truncated).
    #[must_use]
    pub const fn as_secs(self) -> u64 {
        self.0 / 1000
    }

    /// Returns `true` if this timestamp is before another.
    #[must_use]
    pub const fn is_before(self, other: Self) -> bool {
        self.0 < other.0
    }

    /// Returns `true` if this timestamp is after another.
    #[must_use]
    pub const fn is_after(self, other: Self) -> bool {
        self.0 > other.0
    }

    /// Checked addition of milliseconds.
    #[must_use]
    pub const fn checked_add_millis(self, millis: u64) -> Option<Self> {
        match self.0.checked_add(millis) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Saturating subtraction. Returns the difference or zero.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

// =============================================================================
// Nonce
// =============================================================================

/// Monotonically increasing counter for replay protection.
///
/// Each user maintains a nonce that must increase with each order.
/// This prevents replay attacks where old signed orders could be resubmitted.
///
/// # Example
///
/// ```
/// use types::Nonce;
///
/// let nonce = Nonce::new(0);
/// let next = nonce.next();
/// assert_eq!(next.as_u64(), 1);
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Nonce(u64);

impl Nonce {
    /// Creates a new `Nonce` with the given value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the next nonce value.
    ///
    /// # Panics
    ///
    /// Panics if the nonce would overflow. In practice, this will never happen
    /// as it would require 2^64 orders from a single account.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Returns the next nonce value, or `None` on overflow.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Returns `true` if this nonce is valid given an expected minimum.
    ///
    /// A nonce is valid if it is greater than or equal to the expected value.
    #[must_use]
    pub const fn is_valid(self, expected_min: Self) -> bool {
        self.0 >= expected_min.0
    }
}

impl fmt::Display for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Nonce {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod price_tests {
        use super::*;

        #[test]
        fn test_zero() {
            assert!(Price::ZERO.is_zero());
            assert_eq!(Price::ZERO.as_scaled(), 0);
        }

        #[test]
        fn test_from_whole() {
            let price = Price::from_whole(100);
            assert_eq!(price.as_scaled(), 100 * SCALE);
        }

        #[test]
        fn test_from_scaled() {
            let price = Price::from_scaled(SCALE / 2); // 0.5
            assert_eq!(price.as_scaled(), SCALE / 2);
        }

        #[test]
        fn test_try_from_whole_overflow() {
            // This should overflow
            let result = Price::try_from_whole(u128::MAX);
            assert!(result.is_none());
        }

        #[test]
        fn test_checked_add() {
            let p1 = Price::from_whole(100);
            let p2 = Price::from_whole(50);
            let sum = p1.checked_add(p2).unwrap();
            assert_eq!(sum, Price::from_whole(150));
        }

        #[test]
        fn test_checked_add_overflow() {
            let result = Price::MAX.checked_add(Price::from_scaled(1));
            assert!(result.is_none());
        }

        #[test]
        fn test_checked_sub() {
            let p1 = Price::from_whole(100);
            let p2 = Price::from_whole(30);
            let diff = p1.checked_sub(p2).unwrap();
            assert_eq!(diff, Price::from_whole(70));
        }

        #[test]
        fn test_checked_sub_underflow() {
            let p1 = Price::from_whole(10);
            let p2 = Price::from_whole(20);
            assert!(p1.checked_sub(p2).is_none());
        }

        #[test]
        fn test_checked_mul_scalar() {
            let price = Price::from_whole(50);
            let doubled = price.checked_mul_scalar(2).unwrap();
            assert_eq!(doubled, Price::from_whole(100));
        }

        #[test]
        fn test_checked_div_scalar() {
            let price = Price::from_whole(100);
            let halved = price.checked_div_scalar(2).unwrap();
            assert_eq!(halved, Price::from_whole(50));
        }

        #[test]
        fn test_checked_div_by_zero() {
            let price = Price::from_whole(100);
            assert!(price.checked_div_scalar(0).is_none());
        }

        #[test]
        fn test_saturating_add() {
            let result = Price::MAX.saturating_add(Price::from_scaled(1));
            assert_eq!(result, Price::MAX);
        }

        #[test]
        fn test_saturating_sub() {
            let result = Price::ZERO.saturating_sub(Price::from_scaled(1));
            assert_eq!(result, Price::ZERO);
        }

        #[test]
        fn test_min_max() {
            let p1 = Price::from_whole(100);
            let p2 = Price::from_whole(200);

            assert_eq!(p1.min(p2), p1);
            assert_eq!(p1.max(p2), p2);
        }

        #[test]
        fn test_ordering() {
            let p1 = Price::from_whole(100);
            let p2 = Price::from_whole(200);
            assert!(p1 < p2);
        }

        #[test]
        fn test_display_whole() {
            let price = Price::from_whole(100);
            assert_eq!(format!("{price}"), "100.0");
        }

        #[test]
        fn test_display_fractional() {
            let price = Price::from_scaled(SCALE + SCALE / 2); // 1.5
            assert_eq!(format!("{price}"), "1.5");
        }

        #[test]
        fn test_display_small_fraction() {
            let price = Price::from_scaled(SCALE / 4); // 0.25
            assert_eq!(format!("{price}"), "0.25");
        }

        #[test]
        fn test_borsh_roundtrip() {
            let price = Price::from_scaled(123_456_789_012_345_678);
            let bytes = borsh::to_vec(&price).unwrap();
            let decoded: Price = borsh::from_slice(&bytes).unwrap();
            assert_eq!(price, decoded);
        }
    }

    mod quantity_tests {
        use super::*;

        #[test]
        fn test_zero() {
            assert!(Quantity::ZERO.is_zero());
        }

        #[test]
        fn test_from_whole() {
            let qty = Quantity::from_whole(1000);
            assert_eq!(qty.as_scaled(), 1000 * SCALE);
        }

        #[test]
        fn test_checked_mul_price() {
            // 10 tokens * price of 2.0 = 20 (in quote currency)
            let qty = Quantity::from_whole(10);
            let price = Price::from_whole(2);
            let notional = qty.checked_mul_price(price).unwrap();
            assert_eq!(notional, Quantity::from_whole(20));
        }

        #[test]
        fn test_checked_mul_price_fractional() {
            // 10 tokens * price of 1.5 = 15
            let qty = Quantity::from_whole(10);
            let price = Price::from_scaled(SCALE + SCALE / 2); // 1.5
            let notional = qty.checked_mul_price(price).unwrap();
            assert_eq!(notional, Quantity::from_whole(15));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let qty = Quantity::from_scaled(999_888_777_666_555_444);
            let bytes = borsh::to_vec(&qty).unwrap();
            let decoded: Quantity = borsh::from_slice(&bytes).unwrap();
            assert_eq!(qty, decoded);
        }
    }

    mod timestamp_tests {
        use super::*;

        #[test]
        fn test_from_millis() {
            let ts = Timestamp::from_millis(1_700_000_000_000);
            assert_eq!(ts.as_millis(), 1_700_000_000_000);
        }

        #[test]
        fn test_from_secs() {
            let ts = Timestamp::from_secs(1_700_000_000);
            assert_eq!(ts.as_secs(), 1_700_000_000);
            assert_eq!(ts.as_millis(), 1_700_000_000_000);
        }

        #[test]
        fn test_ordering() {
            let t1 = Timestamp::from_millis(1000);
            let t2 = Timestamp::from_millis(2000);

            assert!(t1.is_before(t2));
            assert!(t2.is_after(t1));
            assert!(t1 < t2);
        }

        #[test]
        fn test_checked_add_millis() {
            let ts = Timestamp::from_millis(1000);
            let later = ts.checked_add_millis(500).unwrap();
            assert_eq!(later.as_millis(), 1500);
        }

        #[test]
        fn test_borsh_roundtrip() {
            let ts = Timestamp::from_millis(1_700_000_000_000);
            let bytes = borsh::to_vec(&ts).unwrap();
            let decoded: Timestamp = borsh::from_slice(&bytes).unwrap();
            assert_eq!(ts, decoded);
        }
    }

    mod nonce_tests {
        use super::*;

        #[test]
        fn test_new_and_next() {
            let nonce = Nonce::new(5);
            assert_eq!(nonce.as_u64(), 5);
            assert_eq!(nonce.next().as_u64(), 6);
        }

        #[test]
        fn test_is_valid() {
            let nonce = Nonce::new(10);
            assert!(nonce.is_valid(Nonce::new(10))); // equal is valid
            assert!(nonce.is_valid(Nonce::new(5))); // greater is valid
            assert!(!nonce.is_valid(Nonce::new(15))); // less is invalid
        }

        #[test]
        fn test_checked_next_overflow() {
            let nonce = Nonce::new(u64::MAX);
            assert!(nonce.checked_next().is_none());
        }

        #[test]
        fn test_from_u64() {
            let nonce: Nonce = 42.into();
            assert_eq!(nonce.as_u64(), 42);
        }

        #[test]
        fn test_borsh_roundtrip() {
            let nonce = Nonce::new(12345);
            let bytes = borsh::to_vec(&nonce).unwrap();
            let decoded: Nonce = borsh::from_slice(&bytes).unwrap();
            assert_eq!(nonce, decoded);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::{MarketId, OrderId};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn price_add_sub_inverse(a in 0u128..u128::MAX/2, b in 0u128..u128::MAX/2) {
            let pa = Price::from_scaled(a);
            let pb = Price::from_scaled(b);

            if let Some(sum) = pa.checked_add(pb) {
                let back = sum.checked_sub(pb).unwrap();
                prop_assert_eq!(back, pa);
            }
        }

        #[test]
        fn quantity_add_sub_inverse(a in 0u128..u128::MAX/2, b in 0u128..u128::MAX/2) {
            let qa = Quantity::from_scaled(a);
            let qb = Quantity::from_scaled(b);

            if let Some(sum) = qa.checked_add(qb) {
                let back = sum.checked_sub(qb).unwrap();
                prop_assert_eq!(back, qa);
            }
        }

        #[test]
        fn price_ordering_consistent(a in any::<u128>(), b in any::<u128>()) {
            let pa = Price::from_scaled(a);
            let pb = Price::from_scaled(b);

            // Ordering should match underlying u128 ordering
            prop_assert_eq!(pa.cmp(&pb), a.cmp(&b));
        }

        #[test]
        fn order_id_roundtrip(market in any::<u64>(), seq in any::<u64>()) {
            let market_id = MarketId::new(market);
            let order_id = OrderId::new(market_id, seq);

            prop_assert_eq!(order_id.market_id(), market_id);
            prop_assert_eq!(order_id.sequence(), seq);
        }

        #[test]
        fn price_borsh_roundtrip(value in any::<u128>()) {
            let price = Price::from_scaled(value);
            let bytes = borsh::to_vec(&price).unwrap();
            let decoded: Price = borsh::from_slice(&bytes).unwrap();
            prop_assert_eq!(price, decoded);
        }

        #[test]
        fn quantity_borsh_roundtrip(value in any::<u128>()) {
            let qty = Quantity::from_scaled(value);
            let bytes = borsh::to_vec(&qty).unwrap();
            let decoded: Quantity = borsh::from_slice(&bytes).unwrap();
            prop_assert_eq!(qty, decoded);
        }
    }
}
