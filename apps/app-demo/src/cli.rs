//! Interactive CLI for the trading system demo.
//!
//! Provides a simple command loop to place/cancel orders and view the book.

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::single_match_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::print_literal)]

use std::io::{self, Write};

use matching::{ExecutionReport, MatchFill, MatchOutcome, MatchingEngine};
use orderbook::OrderBook;
use types::{
    Address, MarketId, Nonce, Order, OrderId, OrderSide, Price, Quantity, TimeInForce, Timestamp,
    SCALE,
};

// =============================================================================
// Demo State
// =============================================================================

/// Holds all state for the interactive demo.
struct DemoState {
    /// The single order book we're working with.
    book: OrderBook,
    /// The matching engine.
    engine: MatchingEngine,
    /// Next sequence number for order IDs.
    next_seq: u64,
    /// Next nonce for orders (simulated user nonces).
    next_nonce: u64,
    /// Market ID for this demo.
    market_id: MarketId,
    /// Demo user address.
    user_address: Address,
    /// Recent trades (fills) for display.
    recent_trades: Vec<MatchFill>,
    /// Maximum trades to keep in history.
    max_trade_history: usize,
}

impl DemoState {
    fn new() -> Self {
        let market_id = MarketId::new(1);
        Self {
            book: OrderBook::new(market_id),
            engine: MatchingEngine::new(),
            next_seq: 1,
            next_nonce: 1,
            market_id,
            user_address: Address::new([0xDE; 20]), // Demo user
            recent_trades: Vec::new(),
            max_trade_history: 100,
        }
    }

    /// Generate the next order ID and increment sequence.
    fn next_order_id(&mut self) -> OrderId {
        let id = OrderId::new(self.market_id, self.next_seq);
        self.next_seq += 1;
        id
    }

    /// Generate the next nonce and increment.
    fn next_nonce(&mut self) -> Nonce {
        let n = Nonce::new(self.next_nonce);
        self.next_nonce += 1;
        n
    }

    /// Current timestamp (mock: just use nonce * 1000).
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(self.next_nonce * 1000)
    }

    /// Record fills from an execution report.
    fn record_fills(&mut self, fills: &[MatchFill]) {
        for fill in fills {
            self.recent_trades.push(*fill);
        }
        // Trim to max history
        while self.recent_trades.len() > self.max_trade_history {
            self.recent_trades.remove(0);
        }
    }
}

// =============================================================================
// CLI Entry Point
// =============================================================================

/// Run the interactive CLI.
pub fn run_interactive() {
    println!("==============================================");
    println!("  Trading System Demo - Interactive Mode");
    println!("==============================================");
    println!();
    println!("Type 'help' for available commands.");
    println!();

    let mut state = DemoState::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Print prompt
        print!("> ");
        stdout.flush().unwrap();

        // Read input
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Parse and execute command
        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "help" | "h" | "?" => print_help(),
            "quit" | "exit" | "q" => {
                println!("Goodbye!");
                break;
            }
            "book" | "b" => cmd_book(&state, &parts),
            "place" | "p" => cmd_place(&mut state, &parts),
            "market" | "m" => cmd_market(&mut state, &parts),
            "cancel" | "c" => cmd_cancel(&mut state, &parts),
            "trades" | "t" => cmd_trades(&state, &parts),
            "orders" | "o" => cmd_orders(&state),
            "spread" | "s" => cmd_spread(&state),
            _ => println!(
                "Unknown command: '{}'. Type 'help' for available commands.",
                cmd
            ),
        }
        println!();
    }
}

// =============================================================================
// Command Handlers
// =============================================================================

fn print_help() {
    println!(
        r#"
Available commands:

  book [N]                          Show top N levels of the order book (default: 5)
  place <buy|sell> <price> <qty> [tif]
                                    Place a limit order
                                    tif: gtc (default), ioc, fok, postonly
  market <buy|sell> <qty>           Place a market order
  cancel <seq>                      Cancel order by sequence number
  trades [N]                        Show last N trades (default: 10)
  orders                            Show all resting orders
  spread                            Show best bid/ask and spread
  help                              Show this help message
  quit                              Exit the demo

Examples:
  place buy 100 10                  Buy 10 @ 100 (GTC)
  place sell 105 5 postonly         Sell 5 @ 105 (PostOnly)
  market buy 3                      Market buy 3 units
  cancel 1                          Cancel order with sequence 1
  book 10                           Show top 10 levels
"#
    );
}

