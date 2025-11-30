# Concurrency Design Guide

## Overview

This document provides comprehensive guidance on concurrency patterns for the trading system, specifically addressing:

1. **DashMap deep-dive** - when to use it and when to avoid it
2. **Order book & matching engine concurrency** - why single-threaded is the right choice
3. **System-wide concurrency architecture** - for stages 4+
4. **Migration advice** - safe optimizations without disrupting the core engine

---

## Table of Contents

1. [DashMap In-Depth Analysis](#1-dashmap-in-depth-analysis)
2. [Order Book & Matching Engine Concurrency](#2-order-book--matching-engine-concurrency)
3. [Recommended Concurrency Architecture](#3-recommended-concurrency-architecture)
4. [Where DashMap Is Appropriate](#4-where-dashmap-is-appropriate)
5. [Migration & Optimization Advice](#5-migration--optimization-advice)
6. [Summary & Recommendations](#6-summary--recommendations)

---

## 1. DashMap In-Depth Analysis

### 1.1 What Is DashMap?

`DashMap` is a concurrent hash map implementation for Rust, designed as a drop-in replacement for `std::collections::HashMap` in multi-threaded contexts.

**High-Level Design:**

```
┌─────────────────────────────────────────────────────────────┐
│                        DashMap<K, V>                        │
├─────────────────────────────────────────────────────────────┤
│  Shard 0    │  Shard 1    │  Shard 2    │  ...  │ Shard N-1 │
│  ┌───────┐  │  ┌───────┐  │  ┌───────┐  │       │ ┌───────┐ │
│  │RwLock │  │  │RwLock │  │  │RwLock │  │       │ │RwLock │ │
│  │HashMap│  │  │HashMap│  │  │HashMap│  │       │ │HashMap│ │
│  └───────┘  │  └───────┘  │  └───────┘  │       │ └───────┘ │
└─────────────────────────────────────────────────────────────┘
              │
              │  Key hash determines shard
              ▼
        hash(key) % num_shards → Shard Index
```

**Key characteristics:**

- **Sharding**: Divides the key space into N shards (typically `num_cpus * 4` by default)
- **Per-shard locking**: Each shard has its own `RwLock`, reducing contention
- **Lock-free reads**: Multiple readers can access different shards concurrently
- **Fine-grained writes**: Writes only lock the relevant shard

### 1.2 DashMap vs. Alternatives

#### Comparison Matrix

| Feature | `HashMap` | `hashbrown::HashMap` | `Mutex<HashMap>` | `RwLock<HashMap>` | `DashMap` |
|---------|-----------|---------------------|------------------|-------------------|-----------|
| Thread-safe | ❌ No | ❌ No | ✅ Yes | ✅ Yes | ✅ Yes |
| Concurrent reads | N/A | N/A | ❌ Serialized | ✅ Yes | ✅ Yes |
| Concurrent writes | N/A | N/A | ❌ Serialized | ❌ Serialized | ⚠️ Per-shard |
| Lock granularity | N/A | N/A | Global | Global | Per-shard |
| Read latency | ~20ns | ~15ns | ~50-200ns | ~30-100ns | ~30-80ns |
| Write latency | ~30ns | ~25ns | ~50-200ns | ~100-500ns | ~50-150ns |
| Memory overhead | Low | Low | +Lock | +Lock | +N locks |
| Iteration cost | O(n) | O(n) | O(n) + lock | O(n) + lock | O(n) + N locks |
| API complexity | Simple | Simple | Wrapped | Wrapped | Custom refs |

#### When to Use Each

**`std::collections::HashMap` / `hashbrown::HashMap`:**
- Single-threaded code (our matching engine hot path)
- Inside a single-threaded task or actor
- When wrapped by a higher-level concurrency primitive

**`Mutex<HashMap<K, V>>`:**
- Simple concurrent access with infrequent contention
- When you need to hold the lock across multiple operations
- Small maps where sharding overhead isn't justified

**`RwLock<HashMap<K, V>>`:**
- Read-heavy workloads (many readers, few writers)
- When reads vastly outnumber writes (10:1 or more)
- When write operations are batched

**`DashMap<K, V>`:**
- High-contention concurrent access from many threads
- When operations are mostly independent (different keys)
- Connection/session registries, caches, metrics aggregation
- When you want simple API without manual lock management

### 1.3 DashMap Limitations in Trading Contexts

#### ❌ NOT Suitable for Order Book / Matching Engine Hot Path

**Reason 1: Determinism Requirements**

```rust
// PROBLEM: Non-deterministic iteration order
fn process_all_orders(book: &DashMap<OrderId, Order>) {
    // Iteration order depends on:
    // 1. Hash function randomization
    // 2. Shard acquisition order
    // 3. Internal hash table layout
    for entry in book.iter() {
        process_order(entry.value());  // Non-deterministic order!
    }
}
```

Price-time priority matching requires deterministic ordering. DashMap provides no ordering guarantees.

**Reason 2: Atomic Multi-Key Operations**

```rust
// PROBLEM: No atomic multi-key operations
fn match_orders(
    book: &DashMap<OrderId, Order>,
    taker_id: OrderId,
    maker_id: OrderId,
) {
    // These are separate locks - not atomic!
    let mut taker = book.get_mut(&taker_id).unwrap();
    let mut maker = book.get_mut(&maker_id).unwrap();  // May deadlock!
    
    // If another thread tries maker then taker = deadlock
    execute_fill(&mut taker, &mut maker);
}
```

Matching requires updating multiple orders atomically (taker + one or more makers).

**Reason 3: Priority Queue Integration**

```rust
// PROBLEM: DashMap doesn't integrate with BTreeMap price levels
struct ConcurrentOrderBook {
    // How do you make this concurrent?
    bids: BTreeMap<Reverse<Price>, PriceLevel>,  // No concurrent BTreeMap!
    asks: BTreeMap<Price, PriceLevel>,
    orders: DashMap<OrderId, Order>,  // Separate from price levels!
}
```

The order book requires coordinated access to:
1. Price-level maps (BTreeMap) - for price priority
2. FIFO queues within levels (VecDeque) - for time priority  
3. Order lookup map (HashMap) - for O(1) access by ID

These must be updated atomically. DashMap only helps with #3.

**Reason 4: Tail Latency**

```
Single-threaded HashMap:        DashMap under contention:
┌────────────────────┐          ┌────────────────────┐
│  p50:  20ns        │          │  p50:  50ns        │
│  p99:  50ns        │          │  p99:  500ns       │
│  p999: 100ns       │          │  p999: 5µs         │  ← Lock contention spikes
│  max:  200ns       │          │  max:  50µs        │
└────────────────────┘          └────────────────────┘
```

Lock contention causes unpredictable latency spikes, which are unacceptable for matching.

### 1.4 DashMap Performance Characteristics

```rust
// Benchmark: 8 threads, 1M operations each

// Scenario 1: Independent keys (ideal for DashMap)
// Each thread operates on its own key range
dashmap_independent_keys:    ~45ns/op
rwlock_hashmap_independent:  ~120ns/op  // Lock contention
mutex_hashmap_independent:   ~180ns/op  // Full serialization

// Scenario 2: Hot keys (worst case for DashMap)
// All threads compete for same 100 keys
dashmap_hot_keys:            ~200ns/op  // Shard contention
rwlock_hashmap_hot_keys:     ~250ns/op  // Similar
mutex_hashmap_hot_keys:      ~300ns/op  // Slightly worse

// Scenario 3: Mixed read/write (80% read, 20% write)
dashmap_mixed:               ~60ns/op
rwlock_hashmap_mixed:        ~80ns/op   // RwLock shines here
mutex_hashmap_mixed:         ~150ns/op
```

**Key insight**: DashMap excels when different threads access different keys. When threads compete for the same keys (common in order books where best bid/ask are hot), the benefit diminishes.

---

## 2. Order Book & Matching Engine Concurrency

### 2.1 Industry Standard Pattern

The standard architecture used by professional exchanges (CME, NYSE, LMAX, etc.):

```
┌─────────────────────────────────────────────────────────────────────┐
│                     NETWORK I/O LAYER                               │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│  │ IO      │ │ IO      │ │ IO      │ │ IO      │ │ IO      │       │
│  │ Thread 1│ │ Thread 2│ │ Thread 3│ │ Thread 4│ │ Thread N│       │
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘       │
│       │           │           │           │           │             │
└───────┼───────────┼───────────┼───────────┼───────────┼─────────────┘
        │           │           │           │           │
        └─────────┬─┴───────────┴───────────┴───────┬───┘
                  │     Orders routed by market     │
                  ▼                                 ▼
        ┌─────────────────┐               ┌─────────────────┐
        │ Market A Queue  │               │ Market B Queue  │
        │ (MPSC Channel)  │               │ (MPSC Channel)  │
        └────────┬────────┘               └────────┬────────┘
                 │                                 │
                 ▼                                 ▼
        ┌─────────────────┐               ┌─────────────────┐
        │  MATCHING       │               │  MATCHING       │
        │  ENGINE A       │               │  ENGINE B       │
        │                 │               │                 │
        │  • OrderBook    │               │  • OrderBook    │
        │  • MatchingCore │               │  • MatchingCore │
        │  • Single-      │               │  • Single-      │
        │    threaded!    │               │    threaded!    │
        └────────┬────────┘               └────────┬────────┘
                 │                                 │
                 ▼                                 ▼
        ┌─────────────────┐               ┌─────────────────┐
        │ Outbound Queue  │               │ Outbound Queue  │
        └────────┬────────┘               └────────┬────────┘
                 │                                 │
                 └───────────────┬─────────────────┘
                                 │
                                 ▼
                    ┌────────────────────────┐
                    │ Market Data Publisher  │
                    │ Settlement Batcher     │
                    └────────────────────────┘
```

**Why this works:**

1. **Horizontal scaling**: Add more markets = add more engine threads
2. **No lock contention**: Each engine owns its data exclusively  
3. **Deterministic**: Single-threaded = perfectly reproducible
4. **Simple reasoning**: No race conditions within matching logic
5. **Cache-friendly**: Single thread = hot cache lines stay hot

### 2.2 Why NOT Make the Order Book Concurrent

#### The Fundamental Problem

```rust
// What a "concurrent order book" would need to handle:
fn match_order_concurrent(
    book: &ConcurrentOrderBook,
    taker: Order,
) -> Vec<Fill> {
    // Step 1: Find best opposing level
    let best_price = book.asks.best_price();  // Need lock
    
    // Step 2: Get orders at that level
    let level = book.asks.get_level(best_price);  // Need lock
    
    // Step 3: Match against orders in FIFO order
    for maker_id in level.order_ids() {  // Need to hold lock
        let maker = book.orders.get(&maker_id);  // Another lock
        
        // Step 4: Update both orders
        // PROBLEM: How to atomically update:
        // - maker order quantity
        // - taker order quantity  
        // - price level total quantity
        // - remove order if filled
        // - remove level if empty
        // - update order status
        // ALL ATOMICALLY?
    }
}
```

**Options, all bad:**

1. **Global lock**: Defeats the purpose, same as single-threaded
2. **Fine-grained locks**: Deadlock risk, complexity explosion
3. **Lock-free structures**: Don't exist for this data model
4. **Software transactional memory**: High overhead, not production-ready in Rust

#### Contention Analysis

Even if you solved the atomicity problem, contention kills performance:

```
Order book hot spots:

Best Bid Level ─────┐
                    │ 90% of orders touch
Best Ask Level ─────┤ these two levels
                    │
Other Levels ───────┴── 10% of orders

With concurrent access:
- 90% of operations contend on the same 2 locks
- Effectively serialized anyway
- But now with lock overhead added
```

#### Your Current Performance Is Already Excellent

```
Your benchmarks:
─────────────────────────────────────────
Order insertion:     ~1.64M orders/sec   (~610ns/order)
Deep book matching:  ~1.30M orders/sec   (~770ns/order)
Hot-path matching:   ~7.60M orders/sec   (~130ns/order)
Cancellations:       ~2.10M cancels/sec  (~476ns/cancel)
─────────────────────────────────────────

Industry context:
- CME Globex: ~1M orders/sec per engine (similar to your numbers)
- LMAX: Claims 6M orders/sec (matches your hot-path)
- NYSE: ~100k-500k orders/sec per symbol

Your single-threaded engine is already competitive with
production exchange systems. Adding concurrency would likely
make it SLOWER due to coordination overhead.
```

### 2.3 The Determinism Requirement

**Critical for:**
- Replaying the order stream must produce identical state
- Dispute resolution (proving the engine behaved correctly)
- zkVM execution (future Option C from ARCHITECTURE.md)
- Debugging and testing

**Concurrent access breaks determinism:**

```rust
// Single-threaded: deterministic
fn process_orders_sequential(orders: &[Order]) -> State {
    let mut engine = MatchingEngine::new();
    for order in orders {
        engine.process(order);  // Same order every time
    }
    engine.state()  // Identical result for identical input
}

// Concurrent: non-deterministic
fn process_orders_concurrent(orders: &[Order]) -> State {
    let engine = Arc::new(ConcurrentEngine::new());
    
    orders.par_iter().for_each(|order| {
        engine.process(order);  // Race condition!
    });
    
    engine.state()  // Different result each run!
}
```

**Even with "correct" concurrent execution, you lose:**
- Reproducible test cases
- Ability to replay production issues
- Audit trail validity
- zkVM compatibility

### 2.4 Recommendation: Keep the Core Single-Threaded

**CLEAR RECOMMENDATION: Do NOT add concurrency to the order book or matching engine.**

**Instead:**

1. Keep `OrderBook` and `MatchingEngine` exactly as they are
2. Scale horizontally by running one engine per market
3. Use channels/queues for communication
4. Apply concurrency ONLY at the boundaries (networking, persistence)

---

## 3. Recommended Concurrency Architecture

### 3.1 High-Level Design (Stage 4+)

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        NETWORK I/O POOL                                  │
│                     (tokio runtime, many tasks)                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐                    │
│  │WebSocket │ │WebSocket │ │  REST    │ │  REST    │                    │
│  │Handler 1 │ │Handler N │ │Handler 1 │ │Handler N │                    │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘                    │
│       │            │            │            │                          │
│       └────────────┴──────┬─────┴────────────┘                          │
│                           │                                              │
│                           ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    SHARED STATE LAYER                            │    │
│  │                                                                  │    │
│  │  ┌─────────────────────┐  ┌─────────────────────┐               │    │
│  │  │ Sessions            │  │ Rate Limiters        │               │    │
│  │  │ Arc<DashMap<        │  │ Arc<DashMap<         │               │    │
│  │  │   SessionId,        │  │   UserId,            │               │    │
│  │  │   SessionState>>    │  │   RateLimitState>>   │               │    │
│  │  └─────────────────────┘  └─────────────────────┘               │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                           │                                              │
└───────────────────────────┼──────────────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            │               │               │
            ▼               ▼               ▼
    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
    │Market Router │ │Market Router │ │Market Router │
    │   BTC/USD    │ │   ETH/USD    │ │   SOL/USD    │
    │              │ │              │ │              │
    │ ┌──────────┐ │ │ ┌──────────┐ │ │ ┌──────────┐ │
    │ │  MPSC    │ │ │ │  MPSC    │ │ │ │  MPSC    │ │
    │ │ Channel  │ │ │ │ Channel  │ │ │ │ Channel  │ │
    │ └────┬─────┘ │ │ └────┬─────┘ │ │ └────┬─────┘ │
    └──────┼───────┘ └──────┼───────┘ └──────┼───────┘
           │                │                │
           ▼                ▼                ▼
    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
    │  MATCHING    │ │  MATCHING    │ │  MATCHING    │
    │  ENGINE      │ │  ENGINE      │ │  ENGINE      │
    │  (Thread 1)  │ │  (Thread 2)  │ │  (Thread 3)  │
    │              │ │              │ │              │
    │ • OrderBook  │ │ • OrderBook  │ │ • OrderBook  │
    │ • HashMap    │ │ • HashMap    │ │ • HashMap    │
    │ • BTreeMap   │ │ • BTreeMap   │ │ • BTreeMap   │
    │ • SINGLE-    │ │ • SINGLE-    │ │ • SINGLE-    │
    │   THREADED   │ │   THREADED   │ │   THREADED   │
    └──────┬───────┘ └──────┬───────┘ └──────┬───────┘
           │                │                │
           ▼                ▼                ▼
    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
    │ Output Queue │ │ Output Queue │ │ Output Queue │
    │   (SPSC)     │ │   (SPSC)     │ │   (SPSC)     │
    └──────┬───────┘ └──────┬───────┘ └──────┬───────┘
           │                │                │
           └────────────────┼────────────────┘
                            │
                            ▼
            ┌───────────────────────────────┐
            │      OUTPUT AGGREGATOR        │
            │   (tokio task, fan-in)        │
            │                               │
            │  • Collect fills from engines │
            │  • Publish market data        │
            │  • Queue for settlement       │
            └───────────────┬───────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
    ┌───────────────┐               ┌───────────────┐
    │ Market Data   │               │ Settlement    │
    │ Publisher     │               │ Batcher       │
    │               │               │               │
    │ • WebSocket   │               │ • Batch fills │
    │   broadcast   │               │ • Sign batch  │
    │ • L2 updates  │               │ • Submit L2   │
    └───────────────┘               └───────────────┘
```

### 3.2 Data Structure Placement

| Component | Data Structure | Concurrency Model | Reason |
|-----------|---------------|-------------------|--------|
| **OrderBook.orders** | `HashMap<OrderId, Order>` | Single-threaded | Hot path, no contention |
| **OrderBook.bids/asks** | `BTreeMap<Price, Level>` | Single-threaded | Ordered, atomic updates |
| **Sessions** | `DashMap<SessionId, State>` | Concurrent | Many I/O threads access |
| **Rate Limits** | `DashMap<UserId, Bucket>` | Concurrent | Per-request checks |
| **User Accounts** | `RwLock<HashMap<..>>` | Read-heavy | Balance checks frequent |
| **Market Config** | `Arc<RwLock<Config>>` | Read-heavy | Rarely changes |
| **Metrics** | `DashMap<MetricKey, Counter>` | Concurrent | Many writers |
| **Order Queues** | `mpsc::channel` | Lock-free | High throughput |

### 3.3 Thread Model

```rust
// Simplified thread architecture

use std::sync::Arc;
use tokio::sync::mpsc;
use dashmap::DashMap;

/// Shared state accessible from all async tasks
struct SharedState {
    sessions: Arc<DashMap<SessionId, SessionState>>,
    rate_limits: Arc<DashMap<UserId, RateLimitBucket>>,
    market_senders: Arc<DashMap<MarketId, mpsc::Sender<EngineCommand>>>,
}

/// Each market has its own dedicated thread
struct MarketThread {
    market_id: MarketId,
    receiver: mpsc::Receiver<EngineCommand>,
    engine: MatchingEngine,
    book: OrderBook,  // Plain HashMap/BTreeMap inside
    output: mpsc::Sender<EngineEvent>,
}

impl MarketThread {
    /// Runs on a dedicated OS thread (not tokio)
    fn run(mut self) {
        while let Some(cmd) = self.receiver.blocking_recv() {
            // All operations are single-threaded here
            let events = match cmd {
                EngineCommand::PlaceOrder(order) => {
                    self.engine.place_order(&mut self.book, order)
                }
                EngineCommand::CancelOrder(id) => {
                    self.engine.cancel_order(&mut self.book, id)
                }
            };
            
            // Send results to output aggregator
            for event in events {
                let _ = self.output.blocking_send(event);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let shared = Arc::new(SharedState::new());
    
    // Spawn dedicated threads for each market
    for market_id in configured_markets() {
        let (cmd_tx, cmd_rx) = mpsc::channel(10_000);
        let (event_tx, event_rx) = mpsc::channel(10_000);
        
        shared.market_senders.insert(market_id, cmd_tx);
        
        // Dedicated OS thread for matching engine
        std::thread::spawn(move || {
            let market_thread = MarketThread::new(market_id, cmd_rx, event_tx);
            market_thread.run();
        });
        
        // Tokio task for collecting output
        tokio::spawn(async move {
            process_engine_events(event_rx).await;
        });
    }
    
    // Run async I/O handlers
    run_websocket_server(shared.clone()).await;
}
```

---

## 4. Where DashMap Is Appropriate

### 4.1 Session Registry

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Session state for a connected client
#[derive(Debug, Clone)]
pub struct SessionState {
    pub user_id: UserId,
    pub connected_at: Instant,
    pub subscriptions: Vec<MarketId>,
    pub last_activity: Instant,
    pub message_tx: broadcast::Sender<ServerMessage>,
}

/// Thread-safe session registry
#[derive(Clone)]
pub struct SessionRegistry {
    sessions: Arc<DashMap<SessionId, SessionState>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }
    
    /// Register a new session (called from any I/O task)
    pub fn register(&self, id: SessionId, state: SessionState) {
        self.sessions.insert(id, state);
    }
    
    /// Remove a session (called on disconnect)
    pub fn unregister(&self, id: &SessionId) -> Option<SessionState> {
        self.sessions.remove(id).map(|(_, v)| v)
    }
    
    /// Get session for message routing
    pub fn get(&self, id: &SessionId) -> Option<dashmap::mapref::one::Ref<SessionId, SessionState>> {
        self.sessions.get(id)
    }
    
    /// Broadcast to all sessions subscribed to a market
    pub fn broadcast_to_market(&self, market_id: MarketId, msg: ServerMessage) {
        // DashMap iteration is fine for this use case
        for entry in self.sessions.iter() {
            if entry.subscriptions.contains(&market_id) {
                let _ = entry.message_tx.send(msg.clone());
            }
        }
    }
    
    /// Cleanup stale sessions (called periodically)
    pub fn cleanup_stale(&self, max_idle: Duration) {
        let now = Instant::now();
        self.sessions.retain(|_, state| {
            now.duration_since(state.last_activity) < max_idle
        });
    }
}
```

### 4.2 Rate Limiter

```rust
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Token bucket rate limiter state
#[derive(Debug, Clone)]
pub struct RateLimitBucket {
    tokens: f64,
    last_update: Instant,
    capacity: f64,
    refill_rate: f64,  // tokens per second
}

impl RateLimitBucket {
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            last_update: Instant::now(),
            capacity,
            refill_rate,
        }
    }
    
    /// Try to consume tokens, returns true if allowed
    pub fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
    
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_update = now;
    }
}

/// Thread-safe rate limiter registry
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<DashMap<UserId, RateLimitBucket>>,
    default_capacity: f64,
    default_rate: f64,
}

impl RateLimiter {
    pub fn new(capacity: f64, rate: f64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            default_capacity: capacity,
            default_rate: rate,
        }
    }
    
    /// Check if request is allowed (called from any I/O task)
    pub fn check(&self, user_id: UserId, tokens: f64) -> bool {
        // Get or create bucket
        let mut bucket = self.buckets
            .entry(user_id)
            .or_insert_with(|| RateLimitBucket::new(
                self.default_capacity,
                self.default_rate,
            ));
        
        bucket.try_consume(tokens)
    }
    
    /// Cleanup idle buckets periodically
    pub fn cleanup(&self, max_idle: Duration) {
        let now = Instant::now();
        self.buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_update) < max_idle
        });
    }
}
```

### 4.3 Metrics Aggregation

```rust
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A single counter metric
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }
    
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Thread-safe metrics registry
#[derive(Clone)]
pub struct MetricsRegistry {
    counters: Arc<DashMap<String, Arc<Counter>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
        }
    }
    
    /// Get or create a counter (called from any thread)
    pub fn counter(&self, name: &str) -> Arc<Counter> {
        self.counters
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Counter::default()))
            .clone()
    }
    
    /// Export all metrics (called by metrics endpoint)
    pub fn export(&self) -> Vec<(String, u64)> {
        self.counters
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().get()))
            .collect()
    }
}

// Usage from matching engine output handler
fn record_fill(metrics: &MetricsRegistry, market_id: MarketId) {
    metrics.counter(&format!("fills_total_{}", market_id)).increment();
    metrics.counter("fills_total").increment();
}
```

### 4.4 Order ID to Channel Router

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Pending order responses waiting for engine confirmation
pub struct PendingOrders {
    pending: Arc<DashMap<OrderId, oneshot::Sender<ExecutionReport>>>,
}

impl PendingOrders {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
        }
    }
    
    /// Register a pending order (called from I/O task)
    pub fn register(&self, order_id: OrderId) -> oneshot::Receiver<ExecutionReport> {
        let (tx, rx) = oneshot::channel();
        self.pending.insert(order_id, tx);
        rx
    }
    
    /// Complete a pending order (called from output aggregator)
    pub fn complete(&self, order_id: OrderId, report: ExecutionReport) {
        if let Some((_, tx)) = self.pending.remove(&order_id) {
            let _ = tx.send(report);
        }
    }
}
```

---

## 5. Migration & Optimization Advice

### 5.1 DO NOT Touch the Core Engine

**Clear recommendation: Leave `orderbook` and `matching` crates as-is.**

Your current performance is excellent:
- Hot-path matching at ~130ns/order is near the theoretical minimum
- The single-threaded design is the reason it's fast
- Any concurrency additions would likely slow it down

### 5.2 Safe Optimizations (Optional)

If you want to squeeze more performance without changing the concurrency model:

#### Option A: Use `hashbrown::HashMap` Instead of `std::collections::HashMap`

```rust
// In crates/orderbook/src/book.rs

// Before:
use std::collections::HashMap;

// After:
use hashbrown::HashMap;
```

**Expected impact:**
- 10-20% faster hash map operations
- Same API, drop-in replacement
- No behavior changes

**Why it's faster:**
- SIMD-accelerated hashing (SwissTable)
- Better cache utilization
- Used by Rust std since 1.36, but standalone version has more features

#### Option B: Pre-allocate Order Book Capacity

```rust
// Already exists in your code, but ensure it's used:
impl OrderBook {
    pub fn with_capacity(market_id: MarketId, capacity: usize) -> Self {
        Self {
            market_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::with_capacity(capacity),  // Pre-allocate!
        }
    }
}

// In engine initialization:
let book = OrderBook::with_capacity(market_id, 100_000);  // Expect 100k orders
```

#### Option C: Use `SmallVec` for Price Level Order Lists

```rust
// In crates/orderbook/src/price_level.rs

use smallvec::SmallVec;

pub struct PriceLevel {
    price: Price,
    // Most levels have few orders; inline storage for common case
    order_ids: SmallVec<[OrderId; 8]>,  // 8 orders inline, heap after
    quantities: SmallVec<[Quantity; 8]>,
    total_quantity: Quantity,
}
```

**Expected impact:**
- Fewer allocations for typical price levels
- Better cache locality
- ~5-10% improvement for insert/remove at level

#### Option D: Arena Allocation for Orders (Advanced)

```rust
// More invasive change - consider carefully

use typed_arena::Arena;

pub struct OrderBook {
    market_id: MarketId,
    bids: BTreeMap<Reverse<Price>, PriceLevel>,
    asks: BTreeMap<Price, PriceLevel>,
    
    // Orders stored in arena (bump allocation)
    order_arena: Arena<Order>,
    // Index from ID to arena pointer
    order_index: HashMap<OrderId, &Order>,
}
```

**Pros:**
- All orders in contiguous memory
- Allocation is O(1) bump pointer
- Better cache locality during iteration

**Cons:**
- Can't deallocate individual orders until arena reset
- More complex lifetime management
- Probably not worth it unless you hit memory allocation in profiling

### 5.3 What NOT to Do

❌ **Don't add `Arc<Mutex<..>>` around the order book**
```rust
// BAD - serializes all access, slower than single-threaded
struct BadEngine {
    book: Arc<Mutex<OrderBook>>,
}
```

❌ **Don't use `DashMap` for order storage**
```rust
// BAD - loses ordering, atomic multi-key updates impossible
struct BadOrderBook {
    orders: DashMap<OrderId, Order>,
}
```

❌ **Don't try to parallelize matching**
```rust
// BAD - breaks price-time priority, non-deterministic
fn bad_parallel_match(orders: &[Order]) {
    orders.par_iter().for_each(|o| engine.process(o));
}
```

❌ **Don't share the order book between threads**
```rust
// BAD - even with locks, this is wrong
struct BadShared {
    book: Arc<RwLock<OrderBook>>,  // Never do this!
}
```

---

## 6. Summary & Recommendations

### 6.1 Key Takeaways

| Aspect | Recommendation |
|--------|----------------|
| **Order Book** | Keep single-threaded, use plain `HashMap`/`BTreeMap` |
| **Matching Engine** | Keep single-threaded, one instance per market |
| **Scaling** | Horizontal: one engine thread per market |
| **Sessions** | Use `DashMap<SessionId, State>` |
| **Rate Limiting** | Use `DashMap<UserId, Bucket>` |
| **Metrics** | Use `DashMap<Key, AtomicCounter>` |
| **Inter-thread Comm** | Use `mpsc` channels (tokio or crossbeam) |
| **I/O** | Use tokio async runtime |

### 6.2 Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                    ASYNC I/O LAYER                          │
│              (tokio, many tasks, DashMap for shared state)  │
└───────────────────────────┬─────────────────────────────────┘
                            │ mpsc channels
                            ▼
┌─────────────────────────────────────────────────────────────┐
│               MATCHING ENGINES (dedicated threads)          │
│      One per market, single-threaded, plain HashMap         │
└───────────────────────────┬─────────────────────────────────┘
                            │ mpsc channels  
                            ▼
┌─────────────────────────────────────────────────────────────┐
│               OUTPUT PROCESSING (tokio tasks)               │
│           Market data, settlement batching                  │
└─────────────────────────────────────────────────────────────┘
```

### 6.3 Next Steps for Stage 4+

1. **Stage 4 (Accounts)**: 
   - Account state can use `RwLock<HashMap<UserId, Account>>` 
   - Read-heavy (balance checks), write-rare (fills)
   - Single lock is fine; accounts are checked before sending to engine

2. **Stage 5 (Fees/Settlement)**: 
   - Settlement batcher runs in its own task
   - Collects fills via channel from engines
   - No shared mutable state with engines

3. **Stage 11 (Gateway)**:
   - This is where `DashMap` shines
   - Sessions, rate limits, pending orders all fit the pattern
   - Use the code sketches from Section 4

### 6.4 Final Verdict

**Your instinct to keep the core engine single-threaded is correct.**

The industry standard for high-performance matching is:
1. Single-threaded engine per instrument
2. Horizontal scaling via multiple engines
3. Concurrency only at the boundaries

Your current ~1-7M ops/sec performance proves the design is sound. Adding concurrency to the hot path would be a step backward.

**Use `DashMap` for:**
- Session management ✅
- Rate limiting ✅
- Metrics ✅
- Caches ✅

**Keep plain `HashMap`/`BTreeMap` for:**
- Order book ✅
- Matching engine state ✅
- Any price-time priority data ✅

---

## Appendix: Quick Reference

### When to Use What

```
Need concurrent access from multiple threads?
├── YES: Is it a simple key-value store?
│   ├── YES: Is contention on specific keys expected?
│   │   ├── YES (hot keys): Use RwLock<HashMap> or shard manually
│   │   └── NO (independent keys): Use DashMap ✅
│   └── NO: Need ordering or complex ops?
│       └── Use Mutex/RwLock around the data structure
└── NO: Single-threaded access?
    └── Use plain HashMap/BTreeMap ✅ (fastest)
```

### DashMap Checklist

Before using `DashMap`, verify:
- [ ] Multiple threads genuinely need concurrent access
- [ ] Operations are mostly on different keys
- [ ] No need for atomic multi-key operations
- [ ] Iteration order doesn't matter
- [ ] Occasional lock contention is acceptable
- [ ] The extra ~30ns latency per operation is fine

If any checkbox fails, consider alternatives.
