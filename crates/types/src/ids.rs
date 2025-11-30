//! Identifier types for the trading system.
//!
//! This module defines the various ID types used throughout the system:
//! - [`MarketId`]: Identifies a trading pair (e.g., ETH/USDC)
//! - [`OrderId`]: Globally unique order identifier (encodes market + sequence)
//! - [`Address`]: Ethereum address (20 bytes)

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

// =============================================================================
// MarketId
// =============================================================================

/// Unique identifier for a trading market (e.g., ETH/USDC).
///
/// Markets are assigned sequential IDs when created. The on-chain contract
/// stores the mapping from `MarketId` to the actual token pair addresses.
///
/// # Example
///
/// ```
/// use types::MarketId;
///
/// let market = MarketId::new(1);
/// assert_eq!(market.as_u64(), 1);
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
pub struct MarketId(u64);

impl MarketId {
    /// Creates a new `MarketId` from a raw `u64` value.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw `u64` value of this market ID.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for MarketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Market({})", self.0)
    }
}

impl From<u64> for MarketId {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

// =============================================================================
// OrderId
// =============================================================================

/// Globally unique order identifier.
///
/// An `OrderId` encodes both the market and a sequence number:
/// - **Upper 64 bits**: [`MarketId`]
/// - **Lower 64 bits**: Sequence number (unique within the market)
///
/// This encoding allows O(1) extraction of the market from any order ID
/// and ensures global uniqueness across all markets.
///
/// # Example
///
/// ```
/// use types::{MarketId, OrderId};
///
/// let market = MarketId::new(1);
/// let order_id = OrderId::new(market, 42);
///
/// assert_eq!(order_id.market_id(), market);
/// assert_eq!(order_id.sequence(), 42);
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
pub struct OrderId(u128);

impl OrderId {
    /// Creates a new `OrderId` from a market ID and sequence number.
    ///
    /// The market ID is stored in the upper 64 bits, and the sequence
    /// number in the lower 64 bits.
    #[must_use]
    pub const fn new(market_id: MarketId, sequence: u64) -> Self {
        let combined = ((market_id.0 as u128) << 64) | (sequence as u128);
        Self(combined)
    }

    /// Creates an `OrderId` from a raw `u128` value.
    ///
    /// This is useful for deserialization or when the combined value
    /// is already known.
    #[must_use]
    pub const fn from_raw(raw: u128) -> Self {
        Self(raw)
    }

    /// Returns the raw `u128` value of this order ID.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }

    /// Extracts the [`MarketId`] from this order ID.
    #[must_use]
    pub const fn market_id(self) -> MarketId {
        MarketId::new((self.0 >> 64) as u64)
    }

    /// Extracts the sequence number from this order ID.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn sequence(self) -> u64 {
        self.0 as u64
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Order({}-{})",
            self.market_id().as_u64(),
            self.sequence()
        )
    }
}

// =============================================================================
// Address
// =============================================================================

/// Ethereum address (20 bytes).
///
/// Represents a user's wallet address or a contract address on the blockchain.
/// Stored as a fixed-size byte array for efficient serialization.
///
/// # Example
///
/// ```
/// use types::Address;
///
/// // From hex string (with 0x prefix)
/// let addr = Address::from_hex("0x742d35Cc6634C0532925a3b844Bc9e7595f0bE01").unwrap();
///
/// // To hex string
/// assert!(addr.to_hex().starts_with("0x"));
///
/// // Zero address
/// let zero = Address::zero();
/// assert!(zero.is_zero());
/// ```
#[derive(
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
pub struct Address([u8; 20]);

impl Address {
    /// The length of an Ethereum address in bytes.
    pub const LENGTH: usize = 20;

    /// Creates a new `Address` from a 20-byte array.
    #[must_use]
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Returns the zero address (0x0000...0000).
    ///
    /// The zero address is often used as a placeholder or to represent
    /// "no address" / "burn address".
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 20])
    }

    /// Returns `true` if this is the zero address.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }

    /// Returns the address as a byte slice.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Creates an `Address` from a hex string.
    ///
    /// The string may optionally start with "0x" or "0X".
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The string length is incorrect (must be 40 hex chars, or 42 with 0x prefix)
    /// - The string contains invalid hex characters
    pub fn from_hex(s: &str) -> std::result::Result<Self, AddressParseError> {
        let s = s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s);

        if s.len() != 40 {
            return Err(AddressParseError::InvalidLength {
                expected: 40,
                actual: s.len(),
            });
        }

        let bytes = hex::decode(s).map_err(|e| AddressParseError::InvalidHex(e.to_string()))?;

        let mut arr = [0u8; 20];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Converts the address to a hex string with "0x" prefix.
    #[must_use]
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_hex())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display shortened form: 0x1234...5678
        let hex = hex::encode(self.0);
        write!(f, "0x{}...{}", &hex[..4], &hex[36..])
    }
}

impl TryFrom<&str> for Address {
    type Error = AddressParseError;

    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        Self::from_hex(s)
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Self::new(bytes)
    }
}