/// Display the order book.
fn cmd_book(state: &DemoState, parts: &[&str]) {
    let depth: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);

    println!();
    println!("╔════════════════════════════════════════════╗");
    println!("║              ORDER BOOK                    ║");
    println!("╠════════════════════════════════════════════╣");

    // Collect ask levels (best = lowest price, display in reverse so lowest is at bottom)
    let asks: Vec<_> = state.book.ask_levels().take(depth).collect();

    // Print asks from worst to best (so best ask is at bottom, near spread)
    println!("║  ASKS (Sell Orders)                        ║");
    println!("║  Price           Qty         Orders        ║");
    println!("║  ─────────────────────────────────────     ║");

    if asks.is_empty() {
        println!("║  (no asks)                                 ║");
    } else {
        // Print in reverse (worst to best) so best ask is closest to spread
        for level in asks.iter().rev() {
            let price = format_price(level.price());
            let qty = format_quantity(level.total_quantity());
            let count = level.order_count();
            println!("║  {:>12}  {:>12}  {:>6}         ║", price, qty, count);
        }
    }

    println!("║  ═══════════════════════════════════════   ║");

    // Spread info
    if let (Some(bid), Some(ask)) = (state.book.best_bid_price(), state.book.best_ask_price()) {
        if let Some(spread) = ask.checked_sub(bid) {
            let mid = state.book.mid_price().map(format_price).unwrap_or_default();
            println!(
                "║  Spread: {}  Mid: {}             ║",
                format_price(spread),
                mid
            );
        }
    } else {
        println!("║  Spread: N/A                               ║");
    }

    println!("║  ═══════════════════════════════════════   ║");

    // Print bids from best to worst
    println!("║  BIDS (Buy Orders)                         ║");
    println!("║  Price           Qty         Orders        ║");
    println!("║  ─────────────────────────────────────     ║");

    let bids: Vec<_> = state.book.bid_levels().take(depth).collect();
    if bids.is_empty() {
        println!("║  (no bids)                                 ║");
    } else {
        for level in &bids {
            let price = format_price(level.price());
            let qty = format_quantity(level.total_quantity());
            let count = level.order_count();
            println!("║  {:>12}  {:>12}  {:>6}         ║", price, qty, count);
        }
    }

    println!("╚════════════════════════════════════════════╝");
    println!();
    println!(
        "Total orders: {}  Bid levels: {}  Ask levels: {}",
        state.book.order_count(),
        state.book.bid_level_count(),
        state.book.ask_level_count()
    );
}

/// Place a limit order.
fn cmd_place(state: &mut DemoState, parts: &[&str]) {
    // place <buy|sell> <price> <qty> [tif]
    if parts.len() < 4 {
        println!("Usage: place <buy|sell> <price> <qty> [tif]");
        println!("  tif options: gtc (default), ioc, fok, postonly");
        return;
    }

    let side = match parts[1].to_lowercase().as_str() {
        "buy" | "b" => OrderSide::Buy,
        "sell" | "s" => OrderSide::Sell,
        _ => {
            println!("Invalid side: '{}'. Use 'buy' or 'sell'.", parts[1]);
            return;
        }
    };

    let price: u64 = match parts[2].parse() {
        Ok(p) if p > 0 => p,
        _ => {
            println!("Invalid price: '{}'. Must be a positive integer.", parts[2]);
            return;
        }
    };

    let qty: u64 = match parts[3].parse() {
        Ok(q) if q > 0 => q,
        _ => {
            println!(
                "Invalid quantity: '{}'. Must be a positive integer.",
                parts[3]
            );
            return;
        }
    };

    let tif = if let Some(tif_str) = parts.get(4) {
        match tif_str.to_lowercase().as_str() {
            "gtc" => TimeInForce::GoodTilCancelled,
            "ioc" => TimeInForce::ImmediateOrCancel,
            "fok" => TimeInForce::FillOrKill,
            "postonly" | "post" | "po" => TimeInForce::PostOnly,
            _ => {
                println!(
                    "Invalid TIF: '{}'. Use gtc, ioc, fok, or postonly.",
                    tif_str
                );
                return;
            }
        }
    } else {
        TimeInForce::GoodTilCancelled
    };

    let order_id = state.next_order_id();
    let order = Order::new_limit(
        order_id,
        state.market_id,
        state.user_address,
        side,
        tif,
        Price::from_whole(price),
        Quantity::from_whole(qty),
        state.next_nonce(),
        state.now(),
    );

    println!();
    println!(
        "Placing: {} {} {} @ {} ({})",
        side, qty, "units", price, tif
    );

    match state.engine.place_order(&mut state.book, order) {
        Ok(report) => {
            print_execution_report(&report);
            state.record_fills(&report.fills);
        }
        Err(e) => {
            println!("ERROR: {:?}", e);
        }
    }
}

