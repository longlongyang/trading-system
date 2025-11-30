# Trading System

A high-performance order book and matching engine written in Rust, designed for hybrid on-chain/off-chain trading on Ethereum L2 rollups.

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Features

- 🚀 **High Performance**: ~3.3M order insertions/sec, ~2M+ matches/sec
- 📊 **Full Order Book**: Price-time priority matching with BTreeMap + HashMap
- ⏱️ **All TIF Policies**: GTC, IOC, FOK, PostOnly support
- 🔢 **Fixed-Point Arithmetic**: u128 with 18 decimals (ERC-20 compatible)
- 🔒 **Deterministic**: Identical inputs always produce identical outputs
- 🧪 **Well Tested**: 280+ unit tests with property-based testing

## Quick Start

### Prerequisites

- Rust 1.75 or later
- Cargo

### Build

```bash
cargo build --workspace
```

### Run Tests

```bash
cargo test --workspace
```

### Run Demo Application

Interactive CLI:
```bash
cargo run -p app-demo
```

Benchmark mode:
```bash
cargo run -p app-demo --release -- bench
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Trading System                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   types     │  │  orderbook  │  │      matching       │  │
│  │             │  │             │  │                     │  │
│  │ • Order     │  │ • OrderBook │  │ • MatchingEngine    │  │
│  │ • Trade     │  │ • PriceLevel│  │ • ExecutionReport   │  │
│  │ • Price     │  │ • Snapshot  │  │ • MatchFill         │  │
│  │ • Address   │  │ • L2Summary │  │ • MatchOutcome      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │                    app-demo                          │    │
│  │         Interactive CLI + Benchmarks                 │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| [`types`](crates/types/) | Core types: Order, Trade, Price, Quantity, Address |
| [`orderbook`](crates/orderbook/) | Order book data structures with price-level management |
| [`matching`](crates/matching/) | Matching engine with all TIF policies |
| [`app-demo`](apps/app-demo/) | Interactive demo CLI and benchmarks |

## Usage Example

```rust
use types::{Order, OrderSide, OrderType, TimeInForce, Price, Quantity, Address, OrderId, Timestamp};
use orderbook::OrderBook;
use matching::MatchingEngine;

// Create an order book and matching engine
let mut book = OrderBook::new();
let engine = MatchingEngine::new();

// Create a limit order
let order = Order::new(
    OrderId::new(1, 1),           // market_id=1, sequence=1
    Address::ZERO,                 // trader address
    OrderSide::Buy,
    OrderType::Limit,
    TimeInForce::GTC,
    Price::from_u64(100),         // price: 100.0
    Quantity::from_u64(10),       // quantity: 10.0
    Timestamp::now(),
);

// Place the order
let report = engine.place_order(&mut book, order).unwrap();

match report.outcome {
    MatchOutcome::Resting => println!("Order resting in book"),
    MatchOutcome::Filled => println!("Order fully filled!"),
    MatchOutcome::PartiallyFilledResting => println!("Partial fill, rest in book"),
    _ => {}
}
```

## Performance

Benchmarks run on a single thread (Intel/AMD modern CPU):

| Operation | Throughput |
|-----------|------------|
| Order Insertion | ~3,300,000 ops/sec |
| Deep Book Matching | ~2,000,000 ops/sec |
| Single-Level Matching | ~6,500,000 ops/sec |

## Project Status

| Stage | Description | Status |
|-------|-------------|--------|
| 1 | Core Types & Primitives | ✅ Complete |
| 2 | Order Book Data Structures | ✅ Complete |
| 3 | Matching Engine Core | ✅ Complete |
| 4 | Account & Balance Management | 🔜 Planned |
| 5 | Fee Calculation & Trade Settlement | 🔜 Planned |
| 6 | Batch Assembly & Serialization | 🔜 Planned |
| 7-9 | Smart Contracts (Solidity) | 🔜 Planned |
| 10+ | Integration & Production | 🔜 Planned |

See [IMPLEMENTATION_STAGES.md](docs/IMPLEMENTATION_STAGES.md) for the full roadmap.

## Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design and tradeoffs
- [Implementation Stages](docs/IMPLEMENTATION_STAGES.md) - Detailed implementation plan
- [Concurrency Design](docs/CONCURRENCY_DESIGN.md) - Threading patterns and DashMap analysis
- [Performance Profiling](docs/PERFORMANCE_PROFILING.md) - How to profile with perf and flamegraph
- [Types Design Notes](crates/types/DESIGN_NOTES.md) - Type system decisions

## Design Principles

1. **Single-Threaded Matching**: Core engine is single-threaded for determinism and simplicity
2. **Scale via Sharding**: One engine per market, horizontal scaling
3. **Fixed-Point Math**: No floating point - u128 with 18 decimals everywhere
4. **Fail-Safe Arithmetic**: All operations return `Option<T>` for explicit overflow handling
5. **Serialization Ready**: Borsh for deterministic state, Serde for APIs

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please ensure:

1. All tests pass: `cargo test --workspace`
2. Code is formatted: `cargo fmt`
3. No clippy warnings: `cargo clippy --workspace`
