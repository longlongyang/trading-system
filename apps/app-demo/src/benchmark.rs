//! Performance benchmarks for the trading system.
//!
//! Provides synthetic tests to measure throughput and latency of the
//! matching engine under various scenarios.

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]

use std::time::{Duration, Instant};

use matching::MatchingEngine;
use orderbook::OrderBook;
use types::{
    Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity, TimeInForce, Timestamp,
};

// =============================================================================
// Benchmark Configuration
// =============================================================================

/// Configuration for benchmark runs.
struct BenchConfig {
    /// Number of resting orders to build in the book.
    resting_orders: usize,
    /// Number of taker orders to run through matching.
    taker_orders: usize,
    /// Number of price levels to spread orders across.
    price_levels: usize,
    /// Base price for the order book.
    base_price: u64,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            resting_orders: 10_000_000,
            taker_orders: 5_000_000,
            price_levels: 100,
            base_price: 1000,
        }
    }
}

// =============================================================================
// Benchmark Entry Point
// =============================================================================

/// Run all benchmarks.
pub fn run_benchmarks() {
    println!("==============================================");
    println!("  Trading System Benchmark Suite");
    println!("==============================================");
    println!();
    println!("Running performance tests...");
    println!();

    // Warm up the CPU
    warmup();

    let config = BenchConfig::default();

    // Run individual benchmarks
    bench_order_insertion(&config);
    bench_deep_book_matching(&config);
    bench_rapid_matching(&config);
    bench_cancel_orders(&config);

    println!();
    println!("==============================================");
    println!("  Benchmark Complete");
    println!("==============================================");
}

/// Warm up to stabilize CPU frequency.
fn warmup() {
    print!("Warming up... ");
    let start = Instant::now();
    let mut sum = 0u64;
    while start.elapsed() < Duration::from_millis(500) {
        for i in 0..10_000 {
            sum = sum.wrapping_add(i);
        }
    }
    // Prevent optimization
    if sum == 0 {
        println!("({})", sum);
    }
    println!("done.");
    println!();
}

// =============================================================================
// Individual Benchmarks
// =============================================================================

/// Benchmark: Insert many orders into an empty book.
fn bench_order_insertion(config: &BenchConfig) {
    println!("─────────────────────────────────────────────");
    println!("Benchmark: Order Insertion");
    println!("─────────────────────────────────────────────");
    println!(
        "Inserting {} orders across {} price levels...",
        config.resting_orders, config.price_levels
    );

    let market_id = MarketId::new(1);
    let mut book = OrderBook::with_capacity(market_id, config.resting_orders);
    let engine = MatchingEngine::new();

    let orders = generate_resting_orders(
        market_id,
        config.resting_orders,
        config.price_levels,
        config.base_price,
        OrderSide::Sell, // All asks so they don't match each other
    );

    let start = Instant::now();

    for order in orders {
        let _ = engine.place_order(&mut book, order);
    }

    let elapsed = start.elapsed();

    let orders_per_sec = config.resting_orders as f64 / elapsed.as_secs_f64();
    let ns_per_order = elapsed.as_nanos() as f64 / config.resting_orders as f64;

    println!("  Total time:     {:?}", elapsed);
    println!("  Orders:         {}", config.resting_orders);
    println!("  Throughput:     {:.0} orders/sec", orders_per_sec);
    println!("  Latency:        {:.0} ns/order", ns_per_order);
    println!("  Book depth:     {} levels", book.ask_level_count());
    println!();
}