/// Place a market order.
fn cmd_market(state: &mut DemoState, parts: &[&str]) {
    // market <buy|sell> <qty>
    if parts.len() < 3 {
        println!("Usage: market <buy|sell> <qty>");
        return;
    }

    let side = match parts[1].to_lowercase().as_str() {
        "buy" | "b" => OrderSide::Buy,
        "sell" | "s" => OrderSide::Sell,
        _ => {
            println!("Invalid side: '{}'. Use 'buy' or 'sell'.", parts[1]);
            return;
        }
    };

    let qty: u64 = match parts[2].parse() {
        Ok(q) if q > 0 => q,
        _ => {
            println!(
                "Invalid quantity: '{}'. Must be a positive integer.",
                parts[2]
            );
            return;
        }
    };

    let order_id = state.next_order_id();
    let order = Order::new_market(
        order_id,
        state.market_id,
        state.user_address,
        side,
        Quantity::from_whole(qty),
        state.next_nonce(),
        state.now(),
    );

    println!();
    println!("Placing: MARKET {} {} units", side, qty);

    match state.engine.place_order(&mut state.book, order) {
        Ok(report) => {
            print_execution_report(&report);
            state.record_fills(&report.fills);
        }
        Err(e) => {
            println!("ERROR: {:?}", e);
        }
    }
}

/// Cancel an order by sequence number.
fn cmd_cancel(state: &mut DemoState, parts: &[&str]) {
    if parts.len() < 2 {
        println!("Usage: cancel <sequence_number>");
        return;
    }

    let seq: u64 = match parts[1].parse() {
        Ok(s) => s,
        Err(_) => {
            println!("Invalid sequence number: '{}'", parts[1]);
            return;
        }
    };

    let order_id = OrderId::new(state.market_id, seq);

    println!();
    println!("Cancelling order #{}", seq);

    let result = state.engine.cancel_order(&mut state.book, order_id);
    match result.outcome {
        matching::CancelOutcome::Cancelled => {
            println!("✓ Order cancelled successfully");
            if let Some(order) = result.order {
                println!(
                    "  Cancelled: {} {} @ {} (remaining: {})",
                    order.side,
                    format_quantity(order.remaining_quantity),
                    format_price(order.price),
                    format_quantity(result.cancelled_quantity)
                );
            }
        }
        matching::CancelOutcome::NotFound => {
            println!("✗ Order not found (seq #{})", seq);
        }
        matching::CancelOutcome::AlreadyTerminal => {
            println!("✗ Order already in terminal state");
        }
    }
}

/// Show recent trades.
fn cmd_trades(state: &DemoState, parts: &[&str]) {
    let count: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);

    println!();
    println!("Recent Trades (last {}):", count);
    println!("─────────────────────────────────────────────");

    if state.recent_trades.is_empty() {
        println!("  (no trades yet)");
        return;
    }

    let start = state.recent_trades.len().saturating_sub(count);
    for (i, fill) in state.recent_trades.iter().skip(start).enumerate() {
        let side_str = if fill.taker_side == OrderSide::Buy {
            "BUY "
        } else {
            "SELL"
        };
        println!(
            "  #{}: {} {} @ {} (maker #{}, taker #{})",
            start + i + 1,
            side_str,
            format_quantity(fill.quantity),
            format_price(fill.price),
            fill.maker_order_id.sequence(),
            fill.taker_order_id.sequence(),
        );
    }
}

