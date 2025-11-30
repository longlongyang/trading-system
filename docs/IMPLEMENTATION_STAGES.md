# Implementation Stages

This document outlines the iterative implementation plan for the on-chain/off-chain hybrid order book exchange. Each stage has minimal scope, concrete deliverables, and must compile with passing tests before moving to the next stage.

---

## Overview

```
Stage 1: Core Types & Primitives (Rust)
    ↓
Stage 2: Order Book Data Structures (Rust)
    ↓
Stage 3: Matching Engine Core (Rust)
    ↓
Stage 4: Account & Balance Management (Rust)
    ↓
Stage 5: Fee Calculation & Trade Settlement (Rust)
    ↓
Stage 6: Batch Assembly & Serialization (Rust)
    ↓
Stage 7: Smart Contracts - Vault (Solidity)
    ↓
Stage 8: Smart Contracts - Exchange & Settlement (Solidity)
    ↓
Stage 9: Smart Contracts - Markets Registry (Solidity)
    ↓
Stage 10: Integration - Rust ↔ Contracts (E2E)
    ↓
Stage 11: Order Gateway & API (Rust)
    ↓
Stage 12: Indexer & Event Processing
    ↓
Stage 13: Forced Inclusion & Escape Hatches
    ↓
Stage 14: Monitoring, Circuit Breakers & Hardening
```

---

## Stage 1: Core Types & Primitives (Rust) ✅ COMPLETED

**Goal**: Define foundational types used throughout the system. No logic, just data definitions.

**Status**: Completed on initial implementation. All types defined with full test coverage.

### Deliverables

| Deliverable | Description | Status |
|-------------|-------------|--------|
| `crates/types/src/lib.rs` | Crate root, re-exports | ✅ |
| `crates/types/src/ids.rs` | `MarketId`, `OrderId`, `Address` | ✅ |
| `crates/types/src/primitives.rs` | `Price`, `Quantity`, `Timestamp`, `Nonce` (fixed-point u128) | ✅ |
| `crates/types/src/order.rs` | `OrderSide`, `OrderType`, `TimeInForce`, `OrderStatus`, `Order` struct | ✅ |
| `crates/types/src/trade.rs` | `Trade`, `Fill`, `TradeId`, `FillRole` structs | ✅ |
| `crates/types/src/market.rs` | `MarketConfig`, `MarketStatus`, `FeeConfig` structs | ✅ |
| `crates/types/src/account.rs` | `Balance`, `Account`, `AccountError` structs | ✅ |
| `crates/types/src/error.rs` | `EngineError` enum with all error variants | ✅ |
| `Cargo.toml` | Workspace setup with `types` crate | ✅ |
| `crates/types/DESIGN_NOTES.md` | Design decisions documentation | ✅ |

### Key Design Decisions

1. **Fixed-point arithmetic**: All numeric types use `u128` with 18 decimal places (matching ERC-20 standard)
2. **Serialization**: `borsh` for internal state (deterministic, zkVM-friendly), `serde` for external APIs
3. **OrderId encoding**: Upper 64 bits = MarketId, lower 64 bits = sequence number
4. **Address**: 20-byte Ethereum address with hex parsing and shortened display format
5. **Checked arithmetic**: All operations return `Option<T>` to make overflow handling explicit

### Tests (149 unit tests + 9 doc tests)

- ✅ Unit tests for type conversions
- ✅ Serialization round-trip tests (borsh)
- ✅ OrderId encoding/decoding (market + sequence)
- ✅ Property-based tests with proptest
- ✅ Doc tests for all public examples

### Acceptance Criteria ✅

```bash
cargo build --workspace  # ✅ Passes
cargo test --workspace   # ✅ 149 tests + 9 doc tests pass
```

---

## Stage 2: Order Book Data Structures (Rust) ✅ COMPLETED

**Goal**: Implement the order book structure without matching logic, with high performance in mind.

**Status**: Completed. Full implementation with 73 unit tests + 6 doc tests.

### Deliverables

