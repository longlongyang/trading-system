# Design Notes: Core Types Crate

This document explains the key design decisions made in the `types` crate.

---

## Fixed-Point Arithmetic

### Why Fixed-Point?

We use fixed-point arithmetic (represented as `u128` integers) instead of floating-point for several critical reasons:

1. **Determinism**: Floating-point operations can produce different results on different hardware/compilers. Fixed-point guarantees identical results everywhere.

2. **zkVM Compatibility**: Many zkVMs don't support floating-point natively. Using integers ensures our matching engine can be compiled to zkVM in the future.

3. **Precision Control**: We know exactly how much precision we have and where rounding occurs.

### Representation

- **Price**: 18 decimal places (same as most ERC-20 tokens)
  - `1.00` is stored as `1_000_000_000_000_000_000`
  - Maximum representable value: ~340 undecillion (far more than needed)
  
- **Quantity**: 18 decimal places
  - Matches token decimals for straightforward conversions

### Overflow Protection

All arithmetic operations use checked arithmetic (`checked_add`, `checked_mul`, etc.) and return `Option<T>` or `Result<T, E>` to make overflow handling explicit. We never panic on overflow.

---

## Identifier Design

### OrderId

`OrderId` is a `u128` that encodes:
- **Upper 64 bits**: `MarketId` - identifies which market the order belongs to
- **Lower 64 bits**: Sequence number - unique within the market

This encoding allows:
- O(1) extraction of market from order ID
- Globally unique order IDs across all markets
- ~18 quintillion orders per market (more than enough)

### MarketId

A simple `u64`. Markets are identified by a sequential ID assigned at creation time.
The on-chain contract stores the mapping from `MarketId` to token pair.

### Address

We use a 20-byte array `[u8; 20]` to represent Ethereum addresses. This is:
- The exact on-chain representation
- Easy to convert to/from hex strings
- Easy to serialize with borsh

---

## Order Representation

### Separation of Concerns

- **OrderType**: What kind of order (Limit vs Market)
- **TimeInForce**: How long the order lives (GTC, IOC, FOK, PostOnly)

These are separate enums because they are orthogonal concepts. A Limit order can be GTC, IOC, or FOK. A Market order is implicitly IOC (fills immediately or not at all).

### Price Bound for Market Orders

Market orders include an optional `price_bound` field:
- For buys: maximum price willing to pay
- For sells: minimum price willing to accept

This protects users from extreme slippage during volatile conditions.

---

## Serialization Strategy

### Borsh for Internal State

We use [borsh](https://borsh.io/) for:
- State serialization (order book snapshots)
- Network messages between components
- Potential zkVM compatibility

Borsh is:
- Deterministic (same input always produces same bytes)
- Fast
- Compact

### Serde for External APIs

We derive `serde::Serialize` and `serde::Deserialize` for:
- JSON API responses
- Configuration files
- Debugging

---

## Error Handling

All errors are defined in a single `EngineError` enum. This makes error handling consistent across the codebase and ensures all error cases are documented in one place.

Errors are categorized by subsystem:
- Order validation errors
- Balance/account errors  
- Market errors
- Matching errors

---

## Testing Philosophy

1. **Unit tests live next to the code** in `#[cfg(test)]` modules
2. **Property-based tests** using `proptest` for numeric types
3. **Round-trip tests** for all serializable types
4. **Boundary tests** for overflow and edge cases
