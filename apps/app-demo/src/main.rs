//! # Trading System Demo Application
//!
//! A small but solid demo app that demonstrates the order book and matching
//! engine working correctly.
//!
//! ## Usage
//!
//! ```bash
//! # Interactive demo mode (default)
//! cargo run -p app-demo
//!
//! # Benchmark mode
//! cargo run -p app-demo -- bench
//!
//! # Release benchmark (more accurate timing)
//! cargo run -p app-demo --release -- bench
//! ```

mod benchmark;
mod cli;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Check for benchmark mode
    if args.len() > 1 && args[1] == "bench" {
        benchmark::run_benchmarks();
    } else {
        cli::run_interactive();
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use matching::MatchingEngine;
    use orderbook::OrderBook;
    use types::{
        Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity, TimeInForce,
        Timestamp,
    };

    /// Smoke test: verify the matching engine can be created and a basic order
    /// can be placed through the same logic used by the app.
    #[test]
    fn smoke_test_place_order() {
        let market_id = MarketId::new(1);
        let mut book = OrderBook::new(market_id);
        let engine = MatchingEngine::new();

        // Place a sell order (will rest since no bids)
        let order = Order::new_limit(
            OrderId::new(market_id, 1),
            market_id,
            Address::new([1u8; 20]),
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );

        let result = engine.place_order(&mut book, order);
        assert!(result.is_ok());
        assert_eq!(book.order_count(), 1);
        assert_eq!(book.best_ask_price(), Some(Price::from_whole(100)));
    }

    /// Smoke test: verify matching produces fills correctly.
    #[test]
    fn smoke_test_matching() {
        let market_id = MarketId::new(1);
        let mut book = OrderBook::new(market_id);
        let engine = MatchingEngine::new();

        // Place a sell order
        let sell = Order::new_limit(
            OrderId::new(market_id, 1),
            market_id,
            Address::new([1u8; 20]),
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );
        engine.place_order(&mut book, sell).unwrap();

        // Place a crossing buy order
        let buy = Order::new_limit(
            OrderId::new(market_id, 2),
            market_id,
            Address::new([2u8; 20]),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(5),
            Nonce::new(1),
            Timestamp::from_millis(2000),
        );
        let result = engine.place_order(&mut book, buy).unwrap();

        assert!(result.has_fills());
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.fills[0].quantity, Quantity::from_whole(5));
    }
}