| Deliverable | Description | Status |
|-------------|-------------|--------|
| `crates/orderbook/src/lib.rs` | Crate root with module declarations and re-exports | ✅ |
| `crates/orderbook/src/price_level.rs` | `PriceLevel` with FIFO queue (`VecDeque<OrderId>`) | ✅ |
| `crates/orderbook/src/book.rs` | `OrderBook` with `BTreeMap` for bids/asks, `HashMap` for orders | ✅ |
| `crates/orderbook/src/ops.rs` | `insert_order`, `remove_order`, `update_order_quantity`, `cancel_order` | ✅ |
| `crates/orderbook/src/snapshot.rs` | `OrderBookSnapshot`, `PriceLevelSnapshot`, `L2BookSummary` | ✅ |

### Key Design Decisions

1. **Price ordering**: Bids use `Reverse<Price>` in BTreeMap for descending order (best bid first), asks use natural Price ordering (best ask first)
2. **FIFO at same price**: VecDeque maintains insertion order within each price level
3. **Dual storage**: Orders stored both in price levels (for matching) and HashMap (for O(1) lookup by ID)
4. **Snapshot support**: Full state serialization via borsh and serde_json for checkpointing and market data
5. **L2 aggregation**: L2BookSummary provides price-level aggregated view for market data feeds

### Tests (73 unit tests + 6 doc tests)

- ✅ Insert orders at various prices, verify book structure
- ✅ Remove orders, verify cleanup of empty price levels  
- ✅ Best bid/ask retrieval with price ordering
- ✅ Snapshot serialization round-trip (borsh and JSON)
- ✅ L2 summary aggregation with depth limits
- ✅ Edge cases: empty book, single order, same price multiple orders
- ✅ FIFO order maintenance within price levels
- ✅ Update quantity with status tracking (PartiallyFilled, Filled)

### Acceptance Criteria ✅

```bash
cargo test -p orderbook  # ✅ 73 unit tests + 6 doc tests pass
```

---

## Stage 3: Matching Engine Core (Rust) ✅ COMPLETED

**Goal**: Implement price-time priority matching algorithm with support for all time-in-force policies and market orders.

**Status**: Completed. Full implementation with 42 unit tests + 3 doc tests.

### Deliverables

| Deliverable | Description | Status |
|-------------|-------------|--------|
| `crates/matching/Cargo.toml` | Crate configuration, depends on `types` and `orderbook` | ✅ |
| `crates/matching/src/lib.rs` | Crate root with module declarations and re-exports | ✅ |
| `crates/matching/src/engine.rs` | `MatchingEngine` struct with `place_order` and `cancel_order` | ✅ |
| `crates/matching/src/matcher.rs` | Core matching logic: `match_order`, `would_cross`, `available_quantity` | ✅ |
| `crates/matching/src/result.rs` | Result types: `ExecutionReport`, `MatchFill`, `MatchOutcome`, `CancelResult` | ✅ |
| `crates/matching/src/error.rs` | `MatchingError` enum with all error variants | ✅ |

### Key Types

#### `MatchingEngine`
The main entry point for order processing. Stateless design that operates on external `OrderBook` references.

```rust
impl MatchingEngine {
    pub fn new() -> Self;
    pub fn place_order(&self, book: &mut OrderBook, order: Order) -> MatchResult;
    pub fn cancel_order(&self, book: &mut OrderBook, order_id: OrderId) -> CancelResult;
}
```

#### `ExecutionReport`
Complete report of an order placement including fills, outcome, and quantities.

```rust
pub struct ExecutionReport {
    pub order: Order,
    pub fills: Vec<MatchFill>,
    pub filled_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub outcome: MatchOutcome,
    pub timestamp: Timestamp,
}
```

#### `MatchFill`
A single fill generated when two orders match. Lightweight structure without fee information (fees are Stage 5).

```rust
pub struct MatchFill {
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub maker: Address,
    pub taker: Address,
    pub price: Price,        // Always the maker's price
    pub quantity: Quantity,
    pub taker_side: OrderSide,
    pub timestamp: Timestamp,
}
```

#### `MatchOutcome`
Enum describing what happened to an order.

```rust
pub enum MatchOutcome {
    Filled,                    // Completely filled
    PartiallyFilledResting,    // Partially filled, remainder resting (GTC)
    PartiallyFilledCancelled,  // Partially filled, remainder cancelled (IOC)
    Resting,                   // Resting with no fills
    Cancelled,                 // Cancelled with no fills (IOC no cross)
    RejectedPostOnly,          // PostOnly rejected (would cross)
    RejectedFillOrKill,        // FOK rejected (insufficient liquidity)
}
```

