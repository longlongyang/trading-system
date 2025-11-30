//! Core matching logic.
//!
//! This module implements the price-time priority matching algorithm.
//! It is used internally by [`MatchingEngine`](crate::MatchingEngine).
//!
//! ## Matching Rules
//!
//! 1. **Price Priority**: Orders with better prices are matched first
//!    - Buy orders: Higher bid prices have priority
//!    - Sell orders: Lower ask prices have priority
//!
//! 2. **Time Priority**: At the same price, earlier orders match first (FIFO)
//!
//! 3. **Price Determination**: Trades execute at the maker's (resting) price
//!
//! ## Crossing Logic
//!
//! An incoming buy order crosses the book if `buy_price >= best_ask`.
//! An incoming sell order crosses the book if `sell_price <= best_bid`.

use orderbook::OrderBook;
use types::{Order, OrderId, OrderSide, Price, Quantity, Timestamp};

use crate::result::MatchFill;

/// Checks if an incoming order would cross the spread.
///
/// For a buy order, checks if the price is >= best ask.
/// For a sell order, checks if the price is <= best bid.
///
/// Returns `true` if the order would cross and potentially match.
#[must_use]
pub fn would_cross(book: &OrderBook, side: OrderSide, price: Price) -> bool {
    match side {
        OrderSide::Buy => {
            if let Some(best_ask) = book.best_ask_price() {
                price >= best_ask
            } else {
                false // No asks to cross
            }
        }
        OrderSide::Sell => {
            if let Some(best_bid) = book.best_bid_price() {
                price <= best_bid
            } else {
                false // No bids to cross
            }
        }
    }
}

/// Calculates the available quantity at prices that would match the incoming order.
///
/// For FOK orders, this determines if the order can be fully filled.
///
/// Returns the total matchable quantity up to the given price limit.
#[must_use]
pub fn available_quantity(book: &OrderBook, side: OrderSide, price: Price) -> Quantity {
    let mut total = Quantity::ZERO;

    match side {
        OrderSide::Buy => {
            // Taker is buying, so match against asks
            for ask_level in book.ask_levels() {
                if ask_level.price() > price {
                    break; // Price too high, stop
                }
                total = total
                    .checked_add(ask_level.total_quantity())
                    .unwrap_or(Quantity::MAX);
            }
        }
        OrderSide::Sell => {
            // Taker is selling, so match against bids
            for bid_level in book.bid_levels() {
                if bid_level.price() < price {
                    break; // Price too low, stop
                }
                total = total
                    .checked_add(bid_level.total_quantity())
                    .unwrap_or(Quantity::MAX);
            }
        }
    }

    total
}

/// Calculates available quantity for a market order (no price limit).
///
/// Returns the total available liquidity on the opposite side.
#[must_use]
pub fn available_quantity_market(book: &OrderBook, side: OrderSide) -> Quantity {
    match side {
        OrderSide::Buy => book.total_ask_quantity(),
        OrderSide::Sell => book.total_bid_quantity(),
    }
}

