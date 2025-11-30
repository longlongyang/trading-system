# On-Chain Order Book & Matching Engine Architecture Document

---

## Related Documents

- **[IMPLEMENTATION_STAGES.md](./IMPLEMENTATION_STAGES.md)** - Stage-by-stage implementation plan
- **[CONCURRENCY_DESIGN.md](./CONCURRENCY_DESIGN.md)** - Concurrency patterns, DashMap analysis, and threading architecture

---

## 1. Restatement of Understanding

### Environment
- **Target Platform**: Ethereum Layer 2 rollup (EVM-compatible)
- **Runtime**: EVM for on-chain execution; Rust for core matching logic
- **Asset Scope**: Spot markets with ERC-20 base/quote token pairs

### Core Objectives
1. Build a high-performance order book and matching engine with Rust at its core
2. Execute trades on an L2 with on-chain settlement guarantees
3. Support standard exchange operations: order placement, cancellation, matching, and settlement
4. Integrate with oracles for price feeds and bridges for cross-chain asset movement
5. Maintain deterministic, auditable state transitions

### Main Constraints

| Constraint | Impact |
|------------|--------|
| **Gas Cost** | L2 is cheaper than L1 but still non-trivial; every SLOAD/SSTORE counts |
| **Latency** | L2 block times (1–2s typical) set floor for finality; off-chain matching can provide sub-second "soft" confirmations |
| **Security** | Must be adversarial-resistant: re-entrancy, front-running/MEV, oracle manipulation |
| **Determinism** | Matching must produce identical results given identical inputs—critical for verifiability and dispute resolution |

---

## 2. Design Options

### Option A: Fully On-Chain Matching (Solidity-Native)

**Description**: All order book logic lives in Solidity contracts. Rust is used only for off-chain simulation, testing, and tooling.

| Pros | Cons |
|------|------|
| Maximum trustlessness—everything verifiable on-chain | High gas cost for complex matching |
| Simpler architecture (single source of truth) | Limited data structures in EVM (no efficient sorted maps) |
| No off-chain operator trust | Throughput limited by block gas limit |
| | MEV exposure on every order |

**When to use**: Low-volume markets where decentralization trumps performance; regulatory environments requiring full on-chain auditability.

---

### Option B: Hybrid Off-Chain Matching + On-Chain Settlement

**Description**: Rust matching engine runs off-chain (operated by a sequencer or operator). It produces signed match results (batches of fills). On-chain contracts verify signatures and settle trades atomically.

| Pros | Cons |
|------|------|
| Rust engine can be highly optimized (µs matching) | Introduces trust in the operator (mitigated by fraud proofs or validity proofs) |
| Gas cost reduced to settlement only | More complex infrastructure |
| Sub-second soft-confirmations | Must handle operator liveness (fallback needed) |
| MEV mitigation via encrypted order flow | |

**When to use**: High-throughput exchanges where UX and performance matter; teams willing to operate infrastructure; can add validity proofs later for trustlessness.

---

### Option C: Rust Core in zkVM / WASM Execution Environment

**Description**: Compile Rust matching engine to WASM or a zkVM (e.g., RISC Zero, SP1). Run it as part of a rollup's state transition function or generate ZK proofs of correct matching.

| Pros | Cons |
|------|------|
| Full trustlessness with Rust performance | zkVM overhead still significant (proving time) |
| Matching logic proven correct | Tooling maturity varies |
| Future-proof for ZK L2s | Deployment complexity |

**When to use**: Building a custom rollup or app-chain; long-term vision for ZK-proven execution; willing to invest in zkVM integration.

---

### **Recommendation: Option B (Hybrid) as Primary Path**

**Rationale**:
1. **Performance**: Rust off-chain engine enables matching in microseconds; critical for competitive trading UX.
2. **Gas Efficiency**: On-chain footprint limited to deposits, withdrawals, and settlement; order placement/cancellation can be off-chain with commitments.
3. **Pragmatic Trust Model**: Initial operator trust is acceptable for spot markets; can upgrade to validity proofs (Option C) later without rewriting the Rust core.
4. **MEV Mitigation**: Operator can implement fair ordering (e.g., batch auctions, encrypted mempools).
5. **Path to Decentralization**: Can introduce a decentralized sequencer set or forced-inclusion mechanism over time.