### Key Design Decisions

1. **Stateless Engine**: `MatchingEngine` takes `&mut OrderBook` references rather than owning the book, allowing flexible book management
2. **Lightweight Fills**: `MatchFill` doesn't include fee information - fees are calculated during settlement (Stage 5)
3. **Price-Time Priority**: Implemented via BTreeMap ordering (price) and VecDeque FIFO (time)
4. **Self-Trade Prevention**: Orders from the same owner are skipped during matching (not errored)
5. **Market Orders as IOC**: Market orders use IOC semantics - fill what's available, cancel rest

### Time-in-Force Behaviors

| TIF | Behavior |
|-----|----------|
| **GTC** | Match as much as possible, rest remainder in book |
| **IOC** | Match as much as possible, cancel remainder immediately |
| **FOK** | Fill entirely or reject entirely (no partial fills) |
| **PostOnly** | Rest in book only; reject if would immediately match |

### Tests (42 unit tests + 3 doc tests)

**GTC (Good-til-Cancelled)**
- ✅ Resting order with no match
- ✅ Full fill single order
- ✅ Partial fill with remainder resting
- ✅ Multi-level fill (crosses multiple prices)
- ✅ Non-crossing order (rests in book)

**IOC (Immediate-or-Cancel)**
- ✅ Full fill IOC
- ✅ Partial fill with remainder cancelled
- ✅ No fill cancelled (no cross)

**FOK (Fill-or-Kill)**
- ✅ Successful full fill
- ✅ Rejected due to insufficient liquidity
- ✅ Rejected when price doesn't cross

**PostOnly**
- ✅ Successfully rests in book (no cross)
- ✅ Rejected when would cross spread

**Market Orders**
- ✅ Full fill across multiple levels
- ✅ Partial fill (insufficient liquidity)
- ✅ No liquidity (cancelled)

**Self-Trade Prevention**
- ✅ Skips own orders during matching
- ✅ Matches other owners' orders correctly

**Error Cases**
- ✅ Wrong market rejected
- ✅ Duplicate order ID rejected