/// Matches an incoming order against the order book.
///
/// This is the core matching function. It walks through the opposite side
/// of the book (asks for buy orders, bids for sell orders) and generates
/// fills for each match.
///
/// # Arguments
///
/// * `book` - The order book to match against
/// * `taker_order` - The incoming order
/// * `taker` - The taker's address
/// * `max_quantity` - Maximum quantity to fill (usually `remaining_quantity`)
/// * `timestamp` - Timestamp for the fills
///
/// # Returns
///
/// A tuple of (fills, `remaining_quantity`, `consumed_order_ids`, `partial_fills`).
#[must_use]
pub fn match_order(
    book: &OrderBook,
    taker_order: &Order,
    max_quantity: Quantity,
    timestamp: Timestamp,
) -> MatchResult {
    let mut fills = Vec::new();
    let mut remaining = max_quantity;
    let mut consumed_orders = Vec::new();
    let mut partial_fills = Vec::new();

    let taker_side = taker_order.side;
    let taker_price = taker_order.price;
    let is_market = taker_order.is_market();

    match taker_side {
        OrderSide::Buy => {
            // Match against asks (lowest price first)
            for ask_level in book.ask_levels() {
                if remaining.is_zero() {
                    break;
                }

                let ask_price = ask_level.price();

                // For limit orders, check price constraint
                if !is_market && ask_price > taker_price {
                    break; // Price too high, stop matching
                }

                // Match against orders at this level
                match_at_level(
                    ask_level,
                    book,
                    taker_order,
                    ask_price,
                    &mut remaining,
                    &mut fills,
                    &mut consumed_orders,
                    &mut partial_fills,
                    timestamp,
                );
            }
        }
        OrderSide::Sell => {
            // Match against bids (highest price first)
            for bid_level in book.bid_levels() {
                if remaining.is_zero() {
                    break;
                }

                let bid_price = bid_level.price();

                // For limit orders, check price constraint
                if !is_market && bid_price < taker_price {
                    break; // Price too low, stop matching
                }

                // Match against orders at this level
                match_at_level(
                    bid_level,
                    book,
                    taker_order,
                    bid_price,
                    &mut remaining,
                    &mut fills,
                    &mut consumed_orders,
                    &mut partial_fills,
                    timestamp,
                );
            }
        }
    }

    MatchResult {
        fills,
        remaining_quantity: remaining,
        consumed_orders,
        partial_fills,
    }
}

/// The result of the matching process.
#[derive(Debug)]
pub struct MatchResult {
    /// Fills generated during matching.
    pub fills: Vec<MatchFill>,

    /// Remaining quantity after matching.
    pub remaining_quantity: Quantity,

    /// Order IDs that were fully consumed.
    pub consumed_orders: Vec<OrderId>,

    /// Orders that were partially filled: (`order_id`, `new_remaining_qty`).
    pub partial_fills: Vec<(OrderId, Quantity)>,
}

