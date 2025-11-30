//! Account and balance types.
//!
//! This module defines types for tracking user accounts and balances:
//! - [`Balance`]: Available and locked amounts for a single token
//! - [`Account`]: A user's complete account state including balances
//!
//! # Balance Model
//!
//! Each token balance is split into:
//! - **Available**: Free to withdraw or use for new orders
//! - **Locked**: Reserved for open orders (released when filled or cancelled)
//!
//! The total balance is always: `total = available + locked`

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::{Address, Nonce, Quantity};

// =============================================================================
// Balance
// =============================================================================

/// Balance for a single token, split into available and locked portions.
///
/// Invariant: `total = available + locked`
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Balance {
    /// Amount available for new orders or withdrawal.
    pub available: Quantity,

    /// Amount locked in open orders.
    pub locked: Quantity,
}

impl Balance {
    /// Creates a new balance with the given available amount and zero locked.
    #[must_use]
    pub const fn new(available: Quantity) -> Self {
        Self {
            available,
            locked: Quantity::ZERO,
        }
    }

    /// Creates a zero balance.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            available: Quantity::ZERO,
            locked: Quantity::ZERO,
        }
    }

    /// Returns the total balance (available + locked).
    #[must_use]
    pub fn total(&self) -> Quantity {
        self.available
            .checked_add(self.locked)
            .unwrap_or(Quantity::MAX)
    }

    /// Returns `true` if both available and locked are zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.available.is_zero() && self.locked.is_zero()
    }

    /// Deposits funds, increasing the available balance.
    ///
    /// Returns `None` on overflow.
    #[must_use]
    pub fn deposit(&self, amount: Quantity) -> Option<Self> {
        Some(Self {
            available: self.available.checked_add(amount)?,
            locked: self.locked,
        })
    }

    /// Withdraws funds from the available balance.
    ///
    /// Returns `None` if insufficient available balance.
    #[must_use]
    pub fn withdraw(&self, amount: Quantity) -> Option<Self> {
        Some(Self {
            available: self.available.checked_sub(amount)?,
            locked: self.locked,
        })
    }

    /// Locks funds for an order (moves from available to locked).
    ///
    /// Returns `None` if insufficient available balance.
    #[must_use]
    pub fn lock(&self, amount: Quantity) -> Option<Self> {
        Some(Self {
            available: self.available.checked_sub(amount)?,
            locked: self.locked.checked_add(amount)?,
        })
    }

    /// Unlocks funds (moves from locked to available).
    ///
    /// This is called when an order is cancelled.
    /// Returns `None` if insufficient locked balance.
    #[must_use]
    pub fn unlock(&self, amount: Quantity) -> Option<Self> {
        Some(Self {
            available: self.available.checked_add(amount)?,
            locked: self.locked.checked_sub(amount)?,
        })
    }

    /// Consumes locked funds (removes from locked without returning to available).
    ///
    /// This is called when an order is filled.
    /// Returns `None` if insufficient locked balance.
    #[must_use]
    pub fn consume_locked(&self, amount: Quantity) -> Option<Self> {
        Some(Self {
            available: self.available,
            locked: self.locked.checked_sub(amount)?,
        })
    }

    /// Receives funds (adds to available).
    ///
    /// This is called when receiving proceeds from a trade.
    /// Returns `None` on overflow.
    #[must_use]
    pub fn receive(&self, amount: Quantity) -> Option<Self> {
        self.deposit(amount)
    }

    /// Checks if the given amount can be locked (sufficient available balance).
    #[must_use]
    pub fn can_lock(&self, amount: Quantity) -> bool {
        self.available.as_scaled() >= amount.as_scaled()
    }

    /// Checks if the given amount can be withdrawn (sufficient available balance).
    #[must_use]
    pub fn can_withdraw(&self, amount: Quantity) -> bool {
        self.available.as_scaled() >= amount.as_scaled()
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "available={}, locked={}, total={}",
            self.available,
            self.locked,
            self.total()
        )
    }
}

// =============================================================================
// Account
// =============================================================================

