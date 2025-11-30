//! Error types for the trading engine.
//!
//! This module defines [`EngineError`], a comprehensive error enum that
//! covers all error conditions that can occur in the matching engine
//! and order processing.
//!
//! # Error Categories
//!
//! - **Order validation**: Invalid prices, quantities, parameters
//! - **Account errors**: Insufficient balance, invalid nonce
//! - **Market errors**: Market not found, halted, or closed
//! - **Matching errors**: Self-trade, post-only rejection
//! - **System errors**: Internal errors, overflow

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{AccountError, MarketId, OrderId};

/// Comprehensive error type for the trading engine.
///
/// This enum covers all error conditions that can occur during order
/// processing, matching, and settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EngineError {
    // =========================================================================
    // Order Validation Errors
    // =========================================================================
    /// Order quantity is zero or below minimum.
    InvalidQuantity,

    /// Order price is zero or invalid for a limit order.
    InvalidPrice,

    /// Price does not conform to market tick size.
    InvalidTickSize,

    /// Quantity does not conform to market lot size.
    InvalidLotSize,

    /// Order value is below market minimum.
    OrderValueTooSmall,

    /// Order size exceeds market maximum.
    OrderSizeTooLarge,

    /// Market order has a price specified (should be zero).
    MarketOrderWithPrice,

    /// Limit order is missing a price.
    LimitOrderWithoutPrice,

    // =========================================================================
    // Account Errors
    // =========================================================================
    /// Account not found.
    AccountNotFound,

    /// Insufficient available balance to place order.
    InsufficientBalance,

    /// Insufficient locked balance (internal error).
    InsufficientLockedBalance,

    /// Invalid nonce (replay attack or out of order).
    InvalidNonce,

    // =========================================================================
    // Market Errors
    // =========================================================================
    /// Market not found.
    MarketNotFound {
        /// The market ID that was not found.
        market_id: MarketId,
    },

    /// Market is not accepting new orders.
    MarketNotActive {
        /// The market ID.
        market_id: MarketId,
    },

    /// Market is halted.
    MarketHalted {
        /// The market ID.
        market_id: MarketId,
    },

    /// Market is closed.
    MarketClosed {
        /// The market ID.
        market_id: MarketId,
    },

    // =========================================================================
    // Order Errors
    // =========================================================================
    /// Order not found.
    OrderNotFound {
        /// The order ID that was not found.
        order_id: OrderId,
    },

    /// Order is already in a terminal state.
    OrderNotActive {
        /// The order ID.
        order_id: OrderId,
    },

    /// Cannot cancel an order owned by another user.
    NotOrderOwner,

    /// Duplicate order ID.
    DuplicateOrderId {
        /// The duplicate order ID.
        order_id: OrderId,
    },

    // =========================================================================
    // Matching Errors
    // =========================================================================
    /// Self-trade detected (same user on both sides).
    SelfTrade,

    /// Post-only order would have crossed the spread.
    PostOnlyWouldCross,

    /// Fill-or-kill order could not be completely filled.
    FillOrKillNotFilled,

    // =========================================================================
    // System Errors
    // =========================================================================
    /// Arithmetic overflow in calculation.
    Overflow,

    /// Internal error (bug in the engine).
    InternalError {
        /// Description of the error.
        message: String,
    },

    /// Serialization/deserialization error.
    SerializationError {
        /// Description of the error.
        message: String,
    },
}

