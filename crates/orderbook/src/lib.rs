//! # Order Book Data Structures
//!
//! This crate implements the order book data structures for the trading system.
//! It provides efficient storage and retrieval of orders organized by price level.
//!
//! ## Design
//!
//! The order book uses a price-time priority model:
//! 1. **Price Priority**: Better prices are matched first
//!    - For bids (buy orders): higher prices have priority
//!    - For asks (sell orders): lower prices have priority
//! 2. **Time Priority**: At the same price, earlier orders are matched first (FIFO)
//!
//! ## Data Structures
//!
//! - [`PriceLevel`]: A single price level containing a FIFO queue of order IDs
//! - [`OrderBook`]: The complete order book with bids and asks organized by price
//! - [`OrderBookSnapshot`]: A serializable snapshot of the order book state
//! - [`L2BookSummary`]: An aggregated (price-level) view for market data
//!
//! ## Note
//!
//! This crate only handles data storage and retrieval. The matching logic
//! is implemented in a separate `matching` crate.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod book;
pub mod ops;
pub mod price_level;
pub mod snapshot;

// Re-export main types at crate root
pub use book::OrderBook;
pub use ops::{OrderBookError, Result};
pub use price_level::PriceLevel;
pub use snapshot::{L2BookSummary, L2Level, OrderBookSnapshot, PriceLevelSnapshot};