We will design the Rust engine to be **zkVM-compatible** from the start (no floating point, deterministic execution, serializable state) to preserve the upgrade path to Option C.

---

## 3. High-Level System Architecture (Hybrid Model)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              USER LAYER                                     │
│   Web/Mobile UI  ←→  SDK/API Client  ←→  WebSocket + REST Gateway          │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
┌──────────────────────┐ ┌──────────────────┐ ┌──────────────────────────────┐
│   ORDER GATEWAY      │ │   INDEXER        │ │   RISK / MONITORING          │
│   (Off-chain)        │ │   (Off-chain)    │ │   (Off-chain)                │
│                      │ │                  │ │                              │
│ • Receive orders     │ │ • Index events   │ │ • Real-time P&L              │
│ • Validate signatures│ │ • Build order    │ │ • Position limits            │
│ • Forward to engine  │ │   book snapshots │ │ • Anomaly detection          │
└──────────┬───────────┘ │ • Serve queries  │ └──────────────────────────────┘
           │             └──────────────────┘
           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                     RUST MATCHING ENGINE (Off-chain)                         │
│                                                                              │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌─────────────────┐  │
│  │ OrderBook   │   │ Matching    │   │ State       │   │ Batch           │  │
│  │ Manager     │   │ Core        │   │ Manager     │   │ Assembler       │  │
│  │             │   │             │   │             │   │                 │  │
│  │ Per-market  │   │ Price-time  │   │ Accounts,   │   │ Creates signed  │  │
│  │ order books │   │ priority    │   │ balances,   │   │ settlement      │  │
│  │             │   │ matching    │   │ positions   │   │ batches         │  │
│  └─────────────┘   └─────────────┘   └─────────────┘   └────────┬────────┘  │
│                                                                  │          │
└──────────────────────────────────────────────────────────────────┼──────────┘
                                                                   │
                                      ┌────────────────────────────┘
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                         ON-CHAIN CONTRACTS (L2 EVM)                          │
│                                                                              │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐                 │
│  │   Exchange     │  │    Vault       │  │ OracleAdapter  │                 │
│  │   (Entry)      │  │    (Custody)   │  │                │                 │
│  │                │  │                │  │ • Chainlink    │                 │
│  │ • Settlement   │  │ • Deposits     │  │ • Pyth         │                 │
│  │ • Verification │  │ • Withdrawals  │  │ • Circuit      │                 │
│  │ • Market mgmt  │  │ • Balances     │  │   breakers     │                 │
│  └───────┬────────┘  └───────┬────────┘  └────────────────┘                 │
│          │                   │                                              │
│          └─────────┬─────────┘                                              │
│                    ▼                                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        Markets Registry                              │    │
│  │  • Market configs (base/quote, tick size, fees)                     │    │
│  │  • Market state (paused, active)                                    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                         ETHEREUM L2 ROLLUP                                   │
│                    (Arbitrum / Optimism / Base / zkSync)                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| **Order Gateway** | Receives user orders via API, validates signatures, applies rate limits, forwards to engine |
| **Rust Matching Engine** | Core order book logic, price-time priority matching, generates fills |
| **Batch Assembler** | Groups fills into settlement batches, signs with operator key |
| **Exchange Contract** | Entry point for settlements; verifies batch signatures; calls Vault |
| **Vault Contract** | Custodies ERC-20 tokens; handles deposits/withdrawals; maintains balance ledger |
| **Markets Registry** | Stores market parameters; enforces tick sizes, min order sizes |
| **OracleAdapter** | Wraps oracle calls; provides staleness checks and circuit breakers |
| **Indexer** | Reads on-chain events; reconstructs order book state for queries |
| **Risk/Monitoring** | Tracks exposure, alerts on anomalies |

---

### Typical Flow

#### Deposit → Order → Match → Settlement → Withdrawal