/// Benchmark: Match taker orders against a deep book.
fn bench_deep_book_matching(config: &BenchConfig) {
    println!("─────────────────────────────────────────────");
    println!("Benchmark: Deep Book Matching");
    println!("─────────────────────────────────────────────");
    println!(
        "Building book with {} resting orders, then matching {} taker orders...",
        config.resting_orders, config.taker_orders
    );

    let market_id = MarketId::new(1);
    let mut book = OrderBook::with_capacity(market_id, config.resting_orders);
    let engine = MatchingEngine::new();

    // Build resting ask orders (sells)
    let resting = generate_resting_orders(
        market_id,
        config.resting_orders,
        config.price_levels,
        config.base_price,
        OrderSide::Sell,
    );

    for order in resting {
        let _ = engine.place_order(&mut book, order);
    }

    // Generate taker (buy) orders that will cross the spread
    let takers = generate_taker_orders(
        market_id,
        config.taker_orders,
        config.resting_orders as u64 + 1, // Start sequence after resting orders
        config.base_price + config.price_levels as u64, // Price high enough to match
        OrderSide::Buy,
    );

    let mut total_fills = 0usize;
    let mut latencies = Vec::with_capacity(config.taker_orders);

    let start = Instant::now();

    for order in takers {
        let order_start = Instant::now();
        let result = engine.place_order(&mut book, order);
        let order_elapsed = order_start.elapsed();
        latencies.push(order_elapsed);

        if let Ok(report) = result {
            total_fills += report.fills.len();
        }
    }

    let elapsed = start.elapsed();

    let orders_per_sec = config.taker_orders as f64 / elapsed.as_secs_f64();
    let fills_per_sec = total_fills as f64 / elapsed.as_secs_f64();

    // Latency stats
    latencies.sort();
    let p50 = latencies
        .get(latencies.len() / 2)
        .copied()
        .unwrap_or_default();
    let p99 = latencies
        .get(latencies.len() * 99 / 100)
        .copied()
        .unwrap_or_default();
    let min_lat = latencies.first().copied().unwrap_or_default();
    let max_lat = latencies.last().copied().unwrap_or_default();
    let avg_lat: Duration = latencies.iter().sum::<Duration>() / latencies.len().max(1) as u32;

    println!("  Total time:     {:?}", elapsed);
    println!("  Taker orders:   {}", config.taker_orders);
    println!("  Total fills:    {}", total_fills);
    println!("  Throughput:     {:.0} orders/sec", orders_per_sec);
    println!("  Fill rate:      {:.0} fills/sec", fills_per_sec);
    println!("  Latency (avg):  {:?}", avg_lat);
    println!("  Latency (p50):  {:?}", p50);
    println!("  Latency (p99):  {:?}", p99);
    println!("  Latency (min):  {:?}", min_lat);
    println!("  Latency (max):  {:?}", max_lat);
    println!();
}

/// Benchmark: Rapid matching at a single price level.
fn bench_rapid_matching(config: &BenchConfig) {
    println!("─────────────────────────────────────────────");
    println!("Benchmark: Rapid Single-Level Matching");
    println!("─────────────────────────────────────────────");

    let num_orders = config.taker_orders * 2;
    println!(
        "Alternating buy/sell orders at same price ({} total)...",
        num_orders
    );

    let market_id = MarketId::new(1);
    let mut book = OrderBook::new(market_id);
    let engine = MatchingEngine::new();

    let fixed_price = config.base_price;
    let mut total_fills = 0usize;
    let mut seq = 1u64;

    let start = Instant::now();

    for i in 0..num_orders {
        // Alternate buy and sell
        let side = if i % 2 == 0 {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };

        let order = Order::new_limit(
            OrderId::new(market_id, seq),
            market_id,
            Address::new([((i % 200) + 10) as u8; 20]), // Different owners to avoid self-trade
            side,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(fixed_price),
            Quantity::from_whole(1),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        );
        seq += 1;

        if let Ok(report) = engine.place_order(&mut book, order) {
            total_fills += report.fills.len();
        }
    }

    let elapsed = start.elapsed();

    let orders_per_sec = num_orders as f64 / elapsed.as_secs_f64();
    let fills_per_sec = total_fills as f64 / elapsed.as_secs_f64();
    let ns_per_order = elapsed.as_nanos() as f64 / num_orders as f64;

    println!("  Total time:     {:?}", elapsed);
    println!("  Orders:         {}", num_orders);
    println!("  Total fills:    {}", total_fills);
    println!("  Throughput:     {:.0} orders/sec", orders_per_sec);
    println!("  Fill rate:      {:.0} fills/sec", fills_per_sec);
    println!("  Latency:        {:.0} ns/order", ns_per_order);
    println!();
}

