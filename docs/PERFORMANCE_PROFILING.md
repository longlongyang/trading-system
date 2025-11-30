# Performance Profiling Guide

This guide explains how to profile the trading system using Linux `perf` and `cargo flamegraph` to identify bottlenecks and validate performance characteristics.

---

## Table of Contents

1. [System Setup](#1-system-setup)
2. [Using Linux perf](#2-using-linux-perf)
3. [Using cargo flamegraph](#3-using-cargo-flamegraph)
4. [Interpreting Results](#4-interpreting-results)
5. [Project-Specific Profiling](#5-project-specific-profiling)
6. [Common Bottlenecks & Solutions](#6-common-bottlenecks--solutions)

---

## 1. System Setup

### 1.1 Required Packages (Ubuntu/Debian)

```bash
# Install perf tools
sudo apt update
sudo apt install linux-tools-common linux-tools-generic linux-tools-$(uname -r)

# Verify installation
perf --version
```

For other distributions:
```bash
# Fedora/RHEL
sudo dnf install perf

# Arch Linux
sudo pacman -S perf
```

### 1.2 Kernel Permissions

By default, `perf` requires elevated permissions. You have two options:

**Option A: Temporary permission (recommended for development)**
```bash
# Allow all users to collect perf data (resets on reboot)
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid

# Or for a specific session
sudo sysctl -w kernel.perf_event_paranoid=-1
```

**Option B: Permanent permission**
```bash
# Add to /etc/sysctl.conf
echo 'kernel.perf_event_paranoid=-1' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

**Permission levels:**
| Value | Meaning |
|-------|---------|
| -1 | Allow all users (no restrictions) |
| 0 | Allow all users, but not CPU events |
| 1 | Allow normal users (default on many systems) |
| 2 | Disallow raw tracepoint access |

### 1.3 Debug Symbols

For meaningful profiles, you need debug symbols even in release builds:

```toml
# In Cargo.toml - these settings enable debug info in release builds
[profile.release]
debug = true        # Include debug symbols
lto = "thin"        # Keep some symbol info with LTO
```

Or for a dedicated profiling profile:
```toml
[profile.profiling]
inherits = "release"
debug = true
```

Then build with:
```bash
cargo build --profile profiling
```

---

## 2. Using Linux perf

### 2.1 Basic Profiling Workflow

**Step 1: Record a profile**
```bash
# Profile the demo benchmark
perf record -g --call-graph dwarf \
    cargo run --release -p app-demo -- bench

# Or profile a specific binary directly (faster startup)
cargo build --release -p app-demo
perf record -g --call-graph dwarf \
    ./target/release/demo bench
```

**Step 2: Generate a report**
```bash
# Interactive TUI report
perf report

# Text report sorted by overhead
perf report --stdio --sort=overhead

# Flat profile (no call graph)
perf report --stdio --no-children
```

### 2.2 Key perf Flags

| Flag | Purpose |
|------|---------|
| `-g` | Enable call-graph (stack traces) |
| `--call-graph dwarf` | Use DWARF debug info (best for Rust) |
| `--call-graph fp` | Use frame pointers (faster, less accurate) |
| `-F <freq>` | Sampling frequency (default: 4000 Hz) |
| `-p <pid>` | Attach to running process |
| `-a` | System-wide profiling |
| `--` | Separator for the command to run |

### 2.3 Profiling the Matching Engine

**Profile order book operations:**
```bash
# Run the benchmark with profiling
cd /home/longlong/tradingsystem
perf record -g --call-graph dwarf \
    cargo run --release -p app-demo -- bench 2>&1 | head -50

# View the results
perf report
```

**Profile with higher sampling rate for short benchmarks:**
```bash
perf record -g --call-graph dwarf -F 10000 \
    cargo run --release -p app-demo -- bench
```

### 2.4 perf stat for Quick Metrics

Get hardware counter statistics without full profiling:

```bash
# Basic stats
perf stat cargo run --release -p app-demo -- bench

# Detailed cache/branch stats
perf stat -d cargo run --release -p app-demo -- bench

# Extended stats
perf stat -ddd cargo run --release -p app-demo -- bench
```

Example output interpretation:
```
 Performance counter stats for './target/release/demo bench':

      2,547.23 msec task-clock                #    0.998 CPUs utilized
            12      context-switches          #    4.711 /sec
             0      cpu-migrations            #    0.000 /sec
        12,345      page-faults               #    4.846 K/sec
 8,234,567,890      cycles                    #    3.233 GHz
12,345,678,901      instructions              #    1.50  insn per cycle   <-- Good IPC
   345,678,901      branches                  #  135.714 M/sec
     1,234,567      branch-misses             #    0.36% of all branches  <-- Good
```

Key metrics to watch:
- **Instructions per cycle (IPC)**: > 1.0 is good, > 2.0 is excellent
- **Branch misses**: < 1% is good for predictable code
- **Cache misses**: Lower is better, high L3 misses indicate memory-bound code

---

## 3. Using cargo flamegraph

### 3.1 Installation

```bash
# Install flamegraph
cargo install flamegraph

# Verify it works (needs perf)
flamegraph --version
```

**Note**: `cargo flamegraph` uses `perf` under the hood, so ensure the kernel permissions from Section 1.2 are set.

### 3.2 Generating Flamegraphs

**Profile the demo benchmark:**
```bash
cd /home/longlong/tradingsystem

# Generate flamegraph for the benchmark
cargo flamegraph --release -p app-demo -- bench

# Output: flamegraph.svg in the current directory
```

**Profile a specific binary:**
```bash
# Build first
cargo build --release -p app-demo

# Generate flamegraph
flamegraph -- ./target/release/demo bench
```

**Profile with custom output name:**
```bash
cargo flamegraph --release -p app-demo -o matching_profile.svg -- bench
```

### 3.3 Key flamegraph Options

| Option | Purpose |
|--------|---------|
| `-o <file>` | Output filename (default: flamegraph.svg) |
| `--release` | Build in release mode |
| `-p <package>` | Package to profile |
| `--bin <name>` | Binary to profile |
| `--bench <name>` | Benchmark to profile |
| `--root` | Run with sudo (if permissions aren't set) |
| `--freq <N>` | Sampling frequency |
| `--` | Separator for arguments to the binary |

### 3.4 Viewing Flamegraphs

**Open in browser:**
```bash
# Linux
xdg-open flamegraph.svg

# Or specify browser
firefox flamegraph.svg
google-chrome flamegraph.svg
```

The flamegraph is an interactive SVG:
- **Click** on a frame to zoom in
- **Ctrl+F** to search for function names
- **Click** on title bar to reset zoom

---

## 4. Interpreting Results

### 4.1 Reading a Flamegraph

```
         ┌─────────────────────────────────────────┐
         │           main (total time)             │  ← Root frame
         └─────────────────────────────────────────┘
                           │
         ┌─────────────────┴──────────────────────┐
         │        run_benchmarks                   │
         └─────────────────┬──────────────────────┘
                           │
    ┌──────────────────────┼──────────────────────┐
    │                      │                      │
┌───┴───────┐    ┌─────────┴────────┐    ┌───────┴──────┐
│ bench_    │    │ bench_matching   │    │ bench_       │
│ insert    │    │ (WIDEST = HOT)   │    │ cancel       │
│   25%     │    │      60%         │    │   15%        │
└───────────┘    └──────────────────┘    └──────────────┘
                          │
              ┌───────────┼───────────┐
              │           │           │
         ┌────┴────┐ ┌────┴────┐ ┌────┴────┐
         │BTreeMap │ │match_   │ │ Vec::   │
         │::insert │ │order    │ │ push    │
         │  20%    │ │  30%    │ │  10%    │
         └─────────┘ └─────────┘ └─────────┘
```

**Key principles:**
1. **Width = time**: Wider frames took more CPU time
2. **Y-axis = call stack**: Higher frames were called by lower frames
3. **Hot spots**: Look for the widest frames near the top
4. **Color**: Usually random, not meaningful (unless configured)

### 4.2 Common Patterns to Look For

**Healthy matching engine profile:**
```
Expected hot spots:
├── BTreeMap operations (30-40%)     ← Price level lookups
├── HashMap operations (20-30%)      ← Order ID lookups
├── VecDeque operations (10-20%)     ← FIFO queue management
├── Iterator operations (5-10%)      ← Traversal
└── Arithmetic/comparison (5-10%)    ← Price comparisons
```

**Warning signs:**

| Pattern | Indicates | Action |
|---------|-----------|--------|
| Wide `malloc`/`free` frames | Allocation-heavy | Consider arenas, pre-allocation |
| Wide `memcpy` frames | Excessive copying | Use references, `Cow` |
| Wide `drop` frames | Many destructions | Pool objects, reduce clones |
| Wide lock functions | Contention | Single-thread or reduce sharing |
| Wide `format!` calls | String formatting | Lazy formatting, structured logs |

### 4.3 Comparing Before/After

Generate two profiles and compare:

```bash
# Before optimization
cargo flamegraph --release -p app-demo -o before.svg -- bench

# Make changes...

# After optimization
cargo flamegraph --release -p app-demo -o after.svg -- bench

# Compare visually or use differential flamegraph
# (Install: cargo install inferno)
inferno-diff-folded before.svg after.svg > diff.svg
```

---

## 5. Project-Specific Profiling

### 5.1 Profiling the Demo Benchmark

The `app-demo` crate includes benchmarks for all core operations:

```bash
cd /home/longlong/tradingsystem

# Profile full benchmark suite
cargo flamegraph --release -p app-demo -o full_bench.svg -- bench

# The benchmark tests:
# - Order insertion throughput
# - Deep book matching
# - Single-level matching
# - Cancellation throughput
```

### 5.2 Profiling Specific Operations

**Create a focused test binary** (if needed):

```bash
# Run only matching operations with extended duration
cargo build --release -p app-demo
perf record -g --call-graph dwarf -F 10000 \
    ./target/release/demo bench 2>&1 | grep -A5 "Deep Book"
```

### 5.3 Profiling in CI (Optional)

Add to your CI workflow for regression detection:

```yaml
# .github/workflows/bench.yml
name: Benchmark
on: [push]
jobs:
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Run benchmarks
        run: cargo run --release -p app-demo -- bench
      # Note: flamegraph requires perf permissions not available in CI
```

### 5.4 What to Validate

Use profiling to confirm:

1. **Single-threaded engine is the hot spot**: Most time should be in `matching` and `orderbook` crates, not in synchronization primitives.

2. **HashMap/BTreeMap are the core data structure costs**: This is expected and means the design is sound.

3. **No unexpected allocations**: Look for wide `alloc`/`malloc` frames that aren't part of container operations.

4. **No lock contention**: If you see mutex/rwlock frames, you've accidentally introduced concurrency in the hot path.

---

## 6. Common Bottlenecks & Solutions

### 6.1 Allocation-Heavy Code

**Symptom**: Wide `malloc`/`alloc` frames (> 15% of total)

**Diagnosis**:
```bash
perf record -e malloc -g ./target/release/demo bench
```

**Solutions**:
```rust
// Before: allocating in hot loop
for order in orders {
    let fills = Vec::new();  // Allocation every iteration
    process(order, &mut fills);
}

// After: reuse allocation
let mut fills = Vec::with_capacity(100);
for order in orders {
    fills.clear();  // Reuse capacity
    process(order, &mut fills);
}
```

### 6.2 Excessive Cloning

**Symptom**: Wide `clone` or `memcpy` frames

**Solutions**:
```rust
// Before: cloning
let order = book.get_order(id).clone();
process(order);

// After: borrowing
if let Some(order) = book.get_order(id) {
    process_ref(order);
}
```

### 6.3 HashMap Resize

**Symptom**: Spikes in `HashMap::reserve` or `grow`

**Solutions**:
```rust
// Before: default capacity
let mut orders = HashMap::new();

// After: pre-sized
let mut orders = HashMap::with_capacity(expected_orders);
```

### 6.4 BTreeMap Rebalancing

**Symptom**: Time in `BTreeMap::fix_*` functions

This is generally unavoidable for price-ordered books. If it's dominant:
- Consider if a different data structure fits your access patterns
- Pre-warm the book with realistic price levels

### 6.5 Verifying the Hot Path

Your matching engine hot path should show:

```
Expected CPU distribution (order insertion):
┌────────────────────────────────────────────────────────┐
│ OrderBook::insert_order                           100% │
├────────────────────────────────────────────────────────┤
│ ├── BTreeMap::entry / insert                      ~40% │
│ ├── HashMap::entry / insert                       ~30% │
│ ├── PriceLevel::push_back                         ~15% │
│ ├── Order field access / copy                     ~10% │
│ └── Misc (bounds checks, etc.)                     ~5% │
└────────────────────────────────────────────────────────┘
```

If you see significant time outside these operations (locks, allocations, I/O), investigate.

---

## Quick Reference

### One-Liner Commands

```bash
# Quick perf stats
perf stat cargo run --release -p app-demo -- bench

# Full flamegraph
cargo flamegraph --release -p app-demo -- bench

# Interactive perf report
perf record -g --call-graph dwarf cargo run --release -p app-demo -- bench && perf report

# Cache analysis
perf stat -d cargo run --release -p app-demo -- bench
```

### Checklist Before Profiling

- [ ] Release build with debug symbols
- [ ] `perf_event_paranoid` set appropriately
- [ ] Consistent system load (close other applications)
- [ ] Warm the system (run benchmark once before recording)
- [ ] Sufficient sampling duration (> 1 second of execution)

### Interpreting Results Checklist

- [ ] Is the matching engine the hot spot? (expected: yes)
- [ ] Are BTreeMap/HashMap the main costs? (expected: yes)
- [ ] Is allocation < 15% of time? (good if yes)
- [ ] Are there any lock/mutex frames? (bad if yes in hot path)
- [ ] Is IPC > 1.0? (good if yes)
- [ ] Are branch misses < 1%? (good if yes)

---

## Further Reading

- [The Flame Graph](https://www.brendangregg.com/flamegraphs.html) - Brendan Gregg's original work
- [cargo-flamegraph](https://github.com/flamegraph-rs/flamegraph) - Official repository
- [Linux perf wiki](https://perf.wiki.kernel.org/) - Comprehensive perf documentation
- [Rust Performance Book](https://nnethercote.github.io/perf-book/) - General Rust optimization