/// A user's trading account containing balances and state.
///
/// Each account is identified by an Ethereum address and contains:
/// - Balances for each token (keyed by token address)
/// - The current nonce for replay protection
///
/// # Example
///
/// ```
/// use types::{Account, Address, Quantity, Nonce};
///
/// let owner = Address::new([1u8; 20]);
/// let mut account = Account::new(owner);
///
/// let token = Address::new([2u8; 20]);
/// account.deposit(token, Quantity::from_whole(100)).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Account {
    /// The account owner's address.
    pub owner: Address,

    /// Balances for each token (token address -> balance).
    pub balances: BTreeMap<Address, Balance>,

    /// Current nonce for replay protection.
    ///
    /// Each order must have a nonce >= this value.
    /// The nonce is incremented after each order is processed.
    pub nonce: Nonce,
}

impl Account {
    /// Creates a new empty account.
    #[must_use]
    pub fn new(owner: Address) -> Self {
        Self {
            owner,
            balances: BTreeMap::new(),
            nonce: Nonce::new(0),
        }
    }

    /// Returns the balance for a token, or zero if not present.
    #[must_use]
    pub fn get_balance(&self, token: Address) -> Balance {
        self.balances.get(&token).copied().unwrap_or_default()
    }

    /// Returns the available balance for a token.
    #[must_use]
    pub fn available_balance(&self, token: Address) -> Quantity {
        self.get_balance(token).available
    }

    /// Returns the locked balance for a token.
    #[must_use]
    pub fn locked_balance(&self, token: Address) -> Quantity {
        self.get_balance(token).locked
    }

    /// Returns the total balance for a token.
    #[must_use]
    pub fn total_balance(&self, token: Address) -> Quantity {
        self.get_balance(token).total()
    }

    /// Deposits tokens into the account.
    ///
    /// Returns `Err` if the operation would overflow.
    pub fn deposit(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        let balance = self.get_balance(token);
        let new_balance = balance
            .deposit(amount)
            .ok_or(AccountError::BalanceOverflow)?;
        self.balances.insert(token, new_balance);
        Ok(())
    }

    /// Withdraws tokens from the account.
    ///
    /// Returns `Err` if insufficient available balance.
    pub fn withdraw(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        let balance = self.get_balance(token);
        let new_balance = balance
            .withdraw(amount)
            .ok_or(AccountError::InsufficientBalance)?;
        if new_balance.is_zero() {
            self.balances.remove(&token);
        } else {
            self.balances.insert(token, new_balance);
        }
        Ok(())
    }

    /// Locks tokens for an order.
    ///
    /// Returns `Err` if insufficient available balance.
    pub fn lock(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        let balance = self.get_balance(token);
        let new_balance = balance
            .lock(amount)
            .ok_or(AccountError::InsufficientBalance)?;
        self.balances.insert(token, new_balance);
        Ok(())
    }

    /// Unlocks tokens (order cancelled).
    ///
    /// Returns `Err` if insufficient locked balance.
    pub fn unlock(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        let balance = self.get_balance(token);
        let new_balance = balance
            .unlock(amount)
            .ok_or(AccountError::InsufficientLockedBalance)?;
        self.balances.insert(token, new_balance);
        Ok(())
    }

    /// Consumes locked tokens (order filled).
    pub fn consume_locked(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        let balance = self.get_balance(token);
        let new_balance = balance
            .consume_locked(amount)
            .ok_or(AccountError::InsufficientLockedBalance)?;
        if new_balance.is_zero() {
            self.balances.remove(&token);
        } else {
            self.balances.insert(token, new_balance);
        }
        Ok(())
    }

    /// Receives tokens (proceeds from a trade).
    pub fn receive(&mut self, token: Address, amount: Quantity) -> Result<(), AccountError> {
        self.deposit(token, amount)
    }

    /// Checks if a nonce is valid for this account.
    ///
    /// A nonce is valid if it is greater than or equal to the current nonce.
    #[must_use]
    pub fn is_valid_nonce(&self, nonce: Nonce) -> bool {
        nonce.is_valid(self.nonce)
    }

    /// Increments the account nonce.
    ///
    /// Call this after successfully processing an order.
    pub fn increment_nonce(&mut self) {
        self.nonce = self.nonce.next();
    }

    /// Sets the nonce to a specific value.
    ///
    /// Use with caution - this should only be called during state reconstruction
    /// or if you're certain the nonce should be updated to this value.
    pub fn set_nonce(&mut self, nonce: Nonce) {
        self.nonce = nonce;
    }

