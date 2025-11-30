//! Error types for the matching engine.
//!
//! This module defines errors that can occur during order matching and
//! order book manipulation by the matching engine.

use std::fmt;

use types::OrderId;

/// Errors that can occur during matching engine operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchingError {
    /// Order not found in the order book.
    OrderNotFound {
        /// The order ID that was not found.
        order_id: OrderId,
    },

    /// Order already exists in the order book.
    OrderAlreadyExists {
        /// The duplicate order ID.
        order_id: OrderId,
    },

    /// Order belongs to a different market.
    WrongMarket {
        /// The order ID.
        order_id: OrderId,
    },

    /// Market orders cannot be placed with `PostOnly` time-in-force.
    MarketOrderPostOnly,

    /// Post-only order would have immediately matched (crossed the spread).
    ///
    /// This error is returned when a `PostOnly` order is submitted at a price
    /// that would result in immediate execution. `PostOnly` orders are intended
    /// to add liquidity only.
    PostOnlyWouldCross,

    /// Fill-or-kill order could not be completely filled.
    ///
    /// The order required full execution but there wasn't enough liquidity
    /// at acceptable prices.
    FillOrKillNotFilled {
        /// The quantity that was requested.
        requested: types::Quantity,
        /// The quantity that was available.
        available: types::Quantity,
    },

    /// Self-trade would occur (maker and taker are the same address).
    ///
    /// This is informational - self-trades are skipped, not errored.
    /// The engine continues matching with other orders.
    SelfTradeSkipped,

    /// Order book operation failed.
    OrderBookError {
        /// Description of the error.
        message: String,
    },

    /// Arithmetic overflow during matching calculations.
    Overflow,
}

impl fmt::Display for MatchingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OrderNotFound { order_id } => {
                write!(f, "order not found: {order_id}")
            }
            Self::OrderAlreadyExists { order_id } => {
                write!(f, "order already exists: {order_id}")
            }
            Self::WrongMarket { order_id } => {
                write!(f, "order belongs to different market: {order_id}")
            }
            Self::MarketOrderPostOnly => {
                write!(f, "market orders cannot use PostOnly time-in-force")
            }
            Self::PostOnlyWouldCross => {
                write!(f, "post-only order would have crossed the spread")
            }
            Self::FillOrKillNotFilled {
                requested,
                available,
            } => {
                write!(
                    f,
                    "fill-or-kill order could not be filled: requested {requested}, available {available}"
                )
            }
            Self::SelfTradeSkipped => {
                write!(f, "self-trade was skipped")
            }
            Self::OrderBookError { message } => {
                write!(f, "order book error: {message}")
            }
            Self::Overflow => {
                write!(f, "arithmetic overflow during matching")
            }
        }
    }
}

impl std::error::Error for MatchingError {}

impl From<orderbook::OrderBookError> for MatchingError {
    fn from(err: orderbook::OrderBookError) -> Self {
        Self::OrderBookError {
            message: err.to_string(),
        }
    }
}