```
1. DEPOSIT
   User calls Vault.deposit(token, amount)
   → Vault transfers ERC-20 from user
   → Emits Deposit(user, token, amount)
   → Indexer picks up event, credits user in off-chain state

2. PLACE ORDER
   User signs order off-chain: {market, side, price, size, nonce, expiry}
   → Gateway validates signature, nonce, balance
   → Engine inserts order into book
   → Emits soft-confirmation to user

3. MATCH
   Engine continuously matches crossing orders
   → Produces fills: (taker_order, maker_order, fill_price, fill_size)
   → Updates off-chain balances

4. SETTLEMENT (batched)
   Batch Assembler groups fills by market
   → Creates settlement payload: {fills[], stateRoot, signature}
   → Submits to Exchange.settleBatch(payload)
   → Exchange verifies operator signature
   → Exchange calls Vault.applyFills(fills[])
   → Vault updates on-chain balances
   → Emits Trade events

5. WITHDRAWAL
   User calls Vault.withdraw(token, amount)
   → Vault checks available balance (not reserved)
   → Transfers ERC-20 to user
   → Emits Withdrawal(user, token, amount)
```

---

## 4. Core Domain Model & Data Structures (Rust)

### Key Types

| Type | Role |
|------|------|
| `MarketId` | `u64` identifier for a trading pair (e.g., ETH/USDC) |
| `OrderId` | `u128` globally unique order identifier (includes market + sequence) |
| `OrderSide` | Enum: `Bid` / `Ask` |
| `OrderType` | Enum: `Limit`, `Market` (with price bound) |  
| `TimeInForce` | Enum: `GoodTilCancel`, `PostOnly`, `ImmediateOrCancel` (IOC), `FillOrKill` (FOK) |
| `Price` | Fixed-point `u128` (e.g., 18 decimals) to avoid floating point |
| `Quantity` | Fixed-point `u128` for base asset amount |

### Core Structs

| Struct | Fields & Purpose |
|--------|------------------|
| `Order` | `id`, `market_id`, `owner`, `side`, `order_type`, `time_in_force`, `price`, `price_bound` (for market orders), `original_qty`, `remaining_qty`, `timestamp`, `nonce`, `expiry` |
| `PriceLevel` | `price`, `total_qty`, `orders: VecDeque<OrderId>` — FIFO queue at a single price |
| `OrderBook` | `market_id`, `bids: BTreeMap<Price, PriceLevel>`, `asks: BTreeMap<Price, PriceLevel>`, `orders: HashMap<OrderId, Order>` |
| `Trade` | `id`, `market_id`, `taker_order_id`, `maker_order_id`, `price`, `quantity`, `taker_side`, `timestamp` |
| `Account` | `address`, `balances: HashMap<TokenId, Balance>` |
| `Balance` | `available: u128`, `reserved: u128` (reserved = locked in open orders) |
| `FeeConfig` | `maker_fee_bps: i32`, `taker_fee_bps: i32`, `fee_recipient: Address` — per-market fee settings |
| `AccountStats` | `address`, `volume_30d: u128`, `last_updated: u64` — extensible for future volume tiers |

### Order Book Data Structure Choices

```
OrderBook
├── bids: BTreeMap<Reverse<Price>, PriceLevel>   // Descending order (best bid first)
├── asks: BTreeMap<Price, PriceLevel>            // Ascending order (best ask first)
└── orders: HashMap<OrderId, Order>              // O(1) lookup by ID

PriceLevel
├── price: Price
├── total_qty: Quantity                          // Sum of all orders at this level
└── queue: VecDeque<OrderId>                     // FIFO for time priority
```

**Why these choices**:
- `BTreeMap` gives O(log n) insert/remove and O(1) access to best price via `.first_key_value()`
- `VecDeque` provides O(1) push_back (new orders) and pop_front (matching)
- `HashMap<OrderId, Order>` enables O(1) cancel by ID
- All structures use integer arithmetic (no floats) for determinism

### Serialization for L2

- Use **borsh** or **bincode** for compact, deterministic serialization
- State roots computed via Merkle tree over serialized order book + accounts
- Designed to be **zkVM-friendly**: no heap allocations during hot path, no floating point