    /// Returns the list of all tokens this account holds.
    pub fn tokens(&self) -> impl Iterator<Item = &Address> {
        self.balances.keys()
    }

    /// Returns the number of different tokens held.
    #[must_use]
    pub fn token_count(&self) -> usize {
        self.balances.len()
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Account({} nonce={} tokens={})",
            self.owner,
            self.nonce,
            self.balances.len()
        )
    }
}

// =============================================================================
// AccountError
// =============================================================================

/// Errors that can occur during account operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountError {
    /// Insufficient available balance for the operation.
    InsufficientBalance,

    /// Insufficient locked balance (internal error).
    InsufficientLockedBalance,

    /// Balance would overflow.
    BalanceOverflow,

    /// Invalid nonce (too low).
    InvalidNonce,
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientBalance => write!(f, "insufficient available balance"),
            Self::InsufficientLockedBalance => write!(f, "insufficient locked balance"),
            Self::BalanceOverflow => write!(f, "balance overflow"),
            Self::InvalidNonce => write!(f, "invalid nonce"),
        }
    }
}

impl std::error::Error for AccountError {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address(seed: u8) -> Address {
        Address::new([seed; 20])
    }

    mod balance_tests {
        use super::*;

        #[test]
        fn test_new() {
            let balance = Balance::new(Quantity::from_whole(100));
            assert_eq!(balance.available, Quantity::from_whole(100));
            assert_eq!(balance.locked, Quantity::ZERO);
            assert_eq!(balance.total(), Quantity::from_whole(100));
        }

        #[test]
        fn test_zero() {
            let balance = Balance::zero();
            assert!(balance.is_zero());
            assert_eq!(balance.total(), Quantity::ZERO);
        }

        #[test]
        fn test_deposit() {
            let balance = Balance::new(Quantity::from_whole(100));
            let new_balance = balance.deposit(Quantity::from_whole(50)).unwrap();
            assert_eq!(new_balance.available, Quantity::from_whole(150));
        }

        #[test]
        fn test_withdraw() {
            let balance = Balance::new(Quantity::from_whole(100));
            let new_balance = balance.withdraw(Quantity::from_whole(30)).unwrap();
            assert_eq!(new_balance.available, Quantity::from_whole(70));
        }

        #[test]
        fn test_withdraw_insufficient() {
            let balance = Balance::new(Quantity::from_whole(100));
            assert!(balance.withdraw(Quantity::from_whole(150)).is_none());
        }

        #[test]
        fn test_lock() {
            let balance = Balance::new(Quantity::from_whole(100));
            let new_balance = balance.lock(Quantity::from_whole(40)).unwrap();
            assert_eq!(new_balance.available, Quantity::from_whole(60));
            assert_eq!(new_balance.locked, Quantity::from_whole(40));
            assert_eq!(new_balance.total(), Quantity::from_whole(100));
        }

        #[test]
        fn test_lock_insufficient() {
            let balance = Balance::new(Quantity::from_whole(100));
            assert!(balance.lock(Quantity::from_whole(150)).is_none());
        }

        #[test]
        fn test_unlock() {
            let balance = Balance {
                available: Quantity::from_whole(60),
                locked: Quantity::from_whole(40),
            };
            let new_balance = balance.unlock(Quantity::from_whole(20)).unwrap();
            assert_eq!(new_balance.available, Quantity::from_whole(80));
            assert_eq!(new_balance.locked, Quantity::from_whole(20));
        }

        #[test]
        fn test_consume_locked() {
            let balance = Balance {
                available: Quantity::from_whole(60),
                locked: Quantity::from_whole(40),
            };
            let new_balance = balance.consume_locked(Quantity::from_whole(40)).unwrap();
            assert_eq!(new_balance.available, Quantity::from_whole(60));
            assert_eq!(new_balance.locked, Quantity::ZERO);
            assert_eq!(new_balance.total(), Quantity::from_whole(60));
        }