impl EngineError {
    /// Creates an internal error with the given message.
    #[must_use]
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::InternalError {
            message: message.into(),
        }
    }

    /// Creates a serialization error with the given message.
    #[must_use]
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    /// Returns `true` if this is a validation error (user mistake).
    #[must_use]
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidQuantity
                | Self::InvalidPrice
                | Self::InvalidTickSize
                | Self::InvalidLotSize
                | Self::OrderValueTooSmall
                | Self::OrderSizeTooLarge
                | Self::MarketOrderWithPrice
                | Self::LimitOrderWithoutPrice
        )
    }

    /// Returns `true` if this is an account-related error.
    #[must_use]
    pub fn is_account_error(&self) -> bool {
        matches!(
            self,
            Self::AccountNotFound
                | Self::InsufficientBalance
                | Self::InsufficientLockedBalance
                | Self::InvalidNonce
        )
    }

    /// Returns `true` if this is a market-related error.
    #[must_use]
    pub fn is_market_error(&self) -> bool {
        matches!(
            self,
            Self::MarketNotFound { .. }
                | Self::MarketNotActive { .. }
                | Self::MarketHalted { .. }
                | Self::MarketClosed { .. }
        )
    }

    /// Returns `true` if this is an internal/system error.
    #[must_use]
    pub fn is_internal_error(&self) -> bool {
        matches!(
            self,
            Self::Overflow | Self::InternalError { .. } | Self::SerializationError { .. }
        )
    }

    /// Returns `true` if this error should be retried.
    ///
    /// Generally, only transient errors should be retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        // Most errors are not retryable in a trading system
        // The user needs to fix the issue and resubmit
        false
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Order validation
            Self::InvalidQuantity => write!(f, "invalid order quantity"),
            Self::InvalidPrice => write!(f, "invalid order price"),
            Self::InvalidTickSize => write!(f, "price does not conform to tick size"),
            Self::InvalidLotSize => write!(f, "quantity does not conform to lot size"),
            Self::OrderValueTooSmall => write!(f, "order value below minimum"),
            Self::OrderSizeTooLarge => write!(f, "order size exceeds maximum"),
            Self::MarketOrderWithPrice => write!(f, "market order should not have price"),
            Self::LimitOrderWithoutPrice => write!(f, "limit order requires price"),

            // Account
            Self::AccountNotFound => write!(f, "account not found"),
            Self::InsufficientBalance => write!(f, "insufficient available balance"),
            Self::InsufficientLockedBalance => write!(f, "insufficient locked balance"),
            Self::InvalidNonce => write!(f, "invalid nonce"),

            // Market
            Self::MarketNotFound { market_id } => {
                write!(f, "market {market_id} not found")
            }
            Self::MarketNotActive { market_id } => {
                write!(f, "market {market_id} is not active")
            }
            Self::MarketHalted { market_id } => {
                write!(f, "market {market_id} is halted")
            }
            Self::MarketClosed { market_id } => {
                write!(f, "market {market_id} is closed")
            }

            // Order
            Self::OrderNotFound { order_id } => {
                write!(f, "order {order_id} not found")
            }
            Self::OrderNotActive { order_id } => {
                write!(f, "order {order_id} is not active")
            }
            Self::NotOrderOwner => write!(f, "not the order owner"),
            Self::DuplicateOrderId { order_id } => {
                write!(f, "duplicate order id {order_id}")
            }

            // Matching
            Self::SelfTrade => write!(f, "self-trade not allowed"),
            Self::PostOnlyWouldCross => write!(f, "post-only order would cross spread"),
            Self::FillOrKillNotFilled => write!(f, "fill-or-kill order could not be filled"),

            // System
            Self::Overflow => write!(f, "arithmetic overflow"),
            Self::InternalError { message } => write!(f, "internal error: {message}"),
            Self::SerializationError { message } => write!(f, "serialization error: {message}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<AccountError> for EngineError {
    fn from(err: AccountError) -> Self {
        match err {
            AccountError::InsufficientBalance => Self::InsufficientBalance,
            AccountError::InsufficientLockedBalance => Self::InsufficientLockedBalance,
            AccountError::BalanceOverflow => Self::Overflow,
            AccountError::InvalidNonce => Self::InvalidNonce,
        }
    }
}

/// Result type alias for engine operations.
pub type EngineResult<T> = Result<T, EngineError>;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_validation_error() {
        assert!(EngineError::InvalidQuantity.is_validation_error());
        assert!(EngineError::InvalidPrice.is_validation_error());
        assert!(EngineError::InvalidTickSize.is_validation_error());
        assert!(!EngineError::InsufficientBalance.is_validation_error());
        assert!(!EngineError::SelfTrade.is_validation_error());
    }

    #[test]
    fn test_is_account_error() {
        assert!(EngineError::InsufficientBalance.is_account_error());
        assert!(EngineError::InvalidNonce.is_account_error());
        assert!(!EngineError::InvalidQuantity.is_account_error());
    }

    #[test]
    fn test_is_market_error() {
        let market_id = MarketId::new(1);
        assert!(EngineError::MarketNotFound { market_id }.is_market_error());
        assert!(EngineError::MarketHalted { market_id }.is_market_error());
        assert!(!EngineError::InvalidQuantity.is_market_error());
    }

    #[test]
    fn test_is_internal_error() {
        assert!(EngineError::Overflow.is_internal_error());
        assert!(EngineError::internal("test").is_internal_error());
        assert!(EngineError::serialization("test").is_internal_error());
        assert!(!EngineError::InvalidQuantity.is_internal_error());
    }

    #[test]
    fn test_display() {
        assert_eq!(
            format!("{}", EngineError::InvalidQuantity),
            "invalid order quantity"
        );
        assert_eq!(
            format!(
                "{}",
                EngineError::MarketNotFound {
                    market_id: MarketId::new(42)
                }
            ),
            "market Market(42) not found"
        );
        assert_eq!(
            format!("{}", EngineError::internal("oops")),
            "internal error: oops"
        );
    }

    #[test]
    fn test_from_account_error() {
        let err: EngineError = AccountError::InsufficientBalance.into();
        assert_eq!(err, EngineError::InsufficientBalance);

        let err: EngineError = AccountError::BalanceOverflow.into();
        assert_eq!(err, EngineError::Overflow);
    }

    #[test]
    fn test_is_error_trait() {
        fn check_error<E: std::error::Error>(_e: E) {}
        check_error(EngineError::InvalidQuantity);
    }

    #[test]
    fn test_borsh_roundtrip() {
        let errors = vec![
            EngineError::InvalidQuantity,
            EngineError::MarketNotFound {
                market_id: MarketId::new(1),
            },
            EngineError::OrderNotFound {
                order_id: OrderId::new(MarketId::new(1), 100),
            },
            EngineError::internal("test message"),
        ];

        for err in errors {
            let bytes = borsh::to_vec(&err).unwrap();
            let decoded: EngineError = borsh::from_slice(&bytes).unwrap();
            assert_eq!(err, decoded);
        }
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_result(fail: bool) -> EngineResult<u32> {
            if fail {
                Err(EngineError::InvalidQuantity)
            } else {
                Ok(42)
            }
        }

        assert_eq!(returns_result(false).unwrap(), 42);
        assert!(returns_result(true).is_err());
    }
}
