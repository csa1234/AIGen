// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::crypto::{hash_transaction, sign_message, verify_signature, PublicKey, SecretKey};
use crate::types::{
    validate_address, Amount, BlockchainError, ChainId, Fee, Nonce, Timestamp, TxHash,
};
use ed25519_dalek::Signature;
use genesis::CeoTransactable;
use genesis::GenesisError;
use genesis::{check_shutdown, CeoSignature, CEO_WALLET};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: Amount,
    pub signature: Signature,
    pub timestamp: Timestamp,
    pub nonce: Nonce,
    pub priority: bool,
    pub tx_hash: TxHash,
    pub fee: Fee,
    pub chain_id: ChainId,
    pub payload: Option<Vec<u8>>,
    pub ceo_signature: Option<CeoSignature>,
}

impl Transaction {
    /// Construct a transaction bound to a specific chain.
    ///
    /// This initializes a placeholder signature for unsigned transaction creation.
    /// Call `sign()` before submitting a non-CEO transaction to a chain.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sender: String,
        receiver: String,
        amount: u64,
        timestamp: i64,
        nonce: u64,
        priority: bool,
        chain_id: ChainId,
        payload: Option<Vec<u8>>,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;

        let dummy_signature: Signature = Signature::from([0u8; 64]);

        let fee = Self::calculate_fee(
            &sender, &receiver, amount, timestamp, nonce, priority, chain_id,
        );

        let mut tx = Transaction {
            sender,
            receiver,
            amount: Amount::new(amount),
            signature: dummy_signature,
            timestamp: Timestamp(timestamp),
            nonce: Nonce::new(nonce),
            priority,
            tx_hash: TxHash([0u8; 32]),
            fee,
            chain_id,
            payload,
            ceo_signature: None,
        };
        tx.update_hash();
        Ok(tx)
    }

    /// Convenience constructor using the default genesis chain id.
    pub fn new_default_chain(
        sender: String,
        receiver: String,
        amount: u64,
        timestamp: i64,
        nonce: u64,
        priority: bool,
        payload: Option<Vec<u8>>,
    ) -> Result<Self, GenesisError> {
        let chain_id = ChainId::from_str_id(&genesis::GenesisConfig::default().chain_id);
        Self::new(
            sender, receiver, amount, timestamp, nonce, priority, chain_id, payload,
        )
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_transaction(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        // The tx_hash already commits to chain_id.
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn verify(&self, public_key: &PublicKey) -> bool {
        verify_signature(self.tx_hash.0.as_ref(), &self.signature, public_key)
    }

    pub fn verify_with_pubkey(&self, public_key: &PublicKey) -> Result<(), BlockchainError> {
        self.validate()?;
        if !self.verify(public_key) {
            return Err(BlockchainError::InvalidSignature);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.sender)?;
        validate_address(&self.receiver)?;

        if self.sender == self.receiver {
            return Err(BlockchainError::InvalidTransaction);
        }

        if self.amount.is_zero() {
            return Err(BlockchainError::InvalidAmount);
        }

        // Fee must be non-negative; base fee can be zero for CEO.
        let total_fee = self.fee.total_fee();
        if total_fee.is_zero() && !self.is_ceo_transaction() {
            return Err(BlockchainError::InvalidFee);
        }

        // Timestamp sanity is checked at block level as well; keep a basic non-negative invariant.
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }

        Ok(())
    }

    pub fn total_cost(&self) -> Result<Amount, BlockchainError> {
        self.amount
            .safe_add(self.fee.base_fee)?
            .safe_add(self.fee.priority_fee)
    }

    pub fn calculate_fee(
        sender: &str,
        receiver: &str,
        amount: u64,
        timestamp: i64,
        nonce: u64,
        priority: bool,
        chain_id: ChainId,
    ) -> Fee {
        // Placeholder: fee based on serialized tx size (without signatures) and priority.
        let payload = serde_json::json!({
            "sender": sender,
            "receiver": receiver,
            "amount": amount,
            "timestamp": timestamp,
            "nonce": nonce,
            "priority": priority,
            "chain_id": chain_id.value(),
        });

        let size = serde_json::to_vec(&payload)
            .map(|b| b.len() as u64)
            .unwrap_or(0);
        let base_fee = Amount::new(size.saturating_div(10).saturating_add(1));
        let priority_fee = if priority {
            Amount::new(size.saturating_div(20).saturating_add(1))
        } else {
            Amount::ZERO
        };

        // CEO transactions can be fee-free.
        if sender == CEO_WALLET {
            Fee::new(Amount::ZERO, Amount::ZERO)
        } else {
            Fee::new(base_fee, priority_fee)
        }
    }

    pub fn is_ceo_transaction(&self) -> bool {
        self.priority && self.sender == CEO_WALLET
    }
}

impl CeoTransactable for Transaction {
    fn sender_address(&self) -> &str {
        &self.sender
    }

    fn is_priority(&self) -> bool {
        self.priority
    }

    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }

    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

#[derive(Default, Debug)]
pub struct TransactionPool {
    pending: VecDeque<Transaction>,
}

impl TransactionPool {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn push(&mut self, tx: Transaction) {
        self.pending.push_back(tx);
        self.reorder();
    }

    pub fn pop_next(&mut self) -> Option<Transaction> {
        self.pending.pop_front()
    }

    pub fn peek_n(&self, limit: usize) -> Vec<Transaction> {
        self.pending.iter().take(limit).cloned().collect()
    }

    pub fn contains(&self, hash: &TxHash) -> bool {
        self.pending.iter().any(|t| &t.tx_hash == hash)
    }

    pub fn remove_by_hash(&mut self, hash: &TxHash) -> Option<Transaction> {
        if let Some(idx) = self.pending.iter().position(|t| &t.tx_hash == hash) {
            return self.pending.remove(idx);
        }
        None
    }

    pub fn get_by_sender(&self, sender: &str) -> Vec<&Transaction> {
        self.pending.iter().filter(|t| t.sender == sender).collect()
    }

    pub fn validate_pool(&self) -> Result<(), BlockchainError> {
        let mut seen: HashMap<&str, HashMap<u64, ()>> = HashMap::new();
        for tx in self.pending.iter() {
            let entry = seen.entry(&tx.sender).or_default();
            if entry.insert(tx.nonce.value(), ()).is_some() {
                return Err(BlockchainError::DuplicateNonce);
            }
        }
        Ok(())
    }

    fn reorder(&mut self) {
        let mut v: Vec<Transaction> = self.pending.drain(..).collect();
        v.sort_by(|a, b| {
            // CEO txs first
            match (a.is_ceo_transaction(), b.is_ceo_transaction()) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
            // Then higher priority_fee first
            b.fee.priority_fee.value().cmp(&a.fee.priority_fee.value())
        });
        self.pending = v.into();
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