---

## 5. On-Chain Storage & Contract Layout (EVM)

### Storage Layout Concepts

```solidity
// Markets Registry
mapping(uint64 => Market) public markets;
// marketId => Market config

struct Market {
    address baseToken;
    address quoteToken;
    uint128 tickSize;        // Minimum price increment
    uint128 minOrderSize;    // Minimum base quantity
    int128 makerFeeBps;      // Maker fee in basis points (negative = rebate, e.g., -2 = -0.02%)
    int128 takerFeeBps;      // Taker fee in basis points (e.g., 10 = 0.10%)
    bool paused;
}

// Fee Vault
address public feeVault;              // Protocol treasury receiving fees
mapping(address => uint256) public collectedFees;  // token => accumulated fees

// Vault - Balance Ledger
mapping(address => mapping(address => uint256)) public balances;
// user => token => available balance

mapping(address => mapping(address => uint256)) public reserved;
// user => token => reserved in open orders (optional, can be off-chain only)

// Operator Registry (for hybrid model)
mapping(address => bool) public operators;
// operator address => is authorized

// Nonces (replay protection)
mapping(address => uint256) public nonces;
// user => next valid nonce
```

### Contract Responsibilities

| Contract | Responsibility |
|----------|----------------|
| **Exchange** | Settlement entry point; batch verification; market creation; fee collection |
| **Vault** | Token custody; deposit/withdraw; balance updates during settlement |
| **Markets** | (Can be part of Exchange) Market configuration and parameter updates |
| **OracleAdapter** | (Optional for v1) Unified interface to price feeds; staleness checks; circuit breakers; multi-provider abstraction |
| **ProxyAdmin** | Manages proxy upgrades; owned by timelock + multisig |
| **Timelock** | Enforces delay on upgrades/config changes; transparent announcement period |

### Upgradeability Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     GOVERNANCE LAYER                            │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐ │
│  │  Multisig   │───▶│  Timelock   │───▶│    ProxyAdmin       │ │
│  │  (N-of-M)   │    │  (48h delay)│    │                     │ │
│  └─────────────┘    └─────────────┘    └──────────┬──────────┘ │
└───────────────────────────────────────────────────┼─────────────┘
                                                    │
                    ┌───────────────────────────────┼───────────────┐
                    │           PROXY LAYER         │               │
                    │                               ▼               │
                    │  ┌─────────────┐    ┌─────────────────────┐  │
                    │  │ Exchange    │    │ Implementation v1   │  │
                    │  │ Proxy       │───▶│ (upgradeable)       │  │
                    │  └─────────────┘    └─────────────────────┘  │
                    │  ┌─────────────┐    ┌─────────────────────┐  │
                    │  │ Vault       │    │ Implementation v1   │  │
                    │  │ Proxy       │───▶│ (upgradeable)       │  │
                    │  └─────────────┘    └─────────────────────┘  │
                    └──────────────────────────────────────────────┘
                                                    │
                    ┌───────────────────────────────┼───────────────┐
                    │        IMMUTABLE LAYER        │               │
                    │  ┌─────────────┐    ┌─────────────────────┐  │
                    │  │ Math/Utils  │    │ External Tokens     │  │
                    │  │ Libraries   │    │ (USDC, WETH, etc.)  │  │
                    │  └─────────────┘    └─────────────────────┘  │
                    └──────────────────────────────────────────────┘