**Matching Logic**
- ✅ `would_cross` detection
- ✅ `available_quantity` calculation
- ✅ Single and multi-level matching
- ✅ Correct price (maker's price)

### Acceptance Criteria ✅

```bash
cargo test -p matching  # ✅ 42 unit tests + 3 doc tests pass
cargo test --workspace  # ✅ All 264 unit tests + 18 doc tests pass
```

---

## Stage 4: Account & Balance Management (Rust)

**Goal**: Track user balances with available/reserved separation.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/state/src/lib.rs` | Crate root |
| `crates/state/src/accounts.rs` | `AccountManager` with balance operations |
| `crates/state/src/balance_ops.rs` | `credit`, `debit`, `reserve`, `release`, `transfer` |
| `crates/state/src/validation.rs` | Balance sufficiency checks |

### Tests

- Credit/debit operations
- Reserve balance on order placement
- Release balance on order cancel
- Transfer between accounts (settlement)
- Insufficient balance errors
- Overflow protection

### Invariant Tests

- `available + reserved == total` always
- No negative balances
- Sum of all balances == sum of all deposits

### Acceptance Criteria

```bash
cargo test -p state
# All tests pass, invariants hold
```

---

## Stage 5: Fee Calculation & Trade Settlement (Rust)

**Goal**: Calculate maker/taker fees and settle trades in state.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/state/src/fees.rs` | `FeeCalculator` with maker/taker fee logic |
| `crates/state/src/settlement.rs` | `settle_fill` function: transfer assets + deduct fees |
| `crates/state/src/fee_vault.rs` | Track collected fees per token |

### Tests

- Maker rebate calculation (-0.02%)
- Taker fee calculation (+0.10%)
- Settlement: base/quote asset transfers
- Fee collection in quote asset
- Zero-fee edge case
- Rounding behavior (always round in protocol's favor)

### Acceptance Criteria

```bash
cargo test -p state
# Fee and settlement tests pass
```

---

## Stage 6: Batch Assembly & Serialization (Rust)

**Goal**: Create settlement batches for on-chain submission.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/batch/src/lib.rs` | Crate root |
| `crates/batch/src/batch.rs` | `SettlementBatch` struct |
| `crates/batch/src/assembler.rs` | `BatchAssembler`: collects fills, creates batches |
| `crates/batch/src/encoding.rs` | ABI-compatible encoding for Solidity |
| `crates/batch/src/signing.rs` | Operator signature (secp256k1 / EIP-712) |
| `crates/batch/src/state_root.rs` | Compute Merkle root of state |

### Tests

- Batch creation with multiple fills
- ABI encoding matches Solidity expectations
- Signature verification
- State root computation
- Batch ID uniqueness

### Acceptance Criteria

```bash
cargo test -p batch
# Encoding tests pass
```

---

## Stage 7: Smart Contracts - Vault (Solidity)

**Goal**: Implement token custody with deposit/withdraw.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `contracts/src/Vault.sol` | Core vault contract |
| `contracts/src/interfaces/IVault.sol` | Vault interface |
| `contracts/test/Vault.t.sol` | Foundry tests |
| `contracts/script/DeployVault.s.sol` | Deployment script |

### Vault Functions

```solidity
function deposit(address token, uint256 amount) external;
function withdraw(address token, uint256 amount) external;
function balanceOf(address user, address token) external view returns (uint256);
// Internal: called by Exchange
function _applyBalanceDeltas(BalanceDelta[] calldata deltas) internal;
```

### Tests

- Deposit ERC-20, verify balance
- Withdraw, verify transfer
- Insufficient balance revert
- Reentrancy protection
- Events emitted correctly

### Acceptance Criteria

```bash
cd contracts && forge test --match-contract VaultTest
# All tests pass
```

---

## Stage 8: Smart Contracts - Exchange & Settlement (Solidity)

**Goal**: Implement batch settlement with operator signature verification.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `contracts/src/Exchange.sol` | Main exchange contract |
| `contracts/src/interfaces/IExchange.sol` | Exchange interface |
| `contracts/src/libs/BatchDecoder.sol` | Decode settlement batch calldata |
| `contracts/src/libs/SignatureVerifier.sol` | EIP-712 signature verification |
| `contracts/test/Exchange.t.sol` | Foundry tests |

### Exchange Functions

```solidity
function settleBatch(bytes calldata batchData, bytes calldata signature) external;
function addOperator(address operator) external onlyAdmin;
function removeOperator(address operator) external onlyAdmin;
function pause() external onlyAdmin;
function unpause() external onlyAdmin;
```

### Tests

- Settle batch with valid operator signature
- Reject invalid signature
- Reject replayed batch (same batchId)
- Reject when paused
- Balance updates correct after settlement
- Trade events emitted

### Acceptance Criteria

```bash
cd contracts && forge test --match-contract ExchangeTest
# All tests pass
```

---

## Stage 9: Smart Contracts - Markets Registry (Solidity)

**Goal**: Store market configurations on-chain.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `contracts/src/Markets.sol` | Markets registry contract |
| `contracts/src/interfaces/IMarkets.sol` | Markets interface |
| `contracts/test/Markets.t.sol` | Foundry tests |

### Markets Functions

```solidity
function createMarket(
    uint64 marketId,
    address baseToken,
    address quoteToken,
    uint128 tickSize,
    uint128 minOrderSize,
    int128 makerFeeBps,
    int128 takerFeeBps
) external onlyAdmin;

function updateMarketFees(uint64 marketId, int128 makerFeeBps, int128 takerFeeBps) external onlyAdmin;
function pauseMarket(uint64 marketId) external onlyAdmin;
function unpauseMarket(uint64 marketId) external onlyAdmin;
function getMarket(uint64 marketId) external view returns (Market memory);
```

### Tests

- Create market, verify storage
- Update fees
- Pause/unpause
- Reject duplicate market creation
- Reject invalid parameters (tick size 0, etc.)

### Acceptance Criteria

```bash
cd contracts && forge test --match-contract MarketsTest
# All tests pass
```

---

## Stage 10: Integration - Rust ↔ Contracts (E2E)

**Goal**: End-to-end flow from Rust engine to on-chain settlement.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/integration/src/lib.rs` | Integration crate |
| `crates/integration/src/chain.rs` | Chain interaction (ethers-rs / alloy) |
| `crates/integration/src/submitter.rs` | Batch submission to Exchange contract |
| `crates/integration/tests/e2e.rs` | E2E test with local Anvil node |
| `scripts/e2e_test.sh` | Script to run full E2E flow |

### E2E Test Scenario

1. Deploy contracts to Anvil
2. Create market (ETH/USDC)
3. User A deposits USDC, User B deposits WETH
4. User A places buy order (off-chain in Rust)
5. User B places sell order (off-chain in Rust)
6. Matching engine produces fill
7. Batch assembler creates settlement batch
8. Submit batch to Exchange contract
9. Verify on-chain balances updated correctly
10. User A withdraws WETH, User B withdraws USDC

### Acceptance Criteria

```bash
./scripts/e2e_test.sh
# Full flow completes successfully
```

---

## Stage 11: Order Gateway & API (Rust)

**Goal**: HTTP/WebSocket API for order submission.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/gateway/src/lib.rs` | Gateway crate |
| `crates/gateway/src/server.rs` | Axum/Actix HTTP server |
| `crates/gateway/src/ws.rs` | WebSocket handler for streaming |
| `crates/gateway/src/handlers/orders.rs` | Place/cancel order handlers |
| `crates/gateway/src/handlers/markets.rs` | Market data handlers |
| `crates/gateway/src/auth.rs` | Signature verification (EIP-712) |
| `crates/gateway/src/rate_limit.rs` | Per-user rate limiting |
| `docs/API.md` | API documentation |

### API Endpoints

```
POST   /v1/orders              # Place order
DELETE /v1/orders/{orderId}    # Cancel order
GET    /v1/orders              # List user's open orders
GET    /v1/markets             # List markets
GET    /v1/markets/{id}/book   # Order book snapshot
GET    /v1/account/balances    # User balances
WS     /v1/ws                  # Real-time updates
```

### Tests

- Place order via API, verify in engine
- Cancel order via API
- Invalid signature rejected
- Rate limit enforcement
- WebSocket order updates

### Acceptance Criteria

```bash
cargo test -p gateway
# API tests pass
```

---

## Stage 12: Indexer & Event Processing

**Goal**: Index on-chain events to reconstruct state.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/indexer/src/lib.rs` | Indexer crate |
| `crates/indexer/src/listener.rs` | Event listener (WebSocket/polling) |
| `crates/indexer/src/processor.rs` | Event processor |
| `crates/indexer/src/store.rs` | Indexed data store (in-memory + optional DB) |
| `crates/indexer/src/sync.rs` | State synchronization on startup |

### Indexed Events

- `Deposit(user, token, amount)`
- `Withdrawal(user, token, amount)`
- `Trade(marketId, maker, taker, price, quantity, ...)`
- `BatchSettled(batchId)`
- `MarketCreated(marketId, ...)`

### Tests

- Process deposit event, update state
- Process trade event, update balances
- Startup sync from block 0
- Handle reorgs (if applicable)

### Acceptance Criteria

```bash
cargo test -p indexer
# Indexer tests pass
```

---

## Stage 13: Forced Inclusion & Escape Hatches

**Goal**: Allow users to bypass operator if censored.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `contracts/src/ForcedInclusion.sol` | Forced order submission |
| `contracts/src/interfaces/IForcedInclusion.sol` | Interface |
| `contracts/test/ForcedInclusion.t.sol` | Tests |
| `crates/matching/src/forced.rs` | Rust handling of forced orders |

### Forced Inclusion Functions

```solidity
// User submits order on-chain if operator ignores them
function forceOrder(
    uint64 marketId,
    bool isBuy,
    uint128 price,
    uint128 quantity,
    uint256 nonce
) external;

// User can always withdraw, even if operator is down
function forceWithdraw(address token, uint256 amount) external;

// Operator must process forced orders within N blocks or user can self-settle
function processForcedOrder(uint256 forcedOrderId) external; // operator
function claimForcedOrder(uint256 forcedOrderId) external;   // user, after timeout
```

### Tests

- User forces order, operator processes it
- User forces order, operator ignores, user claims after timeout
- Force withdraw always succeeds
- Cannot force order with insufficient balance

### Acceptance Criteria

```bash
cd contracts && forge test --match-contract ForcedInclusionTest
# All tests pass
```

---

## Stage 14: Monitoring, Circuit Breakers & Hardening

**Goal**: Production readiness with safety mechanisms.

### Deliverables

| Deliverable | Description |
|-------------|-------------|
| `crates/monitoring/src/lib.rs` | Monitoring crate |
| `crates/monitoring/src/metrics.rs` | Prometheus metrics |
| `crates/monitoring/src/alerts.rs` | Alert conditions |
| `contracts/src/CircuitBreaker.sol` | Circuit breaker logic |
| `docs/RUNBOOK.md` | Operations runbook |
| `docs/SECURITY.md` | Security considerations |

### Metrics

- Orders per second (by market)
- Matching latency (p50, p99)
- Settlement batch size
- Gas costs per settlement
- User balance totals
- Open order count

### Circuit Breakers

- Pause market if price deviation > X%
- Pause market if operator unresponsive > N blocks
- Global pause capability

### Tests

- Circuit breaker triggers correctly
- Metrics exported correctly
- Alert conditions fire

### Acceptance Criteria

```bash
cargo test -p monitoring
cd contracts && forge test --match-contract CircuitBreakerTest
# All tests pass
```

---

## Stage Summary

| Stage | Focus | Key Deliverable | Est. Effort |
|-------|-------|-----------------|-------------|
| 1 | Core Types | `crates/types` | 1-2 days |
| 2 | Order Book | `crates/orderbook` | 2-3 days |
| 3 | Matching Engine | `crates/matching` | 3-4 days |
| 4 | Account Management | `crates/state` (accounts) | 2-3 days |
| 5 | Fees & Settlement | `crates/state` (fees) | 1-2 days |
| 6 | Batch Assembly | `crates/batch` | 2-3 days |
| 7 | Vault Contract | `contracts/src/Vault.sol` | 2-3 days |
| 8 | Exchange Contract | `contracts/src/Exchange.sol` | 3-4 days |
| 9 | Markets Contract | `contracts/src/Markets.sol` | 1-2 days |
| 10 | E2E Integration | `crates/integration` | 3-4 days |
| 11 | API Gateway | `crates/gateway` | 3-4 days |
| 12 | Indexer | `crates/indexer` | 2-3 days |
| 13 | Forced Inclusion | `ForcedInclusion.sol` | 2-3 days |
| 14 | Monitoring | `crates/monitoring` | 2-3 days |

**Total Estimated Effort**: ~30-40 days

---

## Getting Started

To begin implementation:

```bash
# Initialize workspace
mkdir -p crates contracts
cd /home/longlong/tradingsystem

# Create Cargo workspace
cat > Cargo.toml << 'EOF'
[workspace]
resolver = "2"
members = [
    "crates/types",
    "crates/orderbook",
    "crates/matching",
    "crates/state",
    "crates/batch",
    "crates/integration",
    "crates/gateway",
    "crates/indexer",
    "crates/monitoring",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/your-org/tradingsystem"

[workspace.dependencies]
# Serialization
serde = { version = "1.0", features = ["derive"] }
borsh = { version = "1.0", features = ["derive"] }

# Crypto
k256 = "0.13"
sha3 = "0.10"

# Async
tokio = { version = "1.0", features = ["full"] }

# Web
axum = "0.7"
tower = "0.4"

# Ethereum
alloy = { version = "0.1", features = ["full"] }

# Testing
proptest = "1.0"
criterion = "0.5"

# Error handling
thiserror = "1.0"
anyhow = "1.0"
EOF

# Initialize Foundry for contracts
cd contracts
forge init --no-commit
```

**Next Step**: Begin with Stage 1 - Core Types & Primitives.

---

## Notes

- Each stage builds on the previous; do not skip stages
- All code must compile without warnings (`#![deny(warnings)]`)
- All tests must pass before moving to next stage
- Document any deviations from architecture in ADRs (Architecture Decision Records)
- Security-critical code should be reviewed before Stage 10 integration
