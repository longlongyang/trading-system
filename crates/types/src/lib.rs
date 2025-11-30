//! # Trading System Core Types
//!
//! This crate defines the foundational types used throughout the trading system.
//! These types are designed to be:
//!
//! - **Deterministic**: All numeric types use fixed-point arithmetic to ensure
//!   identical results across different platforms.
//! - **Serializable**: All types implement `borsh` and `serde` serialization.
//! - **zkVM-compatible**: No floating point, no non-deterministic operations.
//!
//! ## Modules
//!
//! - [`ids`]: Identifier types (`MarketId`, `OrderId`, `Address`)
//! - [`primitives`]: Numeric primitives (`Price`, `Quantity`, `Timestamp`, `Nonce`)
//! - [`order`]: Order-related types (`Order`, `OrderSide`, `OrderType`, `TimeInForce`)
//! - [`trade`]: Trade execution types (`Trade`, `Fill`)
//! - [`market`]: Market configuration (`MarketConfig`)
//! - [`account`]: Account and balance types (`Account`, `Balance`)
//! - [`error`]: Error types (`EngineError`)

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod account;
pub mod error;
pub mod ids;
pub mod market;
pub mod order;
pub mod primitives;
pub mod trade;

// Re-export commonly used types at crate root for convenience
pub use account::{Account, AccountError, Balance};
pub use error::{EngineError, EngineResult};
pub use ids::{Address, AddressParseError, MarketId, OrderId};
pub use market::{FeeConfig, MarketConfig, MarketStatus};
pub use order::{Order, OrderSide, OrderStatus, OrderType, TimeInForce};
pub use primitives::{Nonce, Price, Quantity, Timestamp, DECIMALS, SCALE};
pub use trade::{Fill, FillRole, Trade, TradeId};

/// Result type alias using [`EngineError`].
pub type Result<T> = std::result::Result<T, EngineError>;