/// Error when parsing an address from a string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddressParseError {
    /// The hex string has an invalid length.
    #[error("invalid address length: expected {expected} hex chars, got {actual}")]
    InvalidLength {
        /// Expected length in hex characters.
        expected: usize,
        /// Actual length in hex characters.
        actual: usize,
    },
    /// The string contains invalid hex characters.
    #[error("invalid hex: {0}")]
    InvalidHex(String),
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod market_id_tests {
        use super::*;

        #[test]
        fn test_new_and_as_u64() {
            let market = MarketId::new(42);
            assert_eq!(market.as_u64(), 42);
        }

        #[test]
        fn test_default() {
            let market = MarketId::default();
            assert_eq!(market.as_u64(), 0);
        }

        #[test]
        fn test_from_u64() {
            let market: MarketId = 123.into();
            assert_eq!(market.as_u64(), 123);
        }

        #[test]
        fn test_display() {
            let market = MarketId::new(5);
            assert_eq!(format!("{market}"), "Market(5)");
        }

        #[test]
        fn test_ordering() {
            let m1 = MarketId::new(1);
            let m2 = MarketId::new(2);
            assert!(m1 < m2);
        }

        #[test]
        fn test_borsh_roundtrip() {
            let market = MarketId::new(12345);
            let bytes = borsh::to_vec(&market).unwrap();
            let decoded: MarketId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(market, decoded);
        }
    }

    mod order_id_tests {
        use super::*;

        #[test]
        fn test_new_and_extract() {
            let market = MarketId::new(7);
            let order = OrderId::new(market, 999);

            assert_eq!(order.market_id(), market);
            assert_eq!(order.sequence(), 999);
        }

        #[test]
        fn test_from_raw() {
            let market = MarketId::new(7);
            let order = OrderId::new(market, 999);
            let raw = order.as_u128();

            let reconstructed = OrderId::from_raw(raw);
            assert_eq!(reconstructed, order);
            assert_eq!(reconstructed.market_id(), market);
            assert_eq!(reconstructed.sequence(), 999);
        }

        #[test]
        fn test_encoding_independence() {
            // Different markets with same sequence should be different
            let o1 = OrderId::new(MarketId::new(1), 100);
            let o2 = OrderId::new(MarketId::new(2), 100);
            assert_ne!(o1, o2);

            // Same market with different sequences should be different
            let o3 = OrderId::new(MarketId::new(1), 101);
            assert_ne!(o1, o3);
        }

        #[test]
        fn test_max_values() {
            let market = MarketId::new(u64::MAX);
            let order = OrderId::new(market, u64::MAX);

            assert_eq!(order.market_id().as_u64(), u64::MAX);
            assert_eq!(order.sequence(), u64::MAX);
        }

        #[test]
        fn test_display() {
            let order = OrderId::new(MarketId::new(3), 42);
            assert_eq!(format!("{order}"), "Order(3-42)");
        }

        #[test]
        fn test_borsh_roundtrip() {
            let order = OrderId::new(MarketId::new(123), 456_789);
            let bytes = borsh::to_vec(&order).unwrap();
            let decoded: OrderId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(order, decoded);
        }
    }

    mod address_tests {
        use super::*;

        #[test]
        fn test_zero_address() {
            let zero = Address::zero();
            assert!(zero.is_zero());
            assert_eq!(zero.as_bytes(), &[0u8; 20]);
        }

        #[test]
        fn test_from_bytes() {
            let bytes = [1u8; 20];
            let addr = Address::new(bytes);
            assert_eq!(addr.as_bytes(), &bytes);
            assert!(!addr.is_zero());
        }

        #[test]
        fn test_from_hex_with_prefix() {
            let hex = "0x742d35Cc6634C0532925a3b844Bc9e7595f0bE01";
            let addr = Address::from_hex(hex).unwrap();
            assert!(!addr.is_zero());

            // Round-trip
            let hex_out = addr.to_hex();
            let addr2 = Address::from_hex(&hex_out).unwrap();
            assert_eq!(addr, addr2);
        }

        #[test]
        fn test_from_hex_without_prefix() {
            let hex = "742d35Cc6634C0532925a3b844Bc9e7595f0bE01";
            let addr = Address::from_hex(hex).unwrap();
            assert!(!addr.is_zero());
        }

        #[test]
        fn test_from_hex_uppercase_prefix() {
            let hex = "0X742d35Cc6634C0532925a3b844Bc9e7595f0bE01";
            let addr = Address::from_hex(hex).unwrap();
            assert!(!addr.is_zero());
        }

        #[test]
        fn test_from_hex_invalid_length() {
            let result = Address::from_hex("0x1234");
            assert!(matches!(
                result,
                Err(AddressParseError::InvalidLength {
                    expected: 40,
                    actual: 4
                })
            ));
        }

        #[test]
        fn test_from_hex_invalid_chars() {
            let result = Address::from_hex("0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG");
            assert!(matches!(result, Err(AddressParseError::InvalidHex(_))));
        }

        #[test]
        fn test_display_shortened() {
            let addr = Address::from_hex("0x742d35Cc6634C0532925a3b844Bc9e7595f0bE01").unwrap();
            let display = format!("{addr}");
            assert!(display.starts_with("0x742d"));
            assert!(display.ends_with("be01")); // hex encode produces lowercase
            assert!(display.contains("..."));
        }

        #[test]
        fn test_try_from_str() {
            let addr: Address = "0x742d35Cc6634C0532925a3b844Bc9e7595f0bE01"
                .try_into()
                .unwrap();
            assert!(!addr.is_zero());
        }

        #[test]
        fn test_borsh_roundtrip() {
            let addr = Address::from_hex("0x742d35Cc6634C0532925a3b844Bc9e7595f0bE01").unwrap();
            let bytes = borsh::to_vec(&addr).unwrap();
            let decoded: Address = borsh::from_slice(&bytes).unwrap();
            assert_eq!(addr, decoded);
        }
    }
}