```

**Upgrade Controls**:
- Config changes (fees, limits): Timelock with shorter delay (e.g., 24h)
- Implementation upgrades: Timelock with longer delay (e.g., 48-72h)
- Emergency pause: Multisig can pause immediately (no timelock)
- Renounce upgradeability: Supported per-contract for gradual hardening

### Minimizing Storage Operations

1. **Batch settlements**: Amortize SSTORE cost across multiple fills
2. **Delta encoding**: Only store balance changes, not full balances per fill
3. **Events over storage**: Use events for historical data; indexers rebuild state
4. **Separate markets**: Each market's storage is independent → no cross-market contention
5. **Reserve off-chain**: Track reserved balances off-chain; only settle net changes on-chain

### Gas Optimization Patterns

| Technique | Savings |
|-----------|---------|
| Pack structs into single slots | Reduce SSTORE count |
| Use mappings over arrays for O(1) access | Avoid iteration |
| Emit events instead of storing history | 375 gas vs 20,000 gas |
| Batch fills into single transaction | Amortize base tx cost |

---

## 6. State Transition & Transaction Model

### Core Actions

#### `createMarket`

| Aspect | Details |
|--------|---------|
| **Caller** | Admin only |
| **Checks** | Market doesn't exist; valid token addresses; tick size > 0 |
| **State Written** | `markets[marketId] = Market{...}` |
| **Events** | `MarketCreated(marketId, baseToken, quoteToken, ...)` |

#### `placeOrder` (off-chain in hybrid model)

| Aspect | Details |
|--------|---------|
| **Input** | Signed order: `{market, side, price, qty, nonce, expiry, signature}` |
| **Stateless Checks** | Signature valid; expiry not passed; price aligned to tick |
| **Stateful Checks** | Nonce valid; sufficient available balance |
| **State Written** | Insert into OrderBook; reserve balance |
| **Events** | `OrderPlaced(orderId, ...)` (soft, off-chain) |

#### `cancelOrder` (off-chain)

| Aspect | Details |
|--------|---------|
| **Input** | Signed cancel: `{orderId, nonce, signature}` |
| **Checks** | Order exists; caller is owner |
| **State Written** | Remove from OrderBook; unreserve balance |
| **Events** | `OrderCancelled(orderId)` (soft) |

#### `settleBatch` (on-chain)

| Aspect | Details |
|--------|---------|
| **Input** | `{fills[], operatorSignature, batchId}` |
| **Stateless Checks** | Signature from authorized operator |
| **Stateful Checks** | BatchId not replayed; fills reference valid markets |
| **State Written** | Update `balances` in Vault for each fill; increment batch nonce |
| **Events** | `Trade(marketId, maker, taker, price, qty, ...)` per fill; `BatchSettled(batchId)` |

#### `updateParams`

| Aspect | Details |
|--------|---------|
| **Caller** | Admin (with timelock for sensitive params) |
| **Checks** | Within allowed bounds |
| **State Written** | Update market config or global params |
| **Events** | `ParamsUpdated(...)` |

### Determinism & Time Priority

1. **Sequencer ordering**: Off-chain engine processes orders in receipt order (FIFO)
2. **Timestamp assignment**: Engine assigns monotonic timestamps
3. **Batch commitment**: Operator commits to batch contents before submission
4. **Forced inclusion**: Users can submit orders directly on-chain as fallback (higher latency)
5. **MEV mitigation**: Consider batch auctions (discrete time intervals) or encrypted order flow

---

## 7. Risk, Oracles, and External Data

### Balance & Risk Checks

| Check | When | Implementation |
|-------|------|----------------|
| **Sufficient balance** | Order placement | `available >= order_value + fees` |
| **Position limits** | Order placement | Optional: max notional per user per market |
| **Rate limits** | Order placement | Max orders/second per user |
| **Self-trade prevention** | Matching | Skip fills where maker == taker |

### Oracle Integration

**V1 Spot Markets**: Oracles are **optional** — core trading relies purely on order book matching between users.

**Optional Oracle Uses (v1)**:
- Circuit breakers (pause if book price diverges significantly from external price)
- Monitoring and analytics dashboards
- Cross-margin collateral valuation (if enabled)

**Required for Future Derivatives**:
- Mark price computation
- Liquidation thresholds and health checks
- Collateral valuation across markets
- Funding rate calculations

**Oracle Sources** (abstracted behind `OracleAdapter` interface):
- Primary: Chainlink or Pyth (depending on L2 availability)
- Secondary: DEX TWAPs (Uniswap-style) as supplementary/backup signals
- Multi-feed support: median of sources configurable via governance

**Safeguards**:

```
OracleAdapter.getPrice(marketId):
  1. Read price from primary oracle
  2. Check: block.timestamp - lastUpdate <= MAX_STALENESS (e.g., 60s)
  3. Check: price within DEVIATION_THRESHOLD of secondary oracle (if available)
  4. Check: price within reasonable bounds (not zero, not extreme)
  5. If any check fails → revert or return "INVALID"