        #[test]
        fn test_can_lock() {
            let balance = Balance::new(Quantity::from_whole(100));
            assert!(balance.can_lock(Quantity::from_whole(50)));
            assert!(balance.can_lock(Quantity::from_whole(100)));
            assert!(!balance.can_lock(Quantity::from_whole(150)));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let balance = Balance {
                available: Quantity::from_whole(123),
                locked: Quantity::from_whole(45),
            };
            let bytes = borsh::to_vec(&balance).unwrap();
            let decoded: Balance = borsh::from_slice(&bytes).unwrap();
            assert_eq!(balance, decoded);
        }
    }

    mod account_tests {
        use super::*;

        #[test]
        fn test_new() {
            let owner = test_address(1);
            let account = Account::new(owner);
            assert_eq!(account.owner, owner);
            assert_eq!(account.nonce, Nonce::new(0));
            assert_eq!(account.token_count(), 0);
        }

        #[test]
        fn test_deposit_and_withdraw() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            assert_eq!(account.available_balance(token), Quantity::from_whole(100));

            account.withdraw(token, Quantity::from_whole(30)).unwrap();
            assert_eq!(account.available_balance(token), Quantity::from_whole(70));
        }

        #[test]
        fn test_lock_and_unlock() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            account.lock(token, Quantity::from_whole(40)).unwrap();

            assert_eq!(account.available_balance(token), Quantity::from_whole(60));
            assert_eq!(account.locked_balance(token), Quantity::from_whole(40));

            account.unlock(token, Quantity::from_whole(20)).unwrap();
            assert_eq!(account.available_balance(token), Quantity::from_whole(80));
            assert_eq!(account.locked_balance(token), Quantity::from_whole(20));
        }

        #[test]
        fn test_consume_locked() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            account.lock(token, Quantity::from_whole(40)).unwrap();
            account
                .consume_locked(token, Quantity::from_whole(40))
                .unwrap();

            assert_eq!(account.available_balance(token), Quantity::from_whole(60));
            assert_eq!(account.locked_balance(token), Quantity::ZERO);
            assert_eq!(account.total_balance(token), Quantity::from_whole(60));
        }

        #[test]
        fn test_nonce_management() {
            let owner = test_address(1);
            let mut account = Account::new(owner);

            assert!(account.is_valid_nonce(Nonce::new(0)));
            assert!(account.is_valid_nonce(Nonce::new(1)));

            account.increment_nonce();
            assert!(!account.is_valid_nonce(Nonce::new(0)));
            assert!(account.is_valid_nonce(Nonce::new(1)));
        }

        #[test]
        fn test_multiple_tokens() {
            let owner = test_address(1);
            let token1 = test_address(10);
            let token2 = test_address(20);
            let mut account = Account::new(owner);

            account.deposit(token1, Quantity::from_whole(100)).unwrap();
            account.deposit(token2, Quantity::from_whole(200)).unwrap();

            assert_eq!(account.token_count(), 2);
            assert_eq!(account.available_balance(token1), Quantity::from_whole(100));
            assert_eq!(account.available_balance(token2), Quantity::from_whole(200));
        }

        #[test]
        fn test_withdraw_all_removes_token() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            assert_eq!(account.token_count(), 1);

            account.withdraw(token, Quantity::from_whole(100)).unwrap();
            assert_eq!(account.token_count(), 0);
        }

        #[test]
        fn test_insufficient_balance_error() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            let result = account.withdraw(token, Quantity::from_whole(150));
            assert_eq!(result, Err(AccountError::InsufficientBalance));
        }

        #[test]
        fn test_borsh_roundtrip() {
            let owner = test_address(1);
            let token = test_address(10);
            let mut account = Account::new(owner);

            account.deposit(token, Quantity::from_whole(100)).unwrap();
            account.lock(token, Quantity::from_whole(30)).unwrap();
            account.increment_nonce();

            let bytes = borsh::to_vec(&account).unwrap();
            let decoded: Account = borsh::from_slice(&bytes).unwrap();
            assert_eq!(account, decoded);
        }
    }

    mod account_error_tests {
        use super::*;

        #[test]
        fn test_display() {
            assert_eq!(
                format!("{}", AccountError::InsufficientBalance),
                "insufficient available balance"
            );
            assert_eq!(
                format!("{}", AccountError::BalanceOverflow),
                "balance overflow"
            );
        }

        #[test]
        fn test_is_error() {
            fn check_error<E: std::error::Error>(_e: E) {}
            check_error(AccountError::InsufficientBalance);
        }
    }
}