/// Benchmark: Cancel orders.
fn bench_cancel_orders(config: &BenchConfig) {
    println!("─────────────────────────────────────────────");
    println!("Benchmark: Order Cancellation");
    println!("─────────────────────────────────────────────");

    let num_orders = config.resting_orders;
    println!("Inserting {} orders, then cancelling all...", num_orders);

    let market_id = MarketId::new(1);
    let mut book = OrderBook::with_capacity(market_id, num_orders);
    let engine = MatchingEngine::new();

    // Insert orders
    let orders = generate_resting_orders(
        market_id,
        num_orders,
        config.price_levels,
        config.base_price,
        OrderSide::Sell,
    );

    let order_ids: Vec<_> = orders.iter().map(|o| o.id).collect();

    for order in orders {
        let _ = engine.place_order(&mut book, order);
    }

    // Cancel all orders
    let start = Instant::now();

    let mut cancelled = 0usize;
    for order_id in &order_ids {
        let result = engine.cancel_order(&mut book, *order_id);
        if matches!(result.outcome, matching::CancelOutcome::Cancelled) {
            cancelled += 1;
        }
    }

    let elapsed = start.elapsed();

    let cancels_per_sec = cancelled as f64 / elapsed.as_secs_f64();
    let ns_per_cancel = elapsed.as_nanos() as f64 / cancelled as f64;

    println!("  Total time:     {:?}", elapsed);
    println!("  Cancelled:      {}", cancelled);
    println!("  Throughput:     {:.0} cancels/sec", cancels_per_sec);
    println!("  Latency:        {:.0} ns/cancel", ns_per_cancel);
    println!("  Book empty:     {}", book.is_empty());
    println!();
}

// =============================================================================
// Order Generators
// =============================================================================

/// Generate resting limit orders spread across price levels.
fn generate_resting_orders(
    market_id: MarketId,
    count: usize,
    price_levels: usize,
    base_price: u64,
    side: OrderSide,
) -> Vec<Order> {
    let mut orders = Vec::with_capacity(count);
    let orders_per_level = count / price_levels;

    for level in 0..price_levels {
        let price = if side == OrderSide::Sell {
            base_price + level as u64
        } else {
            base_price - level as u64
        };

        for i in 0..orders_per_level {
            let seq = (level * orders_per_level + i + 1) as u64;
            let order = Order::new_limit(
                OrderId::new(market_id, seq),
                market_id,
                Address::new([((seq % 200) + 10) as u8; 20]), // Varying owners
                side,
                TimeInForce::GoodTilCancelled,
                Price::from_whole(price),
                Quantity::from_whole(1), // 1 unit each
                Nonce::new(seq),
                Timestamp::from_millis(seq * 1000),
            );
            orders.push(order);
        }
    }

    orders
}

/// Generate taker orders that will cross the spread.
fn generate_taker_orders(
    market_id: MarketId,
    count: usize,
    start_seq: u64,
    price: u64,
    side: OrderSide,
) -> Vec<Order> {
    let mut orders = Vec::with_capacity(count);

    for i in 0..count {
        let seq = start_seq + i as u64;
        let order = Order::new_limit(
            OrderId::new(market_id, seq),
            market_id,
            Address::new([((seq % 200) + 10) as u8; 20]),
            side,
            TimeInForce::ImmediateOrCancel, // IOC so they don't rest
            Price::from_whole(price),
            Quantity::from_whole(1),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        );
        orders.push(order);
    }

    orders
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_resting_orders() {
        let market_id = MarketId::new(1);
        let orders = generate_resting_orders(market_id, 100, 10, 1000, OrderSide::Sell);
        assert_eq!(orders.len(), 100);

        // Check first order
        assert_eq!(orders[0].side, OrderSide::Sell);
        assert_eq!(orders[0].price, Price::from_whole(1000));
    }

    #[test]
    fn test_generate_taker_orders() {
        let market_id = MarketId::new(1);
        let orders = generate_taker_orders(market_id, 50, 100, 1000, OrderSide::Buy);
        assert_eq!(orders.len(), 50);
        assert_eq!(orders[0].id.sequence(), 100);
        assert_eq!(orders[0].side, OrderSide::Buy);
        assert_eq!(orders[0].time_in_force, TimeInForce::ImmediateOrCancel);
    }
}
