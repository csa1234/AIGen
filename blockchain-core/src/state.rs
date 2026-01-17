use crate::crypto::hash_data;
use crate::transaction::Transaction;
use crate::types::{Address, Amount, Balance, BlockchainError, Nonce};
use genesis::CEO_WALLET;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub type StateRoot = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountState {
    pub balance: Balance,
    pub nonce: Nonce,
    pub is_contract: bool,
    pub code_hash: Option<[u8; 32]>,
}

impl Default for AccountState {
    fn default() -> Self {
        Self {
            balance: Balance::zero(),
            nonce: Nonce::ZERO,
            is_contract: false,
            code_hash: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct ChainState {
    accounts: RwLock<BTreeMap<Address, AccountState>>,
    validator_reward_address: RwLock<Address>,
}

impl Clone for ChainState {
    fn clone(&self) -> Self {
        let accounts = self.accounts.read().clone();
        let validator_reward_address = self.validator_reward_address.read().clone();
        ChainState {
            accounts: RwLock::new(accounts),
            validator_reward_address: RwLock::new(validator_reward_address),
        }
    }
}

impl ChainState {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(BTreeMap::new()),
            validator_reward_address: RwLock::new(CEO_WALLET.to_string()),
        }
    }

    pub fn snapshot(&self) -> BTreeMap<Address, AccountState> {
        self.accounts.read().clone()
    }

    pub fn restore(&self, snapshot: BTreeMap<Address, AccountState>) {
        *self.accounts.write() = snapshot;
    }

    pub fn set_validator_reward_address(&self, address: Address) {
        *self.validator_reward_address.write() = address;
    }

    pub fn get_balance(&self, address: &str) -> Balance {
        self.accounts
            .read()
            .get(address)
            .map(|a| a.balance)
            .unwrap_or_else(Balance::zero)
    }

    pub fn get_nonce(&self, address: &str) -> Nonce {
        self.accounts
            .read()
            .get(address)
            .map(|a| a.nonce)
            .unwrap_or(Nonce::ZERO)
    }

    pub fn get_account_state(&self, address: &str) -> AccountState {
        self.accounts
            .read()
            .get(address)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_balance(&self, address: Address, balance: Balance) {
        let mut accounts = self.accounts.write();
        let entry = accounts.entry(address).or_default();
        entry.balance = balance;
    }

    pub fn increment_nonce(&self, address: &str) -> Result<Nonce, BlockchainError> {
        let mut accounts = self.accounts.write();
        let entry = accounts.entry(address.to_string()).or_default();
        entry
            .nonce
            .checked_next()
            .ok_or(BlockchainError::InvalidNonce)?;
        entry.nonce.increment();
        Ok(entry.nonce)
    }

    pub fn transfer(&self, from: &str, to: &str, amount: Amount) -> Result<(), BlockchainError> {
        if amount.is_zero() {
            return Err(BlockchainError::InvalidAmount);
        }

        let mut accounts = self.accounts.write();

        let from_entry = accounts.entry(from.to_string()).or_default();
        from_entry.balance = from_entry
            .balance
            .safe_sub(amount)
            .map_err(|_| BlockchainError::InsufficientBalance)?;

        let to_entry = accounts.entry(to.to_string()).or_default();
        to_entry.balance = to_entry.balance.safe_add(amount)?;

        Ok(())
    }

    pub fn validate_transaction(&self, tx: &Transaction) -> Result<(), BlockchainError> {
        // CEO priority transactions are administrative overrides and may bypass
        // normal balance checks.
        if tx.is_ceo_transaction() {
            return Ok(());
        }

        let sender_balance = self.get_balance(&tx.sender).amount();
        let total_cost = tx.total_cost()?;
        if sender_balance.checked_sub(total_cost).is_none() {
            return Err(BlockchainError::InsufficientBalance);
        }

        let expected_nonce = self.get_nonce(&tx.sender);
        if tx.nonce != expected_nonce {
            return Err(BlockchainError::InvalidNonce);
        }

        Ok(())
    }

    pub fn apply_transaction(&self, tx: &Transaction) -> Result<(), BlockchainError> {
        self.validate_transaction(tx)?;

        let fee = tx.fee;
        let validator = self.validator_reward_address.read().clone();

        let sender_addr = tx.sender.clone();
        let receiver_addr = tx.receiver.clone();
        let validator_addr = validator;
        let dev_addr = CEO_WALLET.to_string();

        let involved = [sender_addr.clone(), receiver_addr.clone(), validator_addr.clone(), dev_addr.clone()];

        let mut accounts = self.accounts.write();
        let mut temp: HashMap<Address, AccountState> = HashMap::new();

        let get_state = |addr: &Address, accounts: &BTreeMap<Address, AccountState>, temp: &HashMap<Address, AccountState>| {
            temp.get(addr)
                .cloned()
                .or_else(|| accounts.get(addr).cloned())
                .unwrap_or_default()
        };

        // Apply debits/credits using a temp map to handle overlaps.
        let total_cost = tx.total_cost()?;
        {
            let mut sender_state = get_state(&sender_addr, &accounts, &temp);
            sender_state.balance = sender_state
                .balance
                .safe_sub(total_cost)
                .map_err(|_| BlockchainError::InsufficientBalance)?;
            sender_state.nonce.increment();
            temp.insert(sender_addr.clone(), sender_state);
        }

        {
            let mut receiver_state = get_state(&receiver_addr, &accounts, &temp);
            receiver_state.balance = receiver_state.balance.safe_add(tx.amount)?;
            temp.insert(receiver_addr.clone(), receiver_state);
        }

        {
            let mut validator_state = get_state(&validator_addr, &accounts, &temp);
            validator_state.balance = validator_state.balance.safe_add(fee.validator_share())?;
            temp.insert(validator_addr.clone(), validator_state);
        }

        {
            let mut dev_state = get_state(&dev_addr, &accounts, &temp);
            dev_state.balance = dev_state.balance.safe_add(fee.dev_share())?;
            temp.insert(dev_addr.clone(), dev_state);
        }

        for addr in involved.iter() {
            if let Some(state) = temp.get(addr) {
                accounts.insert(addr.clone(), state.clone());
            }
        }

        Ok(())
    }

    pub fn mint_tokens(&self, address: &str, amount: Amount) -> Result<(), BlockchainError> {
        if amount.is_zero() {
            return Ok(());
        }
        let mut accounts = self.accounts.write();
        let entry = accounts.entry(address.to_string()).or_default();
        entry.balance = entry.balance.safe_add(amount)?;
        Ok(())
    }

    pub fn burn_tokens(&self, address: &str, amount: Amount) -> Result<(), BlockchainError> {
        if amount.is_zero() {
            return Ok(());
        }
        let mut accounts = self.accounts.write();
        let entry = accounts.entry(address.to_string()).or_default();
        entry.balance = entry
            .balance
            .safe_sub(amount)
            .map_err(|_| BlockchainError::InsufficientBalance)?;
        Ok(())
    }

    pub fn apply_slash(&self, address: &str, amount: Amount) -> Result<(), BlockchainError> {
        if amount.is_zero() {
            return Ok(());
        }

        let burn = Amount::new(amount.value().saturating_mul(40) / 100);
        let dev = Amount::new(amount.value().saturating_sub(burn.value()));

        self.burn_tokens(address, burn)?;
        self.transfer(address, CEO_WALLET, dev)?;
        Ok(())
    }

    pub fn calculate_state_root(&self) -> StateRoot {
        let accounts = self.accounts.read();
        if accounts.is_empty() {
            return hash_data(&[]);
        }

        let mut leaves: Vec<[u8; 32]> = Vec::with_capacity(accounts.len());
        for (addr, state) in accounts.iter() {
            let mut buf = Vec::new();
            buf.extend_from_slice(addr.as_bytes());
            buf.extend_from_slice(&state.balance.amount().value().to_le_bytes());
            buf.extend_from_slice(&state.nonce.value().to_le_bytes());
            buf.push(state.is_contract as u8);
            if let Some(ch) = state.code_hash {
                buf.extend_from_slice(&ch);
            }
            leaves.push(hash_data(&buf));
        }

        while leaves.len() > 1 {
            let mut next = Vec::with_capacity((leaves.len() + 1) / 2);
            for pair in leaves.chunks(2) {
                let left = pair[0];
                let right = if pair.len() == 2 { pair[1] } else { pair[0] };
                let mut buf = Vec::with_capacity(64);
                buf.extend_from_slice(&left);
                buf.extend_from_slice(&right);
                next.push(hash_data(&buf));
            }
            leaves = next;
        }

        leaves[0]
    }
}