/// Show all resting orders.
fn cmd_orders(state: &DemoState) {
    println!();
    println!("Resting Orders:");
    println!("─────────────────────────────────────────────");

    if state.book.is_empty() {
        println!("  (no resting orders)");
        return;
    }

    let mut orders: Vec<_> = state.book.orders().collect();
    orders.sort_by_key(|o| o.id.sequence());

    for order in orders {
        println!(
            "  #{}: {} {} @ {} [filled: {}/{}] {}",
            order.id.sequence(),
            order.side,
            format_quantity(order.remaining_quantity),
            format_price(order.price),
            format_quantity(order.filled_quantity()),
            format_quantity(order.quantity),
            order.time_in_force,
        );
    }
}

/// Show spread information.
fn cmd_spread(state: &DemoState) {
    println!();
    let bid = state.book.best_bid_price();
    let ask = state.book.best_ask_price();

    println!(
        "Best Bid: {}",
        bid.map(format_price).unwrap_or_else(|| "N/A".to_string())
    );
    println!(
        "Best Ask: {}",
        ask.map(format_price).unwrap_or_else(|| "N/A".to_string())
    );

    if let (Some(b), Some(a)) = (bid, ask) {
        if let Some(spread) = a.checked_sub(b) {
            println!("Spread:   {}", format_price(spread));
        }
        if let Some(mid) = state.book.mid_price() {
            println!("Mid:      {}", format_price(mid));
        }
    }
}

// =============================================================================
// Formatting Helpers
// =============================================================================

/// Format a Price for display.
fn format_price(price: Price) -> String {
    let scaled = price.as_scaled();
    let whole = scaled / SCALE;
    let frac = scaled % SCALE;
    if frac == 0 {
        format!("{}", whole)
    } else {
        // Show up to 4 decimal places
        let frac_4 = frac / (SCALE / 10_000);
        format!("{}.{:04}", whole, frac_4)
    }
}

/// Format a Quantity for display.
fn format_quantity(qty: Quantity) -> String {
    let scaled = qty.as_scaled();
    let whole = scaled / SCALE;
    let frac = scaled % SCALE;
    if frac == 0 {
        format!("{}", whole)
    } else {
        let frac_4 = frac / (SCALE / 10_000);
        format!("{}.{:04}", whole, frac_4)
    }
}

/// Print an execution report in a nice format.
fn print_execution_report(report: &ExecutionReport) {
    println!();
    println!("┌─ Execution Report ─────────────────────────┐");
    println!(
        "│ Order #{}  {} {} @ {}",
        report.order.id.sequence(),
        report.order.side,
        format_quantity(report.order.quantity),
        format_price(report.order.price),
    );
    println!(
        "│ TIF: {}  Type: {}",
        report.order.time_in_force, report.order.order_type
    );
    println!("├────────────────────────────────────────────┤");

    // Outcome
    let outcome_str = match report.outcome {
        MatchOutcome::Filled => "✓ FILLED",
        MatchOutcome::PartiallyFilledResting => "◐ PARTIAL (resting)",
        MatchOutcome::PartiallyFilledCancelled => "◐ PARTIAL (cancelled)",
        MatchOutcome::Resting => "○ RESTING",
        MatchOutcome::Cancelled => "✗ CANCELLED",
        MatchOutcome::RejectedPostOnly => "✗ REJECTED (PostOnly would cross)",
        MatchOutcome::RejectedFillOrKill => "✗ REJECTED (FOK not fillable)",
    };
    println!("│ Outcome: {}", outcome_str);
    println!(
        "│ Filled: {}  Remaining: {}",
        format_quantity(report.filled_quantity),
        format_quantity(report.remaining_quantity)
    );

    // Fills
    if !report.fills.is_empty() {
        println!("├────────────────────────────────────────────┤");
        println!("│ Fills ({}):", report.fills.len());
        for fill in &report.fills {
            println!(
                "│   {} @ {} (maker #{})",
                format_quantity(fill.quantity),
                format_price(fill.price),
                fill.maker_order_id.sequence(),
            );
        }
        if let Some(avg) = report.average_price() {
            println!("│ Average price: {}", format_price(avg));
        }
    }

    println!("└────────────────────────────────────────────┘");
}
