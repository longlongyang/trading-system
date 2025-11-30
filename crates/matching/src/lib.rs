//! # Matching Engine Core
//!
//! This crate implements the core matching engine for the trading system.
//! It sits on top of the order book data structures (from `orderbook` crate)
//! and implements price-time priority matching.
//!
//! ## Design Overview
//!
//! The matching engine is responsible for:
//! 1. Processing incoming orders against the order book
//! 2. Generating fills when orders match
//! 3. Updating the order book state (adding/removing orders)
//! 4. Handling different time-in-force policies (GTC, IOC, FOK, `PostOnly`)
//! 5. Supporting market orders
//! 6. Self-trade prevention
//!
//! ## What This Crate Does NOT Do
//!
//! - **Balance management**: Checking/reserving balances is done by a higher layer
//! - **Fee calculation**: Fees are computed during settlement (Stage 5)
//! - **Order validation**: Market-level validation (tick size, min qty) is external
//! - **Trade ID assignment**: The caller is responsible for assigning trade IDs
//!
//! ## Usage
//!
//! ```
//! use matching::MatchingEngine;
//! use orderbook::OrderBook;
//! use types::{
//!     Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity,
//!     TimeInForce, Timestamp,
//! };
//!
//! // Create an order book and matching engine
//! let market_id = MarketId::new(1);
//! let mut book = OrderBook::new(market_id);
//! let mut engine = MatchingEngine::new();
//!
//! // Create a sell order (will rest in book since no bids)
//! let sell_order = Order::new_limit(
//!     OrderId::new(market_id, 1),
//!     market_id,
//!     Address::zero(),
//!     OrderSide::Sell,
//!     TimeInForce::GoodTilCancelled,
//!     Price::from_whole(100),
//!     Quantity::from_whole(10),
//!     Nonce::new(1),
//!     Timestamp::from_millis(1000),
//! );
//!
//! let result = engine.place_order(&mut book, sell_order);
//! assert!(result.is_ok());
//! ```
//!
//! ## Matching Rules
//!
//! 1. **Price Priority**: Better prices are matched first
//!    - Bids: Higher price has priority
//!    - Asks: Lower price has priority
//!
//! 2. **Time Priority**: At the same price, earlier orders are matched first (FIFO)
//!
//! 3. **Price Determination**: Trades execute at the maker's (resting) price
//!
//! ## Time-in-Force Behaviors
//!
//! - **GTC (Good-til-Cancelled)**: Match what's possible, rest remainder in book
//! - **IOC (Immediate-or-Cancel)**: Match what's possible, cancel remainder
//! - **FOK (Fill-or-Kill)**: Fill entirely or cancel entirely
//! - **`PostOnly`**: Only rest in book; reject if would immediately match

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod engine;
mod error;
mod matcher;
mod result;

// Re-export main types at crate root
pub use engine::MatchingEngine;
pub use error::MatchingError;
pub use result::{
    CancelOutcome, CancelResult, ExecutionReport, MatchFill, MatchOutcome, MatchResult,
    OrderPlacement,
};