/// Matches against orders at a single price level.
#[allow(clippy::too_many_arguments)]
fn match_at_level(
    level: &orderbook::PriceLevel,
    book: &OrderBook,
    taker_order: &Order,
    fill_price: Price,
    remaining: &mut Quantity,
    fills: &mut Vec<MatchFill>,
    consumed_orders: &mut Vec<OrderId>,
    partial_fills: &mut Vec<(OrderId, Quantity)>,
    timestamp: Timestamp,
) {
    // Iterate through orders in FIFO order
    for (maker_order_id, maker_quantity) in level.orders() {
        if remaining.is_zero() {
            break;
        }

        // Get maker order details
        let Some(maker_order) = book.get_order(maker_order_id) else {
            continue; // Order not found (shouldn't happen)
        };

        // Self-trade prevention: skip if same owner
        if maker_order.owner == taker_order.owner {
            continue;
        }

        // Calculate fill quantity
        let fill_qty = (*remaining).min(maker_quantity);

        // Create the fill
        let fill = MatchFill::new(
            maker_order_id,
            taker_order.id,
            maker_order.owner,
            taker_order.owner,
            fill_price,
            fill_qty,
            taker_order.side,
            timestamp,
        );
        fills.push(fill);

        // Update remaining
        *remaining = remaining.checked_sub(fill_qty).unwrap_or(Quantity::ZERO);

        // Track consumed/partial orders
        if fill_qty >= maker_quantity {
            consumed_orders.push(maker_order_id);
        } else {
            let new_remaining = maker_quantity
                .checked_sub(fill_qty)
                .unwrap_or(Quantity::ZERO);
            partial_fills.push((maker_order_id, new_remaining));
        }
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use types::{Address, MarketId, Nonce, TimeInForce};

    fn market_id() -> MarketId {
        MarketId::new(1)
    }

    fn maker_address() -> Address {
        Address::new([1u8; 20])
    }

    fn taker_address() -> Address {
        Address::new([2u8; 20])
    }

    fn make_sell_order(seq: u64, price: u64, qty: u64) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            maker_address(),
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    fn make_buy_order(seq: u64, price: u64, qty: u64) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            taker_address(),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    #[test]
    fn would_cross_empty_book() {
        let book = OrderBook::new(market_id());
        assert!(!would_cross(&book, OrderSide::Buy, Price::from_whole(100)));
        assert!(!would_cross(&book, OrderSide::Sell, Price::from_whole(100)));
    }

    #[test]
    fn would_cross_buy_crosses() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        // Buy at 100 crosses ask at 100
        assert!(would_cross(&book, OrderSide::Buy, Price::from_whole(100)));
        // Buy at 101 crosses ask at 100
        assert!(would_cross(&book, OrderSide::Buy, Price::from_whole(101)));
        // Buy at 99 does not cross
        assert!(!would_cross(&book, OrderSide::Buy, Price::from_whole(99)));
    }

    #[test]
    fn would_cross_sell_crosses() {
        let mut book = OrderBook::new(market_id());
        let bid_order = Order::new_limit(
            OrderId::new(market_id(), 1),
            market_id(),
            maker_address(),
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );
        book.insert_order(bid_order).unwrap();

        // Sell at 100 crosses bid at 100
        assert!(would_cross(&book, OrderSide::Sell, Price::from_whole(100)));
        // Sell at 99 crosses bid at 100
        assert!(would_cross(&book, OrderSide::Sell, Price::from_whole(99)));
        // Sell at 101 does not cross
        assert!(!would_cross(&book, OrderSide::Sell, Price::from_whole(101)));
    }

    #[test]
    fn available_quantity_empty_book() {
        let book = OrderBook::new(market_id());
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(100)),
            Quantity::ZERO
        );
    }

    #[test]
    fn available_quantity_single_level() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        // Buy at 100 can fill 10
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(100)),
            Quantity::from_whole(10)
        );

        // Buy at 99 can fill 0 (price too low)
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(99)),
            Quantity::ZERO
        );
    }

    #[test]
    fn available_quantity_multiple_levels() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();
        book.insert_order(make_sell_order(2, 101, 20)).unwrap();
        book.insert_order(make_sell_order(3, 102, 30)).unwrap();

        // Buy at 100 can only fill 10
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(100)),
            Quantity::from_whole(10)
        );

        // Buy at 101 can fill 10 + 20 = 30
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(101)),
            Quantity::from_whole(30)
        );

        // Buy at 102 can fill 10 + 20 + 30 = 60
        assert_eq!(
            available_quantity(&book, OrderSide::Buy, Price::from_whole(102)),
            Quantity::from_whole(60)
        );
    }

    #[test]
    fn match_order_single_fill() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        let buy_order = make_buy_order(2, 100, 10);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(10),
            Timestamp::from_millis(2000),
        );

        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(result.consumed_orders.len(), 1);
        assert!(result.partial_fills.is_empty());

        let fill = &result.fills[0];
        assert_eq!(fill.price, Price::from_whole(100));
        assert_eq!(fill.quantity, Quantity::from_whole(10));
        assert_eq!(fill.taker_side, OrderSide::Buy);
    }

    #[test]
    fn match_order_partial_fill() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        // Buy 5, but there are 10 available
        let buy_order = make_buy_order(2, 100, 5);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(5),
            Timestamp::from_millis(2000),
        );

        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert!(result.consumed_orders.is_empty()); // Maker not fully consumed
        assert_eq!(result.partial_fills.len(), 1);
        assert_eq!(result.partial_fills[0].1, Quantity::from_whole(5)); // 5 remaining
    }

    #[test]
    fn match_order_multi_level() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 5)).unwrap();
        book.insert_order(make_sell_order(2, 101, 5)).unwrap();

        // Buy 10 at 101 - should fill both levels
        let buy_order = make_buy_order(3, 101, 10);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(10),
            Timestamp::from_millis(3000),
        );

        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(result.consumed_orders.len(), 2);

        // First fill at 100
        assert_eq!(result.fills[0].price, Price::from_whole(100));
        assert_eq!(result.fills[0].quantity, Quantity::from_whole(5));

        // Second fill at 101
        assert_eq!(result.fills[1].price, Price::from_whole(101));
        assert_eq!(result.fills[1].quantity, Quantity::from_whole(5));
    }

    #[test]
    fn match_order_self_trade_prevention() {
        let mut book = OrderBook::new(market_id());

        // Maker and taker have same address
        let same_owner = maker_address();

        let sell_order = Order::new_limit(
            OrderId::new(market_id(), 1),
            market_id(),
            same_owner,
            OrderSide::Sell,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(1),
            Timestamp::from_millis(1000),
        );
        book.insert_order(sell_order).unwrap();

        let buy_order = Order::new_limit(
            OrderId::new(market_id(), 2),
            market_id(),
            same_owner, // Same owner
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(10),
            Nonce::new(2),
            Timestamp::from_millis(2000),
        );

        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(10),
            Timestamp::from_millis(2000),
        );

        // No fills due to self-trade prevention
        assert!(result.fills.is_empty());
        assert_eq!(result.remaining_quantity, Quantity::from_whole(10));
    }

    #[test]
    fn match_order_no_cross() {
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        // Buy at 99 - won't cross the ask at 100
        let buy_order = make_buy_order(2, 99, 10);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(10),
            Timestamp::from_millis(2000),
        );

        assert!(result.fills.is_empty());
        assert_eq!(result.remaining_quantity, Quantity::from_whole(10));
    }

    // =========================================================================
    // Stress Tests
    // =========================================================================

    /// Helper to create an order with a specific owner
    fn make_order_with_owner(
        seq: u64,
        side: OrderSide,
        price: u64,
        qty: u64,
        owner: Address,
    ) -> Order {
        Order::new_limit(
            OrderId::new(market_id(), seq),
            market_id(),
            owner,
            side,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(price),
            Quantity::from_whole(qty),
            Nonce::new(seq),
            Timestamp::from_millis(seq * 1000),
        )
    }

    /// Helper to create unique maker addresses that won't conflict with `taker_address`
    #[allow(clippy::cast_possible_truncation)]
    fn unique_maker_address(i: u64) -> Address {
        // Use values starting from 10 to avoid conflict with taker_address ([2u8; 20])
        let mut bytes = [0u8; 20];
        bytes[0] = ((i % 245) + 10) as u8; // 10-254 range
        bytes[1] = (i / 245) as u8;
        Address::new(bytes)
    }

    #[test]
    fn stress_many_orders_single_level() {
        // Test matching against many orders at the same price level
        let mut book = OrderBook::new(market_id());
        let num_orders = 100u64;

        // Insert 100 sell orders at price 100, each for qty 1
        for i in 1..=num_orders {
            let owner = unique_maker_address(i);
            let order = make_order_with_owner(i, OrderSide::Sell, 100, 1, owner);
            book.insert_order(order).unwrap();
        }

        assert_eq!(book.order_count(), num_orders as usize);

        // Create a buy order that should match all of them
        let buy_order = make_buy_order(num_orders + 1, 100, num_orders);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(num_orders),
            Timestamp::from_millis((num_orders + 1) * 1000),
        );

        // Should have 100 fills
        assert_eq!(result.fills.len(), num_orders as usize);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(result.consumed_orders.len(), num_orders as usize);

        // Verify fills are in FIFO order (by order ID sequence)
        for (i, fill) in result.fills.iter().enumerate() {
            assert_eq!(fill.maker_order_id.sequence(), (i + 1) as u64);
            assert_eq!(fill.quantity, Quantity::from_whole(1));
        }
    }

    #[test]
    fn stress_many_price_levels() {
        // Test matching across many price levels
        let mut book = OrderBook::new(market_id());
        let num_levels = 50u64;

        // Insert sell orders at prices 100, 101, 102, ..., 149
        for i in 0..num_levels {
            let owner = unique_maker_address(i + 1);
            let order = make_order_with_owner(i + 1, OrderSide::Sell, 100 + i, 10, owner);
            book.insert_order(order).unwrap();
        }

        assert_eq!(book.order_count(), num_levels as usize);
        assert_eq!(book.ask_level_count(), num_levels as usize);

        // Buy order at price 149 should match all levels
        let buy_order = make_buy_order(num_levels + 1, 149, num_levels * 10);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(num_levels * 10),
            Timestamp::from_millis((num_levels + 1) * 1000),
        );

        assert_eq!(result.fills.len(), num_levels as usize);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);

        // Verify fills are in price order (best price first = lowest ask)
        for (i, fill) in result.fills.iter().enumerate() {
            assert_eq!(fill.price, Price::from_whole(100 + i as u64));
        }
    }

    #[test]
    fn stress_partial_fill_many_orders() {
        // Test partial fill that spans multiple orders at same level
        let mut book = OrderBook::new(market_id());

        // Insert 10 sell orders at price 100, each for qty 10
        for i in 1..=10 {
            let owner = unique_maker_address(i);
            let order = make_order_with_owner(i, OrderSide::Sell, 100, 10, owner);
            book.insert_order(order).unwrap();
        }

        // Buy order for 55 units - should fill 5 completely, 1 partially
        let buy_order = make_buy_order(11, 100, 55);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(55),
            Timestamp::from_millis(11000),
        );

        assert_eq!(result.fills.len(), 6); // 5 full fills + 1 partial
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(result.consumed_orders.len(), 5); // 5 fully consumed
        assert_eq!(result.partial_fills.len(), 1); // 1 partial fill

        // First 5 fills are for 10 each
        for fill in result.fills.iter().take(5) {
            assert_eq!(fill.quantity, Quantity::from_whole(10));
        }

        // Last fill is for 5
        assert_eq!(result.fills[5].quantity, Quantity::from_whole(5));

        // Partial fill should have 5 remaining
        assert_eq!(result.partial_fills[0].1, Quantity::from_whole(5));
    }

    #[test]
    fn stress_mixed_price_levels_partial_fill() {
        // Test partial fill that crosses some levels completely and one partially
        let mut book = OrderBook::new(market_id());

        // Insert asks at prices 100 (qty 10), 101 (qty 20), 102 (qty 30)
        let owner1 = unique_maker_address(1);
        let owner2 = unique_maker_address(2);
        let owner3 = unique_maker_address(3);

        book.insert_order(make_order_with_owner(1, OrderSide::Sell, 100, 10, owner1))
            .unwrap();
        book.insert_order(make_order_with_owner(2, OrderSide::Sell, 101, 20, owner2))
            .unwrap();
        book.insert_order(make_order_with_owner(3, OrderSide::Sell, 102, 30, owner3))
            .unwrap();

        // Buy 25 at 102 - fills level 100 (10), partial level 101 (15)
        let buy_order = make_buy_order(4, 102, 25);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(25),
            Timestamp::from_millis(4000),
        );

        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);

        // First fill: 10 @ 100
        assert_eq!(result.fills[0].price, Price::from_whole(100));
        assert_eq!(result.fills[0].quantity, Quantity::from_whole(10));

        // Second fill: 15 @ 101
        assert_eq!(result.fills[1].price, Price::from_whole(101));
        assert_eq!(result.fills[1].quantity, Quantity::from_whole(15));

        // Order at 101 should have 5 remaining
        assert_eq!(result.partial_fills.len(), 1);
        assert_eq!(result.partial_fills[0].1, Quantity::from_whole(5));
    }

    #[test]
    fn stress_large_order_book() {
        // Test with a large order book (1000 orders)
        let mut book = OrderBook::new(market_id());
        let orders_per_level = 10u64;
        let num_levels = 100u64;

        // Insert 1000 sell orders across 100 price levels
        let mut seq = 1u64;
        for level in 0..num_levels {
            for _ in 0..orders_per_level {
                let owner = unique_maker_address(seq);
                let order = make_order_with_owner(seq, OrderSide::Sell, 100 + level, 1, owner);
                book.insert_order(order).unwrap();
                seq += 1;
            }
        }

        assert_eq!(book.order_count(), (num_levels * orders_per_level) as usize);

        // Calculate available quantity
        let available = available_quantity(&book, OrderSide::Buy, Price::from_whole(150));
        // Should include levels 100-150 = 51 levels * 10 orders * 1 qty = 510
        assert_eq!(available, Quantity::from_whole(510));
    }

    #[test]
    fn stress_self_trade_prevention_mixed() {
        // Test self-trade prevention with mixed ownership
        let mut book = OrderBook::new(market_id());
        let shared_owner = Address::new([99u8; 20]);

        // Insert alternating owned orders at price 100
        for i in 1..=10u64 {
            let owner = if i % 2 == 0 {
                shared_owner
            } else {
                unique_maker_address(i)
            };
            let order = make_order_with_owner(i, OrderSide::Sell, 100, 10, owner);
            book.insert_order(order).unwrap();
        }

        // Buy from shared_owner - should skip own orders (evens)
        let buy_order = Order::new_limit(
            OrderId::new(market_id(), 11),
            market_id(),
            shared_owner,
            OrderSide::Buy,
            TimeInForce::GoodTilCancelled,
            Price::from_whole(100),
            Quantity::from_whole(100),
            Nonce::new(11),
            Timestamp::from_millis(11000),
        );

        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(100),
            Timestamp::from_millis(11000),
        );

        // Should only match against odd-indexed orders (5 orders * 10 qty = 50)
        assert_eq!(result.fills.len(), 5);
        assert_eq!(
            result
                .fills
                .iter()
                .map(|f| f.quantity.as_scaled())
                .sum::<u128>(),
            Quantity::from_whole(50).as_scaled()
        );
        assert_eq!(result.remaining_quantity, Quantity::from_whole(50));

        // Verify we matched odd sequence numbers only
        for fill in &result.fills {
            assert!(fill.maker_order_id.sequence() % 2 == 1);
        }
    }

    // =========================================================================
    // Performance / Timing Tests
    // =========================================================================

    #[test]
    fn perf_match_1000_orders() {
        // Performance test: match against 1000 orders
        let mut book = OrderBook::new(market_id());

        // Insert 1000 sell orders at price 100
        for i in 1..=1000u64 {
            let owner = unique_maker_address(i);
            let order = make_order_with_owner(i, OrderSide::Sell, 100, 1, owner);
            book.insert_order(order).unwrap();
        }

        let start = std::time::Instant::now();

        let buy_order = make_buy_order(1001, 100, 1000);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(1000),
            Timestamp::from_millis(1_001_000),
        );

        let elapsed = start.elapsed();

        assert_eq!(result.fills.len(), 1000);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);

        // Should complete in under 50ms (very conservative)
        assert!(
            elapsed.as_millis() < 50,
            "Matching 1000 orders took {elapsed:?}, expected < 50ms"
        );
    }

    #[test]
    fn perf_available_quantity_deep_book() {
        // Performance test: calculate available quantity in deep book
        let mut book = OrderBook::new(market_id());

        // Insert 10000 orders across 100 levels
        let mut seq = 1u64;
        for level in 0..100u64 {
            for _ in 0..100 {
                let owner = unique_maker_address(seq);
                let order = make_order_with_owner(seq, OrderSide::Sell, 100 + level, 1, owner);
                book.insert_order(order).unwrap();
                seq += 1;
            }
        }

        let start = std::time::Instant::now();

        // Calculate available quantity multiple times
        for _ in 0..100 {
            let _ = available_quantity(&book, OrderSide::Buy, Price::from_whole(199));
        }

        let elapsed = start.elapsed();

        // 100 iterations should complete in under 50ms
        assert!(
            elapsed.as_millis() < 50,
            "100 available_quantity calls took {elapsed:?}, expected < 50ms"
        );
    }

    #[test]
    fn perf_would_cross_check() {
        // Performance test: rapid would_cross checks
        let mut book = OrderBook::new(market_id());

        // Build a book with some depth
        for i in 1..=100u64 {
            let owner = unique_maker_address(i);
            book.insert_order(make_order_with_owner(
                i,
                OrderSide::Sell,
                100 + i,
                10,
                owner,
            ))
            .unwrap();
            let owner2 = unique_maker_address(i + 1000);
            book.insert_order(make_order_with_owner(
                i + 100,
                OrderSide::Buy,
                99 - (i % 50),
                10,
                owner2,
            ))
            .unwrap();
        }

        let start = std::time::Instant::now();

        // Perform 10000 would_cross checks
        for i in 0..10000u64 {
            let price = Price::from_whole(50 + (i % 100));
            let _ = would_cross(&book, OrderSide::Buy, price);
            let _ = would_cross(&book, OrderSide::Sell, price);
        }

        let elapsed = start.elapsed();

        // 20000 checks should complete in under 50ms
        assert!(
            elapsed.as_millis() < 50,
            "20000 would_cross checks took {elapsed:?}, expected < 50ms"
        );
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn edge_case_zero_quantity_fill() {
        // Edge case: order with zero remaining quantity
        let mut book = OrderBook::new(market_id());
        book.insert_order(make_sell_order(1, 100, 10)).unwrap();

        // Try to match with zero quantity
        let buy_order = make_buy_order(2, 100, 10);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::ZERO, // Zero max quantity
            Timestamp::from_millis(2000),
        );

        assert!(result.fills.is_empty());
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
    }

    #[test]
    fn edge_case_exact_level_fill() {
        // Edge case: order exactly fills one level, nothing left
        let mut book = OrderBook::new(market_id());

        // Two levels with exact quantities
        let owner1 = unique_maker_address(1);
        let owner2 = unique_maker_address(2);
        book.insert_order(make_order_with_owner(1, OrderSide::Sell, 100, 50, owner1))
            .unwrap();
        book.insert_order(make_order_with_owner(2, OrderSide::Sell, 101, 50, owner2))
            .unwrap();

        // Buy exactly 50 - should fill level 100 exactly
        let buy_order = make_buy_order(3, 101, 50);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(50),
            Timestamp::from_millis(3000),
        );

        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.remaining_quantity, Quantity::ZERO);
        assert_eq!(result.consumed_orders.len(), 1);
        assert!(result.partial_fills.is_empty());
    }

    #[test]
    fn edge_case_price_boundary() {
        // Edge case: test exact price boundary matching
        let mut book = OrderBook::new(market_id());
        let owner = unique_maker_address(1);

        // Sell at exactly 100
        book.insert_order(make_order_with_owner(1, OrderSide::Sell, 100, 10, owner))
            .unwrap();

        // Buy at exactly 100 - should match
        let buy_at_100 = make_buy_order(2, 100, 5);
        let result = match_order(
            &book,
            &buy_at_100,
            Quantity::from_whole(5),
            Timestamp::from_millis(2000),
        );
        assert_eq!(result.fills.len(), 1);

        // Buy at 99 - should NOT match
        let buy_at_99 = make_buy_order(3, 99, 5);
        let result = match_order(
            &book,
            &buy_at_99,
            Quantity::from_whole(5),
            Timestamp::from_millis(3000),
        );
        assert!(result.fills.is_empty());
    }

    #[test]
    fn edge_case_all_orders_same_owner_as_taker() {
        // Edge case: all resting orders belong to the taker
        let mut book = OrderBook::new(market_id());
        let owner = taker_address();

        for i in 1..=5 {
            book.insert_order(make_order_with_owner(i, OrderSide::Sell, 100, 10, owner))
                .unwrap();
        }

        let buy_order = make_buy_order(6, 100, 50);
        let result = match_order(
            &book,
            &buy_order,
            Quantity::from_whole(50),
            Timestamp::from_millis(6000),
        );

        // No fills due to self-trade prevention
        assert!(result.fills.is_empty());
        assert_eq!(result.remaining_quantity, Quantity::from_whole(50));
    }

    #[test]
    fn edge_case_interleaved_matching() {
        // Edge case: matching with multiple takers interleaved
        let mut book = OrderBook::new(market_id());

        // Add 10 sells at price 100
        for i in 1..=10u64 {
            let owner = unique_maker_address(i);
            book.insert_order(make_order_with_owner(i, OrderSide::Sell, 100, 10, owner))
                .unwrap();
        }

        // First taker takes 25
        let buy1 = make_buy_order(11, 100, 25);
        let result1 = match_order(
            &book,
            &buy1,
            Quantity::from_whole(25),
            Timestamp::from_millis(11000),
        );
        assert_eq!(result1.fills.len(), 3); // 10 + 10 + 5
        assert_eq!(result1.consumed_orders.len(), 2);
        assert_eq!(result1.partial_fills.len(), 1);

        // Simulate applying the result (in real usage, engine would do this)
        // For this test, we just verify the math
        let total_filled: u128 = result1.fills.iter().map(|f| f.quantity.as_scaled()).sum();
        assert_eq!(total_filled, Quantity::from_whole(25).as_scaled());
    }
}