```

### Circuit Breakers

| Trigger | Action |
|---------|--------|
| Oracle stale > threshold | Pause market |
| Price deviation > X% in Y blocks | Pause market |
| Large position concentration | Alert + optional limit new orders |
| Operator unresponsive | Enable forced on-chain order submission |

**Recovery**: Admin can unpause after manual review; or automatic after conditions normalize.

---

## 8. Performance, Gas, and Scalability Strategy

### Identified Bottlenecks

| Bottleneck | Mitigation |
|------------|------------|
| On-chain storage writes | Batch settlements; minimal on-chain state |
| Order book traversal | BTreeMap for O(log n) operations |
| Matching hot loop | Rust, no allocations in hot path, cache-friendly |
| Sequencer single point | Decentralized sequencer set (future); forced inclusion fallback |

### Target Throughput

| Metric | Target |
|--------|--------|
| Off-chain matching | 100,000+ orders/second |
| On-chain settlements | 50–200 fills per batch |
| Settlement latency | Every block or every N seconds (configurable) |

### Gas Estimates (L2, approximate)

| Operation | Gas (L2) | Notes |
|-----------|----------|-------|
| Deposit | ~65,000 | ERC-20 transfer + storage write |
| Withdrawal | ~50,000 | Storage read + ERC-20 transfer |
| Settle batch (50 fills) | ~500,000 | ~10k per fill (storage updates, events) |
| Place order (on-chain fallback) | ~80,000 | Rarely used |

### Scalability Strategies

1. **Market sharding**: Each market is independent; can parallelize settlement
2. **Compressed calldata**: Use tight encoding for fills (no ABIv2 overhead)
3. **Off-chain indexing**: Full order book state reconstructed from events; no on-chain queries
4. **Lazy settlement**: Settle only when user wants to withdraw or periodically

---

## 9. Testing & Benchmarking Plan

### Unit Tests (Rust Engine)

| Area | Tests |
|------|-------|
| Order insertion | Insert at various prices; verify tree structure |
| Matching | Price-time priority; partial fills; self-trade prevention |
| Cancellation | Cancel existing order; cancel non-existent (error) |
| Balance management | Reserve on place; unreserve on cancel; settle on fill |
| Edge cases | Zero quantity; max price; overflow protection |

### Integration / E2E Tests

| Scenario | Coverage |
|----------|----------|
| Deposit → Trade → Withdraw | Full happy path |
| Settlement batch verification | On-chain signature checks |
| Oracle integration | Stale price rejection; circuit breaker trigger |
| Forced inclusion | User bypasses operator |
| Operator rotation | New operator key settlement |

### Property / Fuzz Tests

| Invariant | Description |
|-----------|-------------|
| Conservation | Sum of all balances == total deposits - total withdrawals |
| No negative balances | `available >= 0`, `reserved >= 0` always |
| Order book consistency | Orders in book == orders in HashMap |
| Determinism | Same input sequence → same output (run twice, compare) |

**Fuzzing**: Use `proptest` or `arbitrary` to generate random order sequences and verify invariants hold.

### Benchmarks

| Scenario | Metrics |
|----------|---------|
| Insert 1M orders | Time, memory |
| Match 100K orders | Time, fills/second |
| Cancel 50K orders | Time |
| Mixed workload (realistic) | Throughput, p50/p99 latency |

**Tools**: `criterion` for Rust microbenchmarks; custom harness for system benchmarks.

---

## Summary

This architecture delivers a **high-performance hybrid order book** that combines:
- **Rust off-chain engine** for microsecond matching and complex order types
- **Minimal on-chain footprint** for gas efficiency (settlement and custody only)
- **Security via batched, signed settlements** with operator accountability
- **Upgrade path to zkVM** for trustless verification in the future
