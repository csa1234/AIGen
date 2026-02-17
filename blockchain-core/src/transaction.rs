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

/// Reward transaction type for distributing compute and storage rewards
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RewardTx {
    pub recipient: String,
    pub amount: Amount,
    pub reward_type: RewardType,
    pub task_id: Option<String>,
    pub timestamp: Timestamp,
    pub tx_hash: TxHash,
    pub ceo_signature: Option<CeoSignature>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RewardType {
    Compute,
    Storage,
    Training,
}

impl RewardTx {
    /// Create a new reward transaction
    pub fn new(
        recipient: String,
        amount: u64,
        reward_type: RewardType,
        task_id: Option<String>,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&recipient)?;

        if amount == 0 {
            return Err(GenesisError::InvalidAddress("Reward amount must be positive".to_string()));
        }

        let mut tx = RewardTx {
            recipient,
            amount: Amount::new(amount),
            reward_type,
            task_id,
            timestamp: Timestamp(timestamp),
            tx_hash: TxHash([0u8; 32]),
            ceo_signature: None,
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_reward_tx(self);
    }

    pub fn sign_with_ceo(mut self, ceo_signature: CeoSignature) -> Self {
        self.ceo_signature = Some(ceo_signature);
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.recipient)?;

        if self.amount.is_zero() {
            return Err(BlockchainError::InvalidAmount);
        }

        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }

        Ok(())
    }

    pub fn is_authorized(&self) -> bool {
        self.ceo_signature.is_some()
    }
}

impl CeoTransactable for RewardTx {
    fn sender_address(&self) -> &str {
        &self.recipient
    }

    fn is_priority(&self) -> bool {
        true
    }

    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }

    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

/// Hash a reward transaction for signing
pub fn hash_reward_tx(tx: &RewardTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.recipient.as_bytes());
    hasher.update(&tx.amount.value().to_le_bytes());
    hasher.update(format!("{:?}", tx.reward_type).as_bytes());
    if let Some(ref task_id) = tx.task_id {
        hasher.update(task_id.as_bytes());
    }
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Stake transaction - locks tokens for validator/compute participation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeTx {
    pub staker: String,
    pub amount: Amount,
    pub role: crate::state::StakeRole,
    pub timestamp: Timestamp,
    pub signature: Signature,
    pub tx_hash: TxHash,
}

impl StakeTx {
    pub fn new(
        staker: String,
        amount: u64,
        role: crate::state::StakeRole,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&staker)?;

        if amount == 0 {
            return Err(GenesisError::InvalidAddress("Stake amount must be positive".to_string()));
        }

        let mut tx = StakeTx {
            staker,
            amount: Amount::new(amount),
            role,
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_stake_tx(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.staker)?;
        if self.amount.is_zero() {
            return Err(BlockchainError::InvalidAmount);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

pub fn hash_stake_tx(tx: &StakeTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.staker.as_bytes());
    hasher.update(&tx.amount.value().to_le_bytes());
    hasher.update(format!("{:?}", tx.role).as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Unstake transaction - initiates unstaking with 7-day cooldown
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnstakeTx {
    pub staker: String,
    pub amount: Amount,
    pub timestamp: Timestamp,
    pub signature: Signature,
    pub tx_hash: TxHash,
}

impl UnstakeTx {
    pub const UNSTAKE_COOLDOWN_SECS: i64 = 7 * 24 * 60 * 60; // 7 days

    pub fn new(
        staker: String,
        amount: u64,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&staker)?;

        if amount == 0 {
            return Err(GenesisError::InvalidAddress("Unstake amount must be positive".to_string()));
        }

        let mut tx = UnstakeTx {
            staker,
            amount: Amount::new(amount),
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_unstake_tx(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.staker)?;
        if self.amount.is_zero() {
            return Err(BlockchainError::InvalidAmount);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

pub fn hash_unstake_tx(tx: &UnstakeTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.staker.as_bytes());
    hasher.update(&tx.amount.value().to_le_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Claim unstaked tokens after cooldown period
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimStakeTx {
    pub staker: String,
    pub timestamp: Timestamp,
    pub signature: Signature,
    pub tx_hash: TxHash,
}

impl ClaimStakeTx {
    pub fn new(
        staker: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&staker)?;

        let mut tx = ClaimStakeTx {
            staker,
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_claim_stake_tx(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.staker)?;
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

pub fn hash_claim_stake_tx(tx: &ClaimStakeTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.staker.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Register as G8 Safety Committee candidate
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterG8CandidateTx {
    pub candidate: String,
    pub background_check_hash: [u8; 32],
    pub expertise_statement: String,
    pub timestamp: Timestamp,
    pub signature: Signature,
    pub tx_hash: TxHash,
}

impl RegisterG8CandidateTx {
    pub fn new(
        candidate: String,
        background_check_hash: [u8; 32],
        expertise_statement: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&candidate)?;

        if expertise_statement.len() > 500 {
            return Err(GenesisError::InvalidAddress("Expertise statement must be <= 500 chars".to_string()));
        }

        let mut tx = RegisterG8CandidateTx {
            candidate,
            background_check_hash,
            expertise_statement,
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_register_g8_tx(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.candidate)?;
        if self.expertise_statement.len() > 500 {
            return Err(BlockchainError::InvalidTransaction);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

pub fn hash_register_g8_tx(tx: &RegisterG8CandidateTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.candidate.as_bytes());
    hasher.update(&tx.background_check_hash);
    hasher.update(tx.expertise_statement.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Vote for G8 candidate
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoteG8Tx {
    pub voter: String,
    pub election_id: String,
    pub candidate_address: String,
    pub timestamp: Timestamp,
    pub signature: Signature,
    pub tx_hash: TxHash,
}

impl VoteG8Tx {
    pub fn new(
        voter: String,
        election_id: String,
        candidate_address: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&voter)?;
        validate_address(&candidate_address)?;

        let mut tx = VoteG8Tx {
            voter,
            election_id,
            candidate_address,
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_vote_g8_tx(self);
    }

    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.voter)?;
        validate_address(&self.candidate_address)?;
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

pub fn hash_vote_g8_tx(tx: &VoteG8Tx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.voter.as_bytes());
    hasher.update(tx.election_id.as_bytes());
    hasher.update(tx.candidate_address.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Record G8 meeting (CEO-signed)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordG8MeetingTx {
    pub meeting_id: String,
    pub attendees: Vec<String>,
    pub minutes_ipfs_hash: String,
    pub timestamp: Timestamp,
    pub ceo_signature: Option<CeoSignature>,
    pub tx_hash: TxHash,
}

impl RecordG8MeetingTx {
    pub fn new(
        meeting_id: String,
        attendees: Vec<String>,
        minutes_ipfs_hash: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;

        let mut tx = RecordG8MeetingTx {
            meeting_id,
            attendees,
            minutes_ipfs_hash,
            timestamp: Timestamp(timestamp),
            ceo_signature: None,
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_g8_meeting_tx(self);
    }

    pub fn sign_with_ceo(mut self, ceo_signature: CeoSignature) -> Self {
        self.ceo_signature = Some(ceo_signature);
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        for attendee in &self.attendees {
            validate_address(attendee)?;
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

impl CeoTransactable for RecordG8MeetingTx {
    fn sender_address(&self) -> &str {
        CEO_WALLET
    }

    fn is_priority(&self) -> bool {
        true
    }

    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }

    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

pub fn hash_g8_meeting_tx(tx: &RecordG8MeetingTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.meeting_id.as_bytes());
    for attendee in &tx.attendees {
        hasher.update(attendee.as_bytes());
    }
    hasher.update(tx.minutes_ipfs_hash.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Remove G8 member (CEO-signed)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoveG8MemberTx {
    pub member_address: String,
    pub reason: String,
    pub timestamp: Timestamp,
    pub ceo_signature: Option<CeoSignature>,
    pub tx_hash: TxHash,
}

impl RemoveG8MemberTx {
    pub fn new(
        member_address: String,
        reason: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        validate_address(&member_address)?;

        if reason.len() < 50 {
            return Err(GenesisError::InvalidAddress("Removal reason must be >= 50 chars".to_string()));
        }

        let mut tx = RemoveG8MemberTx {
            member_address,
            reason,
            timestamp: Timestamp(timestamp),
            ceo_signature: None,
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }

    fn update_hash(&mut self) {
        self.tx_hash = hash_remove_g8_tx(self);
    }

    pub fn sign_with_ceo(mut self, ceo_signature: CeoSignature) -> Self {
        self.ceo_signature = Some(ceo_signature);
        self
    }

    pub fn validate(&self) -> Result<(), BlockchainError> {
        validate_address(&self.member_address)?;
        if self.reason.len() < 50 {
            return Err(BlockchainError::InvalidTransaction);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

impl CeoTransactable for RemoveG8MemberTx {
    fn sender_address(&self) -> &str {
        CEO_WALLET
    }

    fn is_priority(&self) -> bool {
        true
    }

    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }

    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

pub fn hash_remove_g8_tx(tx: &RemoveG8MemberTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.member_address.as_bytes());
    hasher.update(tx.reason.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Register a model on-chain (CEO-signed)
/// 
/// This transaction registers a new AI model in the on-chain registry.
/// Only the CEO can submit this transaction, ensuring controlled model deployment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterModelTx {
    /// Unique model identifier
    pub model_id: String,
    /// Model name
    pub name: String,
    /// Model version
    pub version: String,
    /// IPFS CID where model weights are stored
    pub ipfs_cid: String,
    /// SHA3-256 hash of model weights for verification
    pub weights_hash: [u8; 32],
    /// Model architecture type (e.g., "llama4", "kimi-k2.5")
    pub architecture: String,
    /// Minimum subscription tier required (0=Free, 1=Basic, 2=Pro, 3=Enterprise)
    pub minimum_tier: u8,
    /// Whether this is a core model (required for network bootstrap)
    pub is_core: bool,
    /// Number of shards for distributed inference
    pub shard_count: u32,
    /// Transaction timestamp
    pub timestamp: Timestamp,
    /// CEO signature authorizing this registration
    pub ceo_signature: Option<CeoSignature>,
    /// Transaction hash
    pub tx_hash: TxHash,
}

impl RegisterModelTx {
    /// Create a new model registration transaction
    pub fn new(
        model_id: String,
        name: String,
        version: String,
        ipfs_cid: String,
        weights_hash: [u8; 32],
        architecture: String,
        minimum_tier: u8,
        is_core: bool,
        shard_count: u32,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        
        if model_id.is_empty() || name.is_empty() || version.is_empty() {
            return Err(GenesisError::InvalidAddress("Model fields cannot be empty".to_string()));
        }
        
        let mut tx = RegisterModelTx {
            model_id,
            name,
            version,
            ipfs_cid,
            weights_hash,
            architecture,
            minimum_tier,
            is_core,
            shard_count,
            timestamp: Timestamp(timestamp),
            ceo_signature: None,
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }
    
    fn update_hash(&mut self) {
        self.tx_hash = hash_register_model_tx(self);
    }
    
    /// Sign the transaction with CEO key
    pub fn sign_with_ceo(mut self, ceo_signature: CeoSignature) -> Self {
        self.ceo_signature = Some(ceo_signature);
        self
    }
    
    /// Validate the transaction
    pub fn validate(&self) -> Result<(), BlockchainError> {
        if self.model_id.is_empty() || self.name.is_empty() {
            return Err(BlockchainError::InvalidTransaction);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

impl CeoTransactable for RegisterModelTx {
    fn sender_address(&self) -> &str {
        CEO_WALLET
    }
    
    fn is_priority(&self) -> bool {
        true
    }
    
    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }
    
    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

/// Hash function for RegisterModelTx
pub fn hash_register_model_tx(tx: &RegisterModelTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.model_id.as_bytes());
    hasher.update(tx.name.as_bytes());
    hasher.update(tx.version.as_bytes());
    hasher.update(tx.ipfs_cid.as_bytes());
    hasher.update(&tx.weights_hash);
    hasher.update(tx.architecture.as_bytes());
    hasher.update(&[tx.minimum_tier]);
    hasher.update(&[tx.is_core as u8]);
    hasher.update(&tx.shard_count.to_le_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Update constitution (CEO-signed)
/// 
/// This transaction updates the on-chain constitution with a new version.
/// Requires CEO signature and a detailed reason for the update (minimum 100 chars).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateConstitutionTx {
    /// Constitution version number
    pub version: u32,
    /// IPFS hash of the constitution document
    pub ipfs_hash: String,
    /// SHA3-256 hash of the principles for verification
    pub principles_hash: [u8; 32],
    /// Detailed reason for the update (minimum 100 characters)
    pub reason: String,
    /// Transaction timestamp
    pub timestamp: Timestamp,
    /// CEO signature authorizing this update
    pub ceo_signature: Option<CeoSignature>,
    /// Transaction hash
    pub tx_hash: TxHash,
}

impl UpdateConstitutionTx {
    /// Create a new constitution update transaction
    pub fn new(
        version: u32,
        ipfs_hash: String,
        principles_hash: [u8; 32],
        reason: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        
        if reason.len() < 100 {
            return Err(GenesisError::InvalidAddress("Constitution update reason must be >= 100 chars".to_string()));
        }
        
        let mut tx = UpdateConstitutionTx {
            version,
            ipfs_hash,
            principles_hash,
            reason,
            timestamp: Timestamp(timestamp),
            ceo_signature: None,
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }
    
    fn update_hash(&mut self) {
        self.tx_hash = hash_update_constitution_tx(self);
    }
    
    /// Sign the transaction with CEO key
    pub fn sign_with_ceo(mut self, ceo_signature: CeoSignature) -> Self {
        self.ceo_signature = Some(ceo_signature);
        self
    }
    
    /// Validate the transaction
    pub fn validate(&self) -> Result<(), BlockchainError> {
        if self.reason.len() < 100 {
            return Err(BlockchainError::InvalidTransaction);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

impl CeoTransactable for UpdateConstitutionTx {
    fn sender_address(&self) -> &str {
        CEO_WALLET
    }
    
    fn is_priority(&self) -> bool {
        true
    }
    
    fn ceo_signature(&self) -> Option<&CeoSignature> {
        self.ceo_signature.as_ref()
    }
    
    fn message_to_sign(&self) -> Vec<u8> {
        self.tx_hash.0.to_vec()
    }
}

/// Hash function for UpdateConstitutionTx
pub fn hash_update_constitution_tx(tx: &UpdateConstitutionTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(&tx.version.to_le_bytes());
    hasher.update(tx.ipfs_hash.as_bytes());
    hasher.update(&tx.principles_hash);
    hasher.update(tx.reason.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// Record a training round with PoI proof
/// 
/// This transaction records a federated learning training round on-chain,
/// requiring a valid PoI proof for verification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordTrainingRoundTx {
    /// Training round ID
    pub round_id: String,
    /// Model ID being trained
    pub model_id: String,
    /// Training round version
    pub version: u64,
    /// IPFS CID of the trained model weights
    pub weights_cid: String,
    /// SHA3-256 hash of the weights
    pub weights_hash: [u8; 32],
    /// Number of participating nodes
    pub participant_count: u32,
    /// Training loss achieved
    pub final_loss: f64,
    /// PoI proof JSON (serialized)
    pub poi_proof: String,
    /// Transaction timestamp
    pub timestamp: Timestamp,
    /// Signature from the training coordinator
    pub signature: Signature,
    /// Transaction hash
    pub tx_hash: TxHash,
}

impl RecordTrainingRoundTx {
    /// Create a new training round transaction
    pub fn new(
        round_id: String,
        model_id: String,
        version: u64,
        weights_cid: String,
        weights_hash: [u8; 32],
        participant_count: u32,
        final_loss: f64,
        poi_proof: String,
        timestamp: i64,
    ) -> Result<Self, GenesisError> {
        check_shutdown()?;
        
        if round_id.is_empty() || model_id.is_empty() {
            return Err(GenesisError::InvalidAddress("Round ID and model ID cannot be empty".to_string()));
        }
        
        let mut tx = RecordTrainingRoundTx {
            round_id,
            model_id,
            version,
            weights_cid,
            weights_hash,
            participant_count,
            final_loss,
            poi_proof,
            timestamp: Timestamp(timestamp),
            signature: Signature::from([0u8; 64]),
            tx_hash: TxHash([0u8; 32]),
        };
        tx.update_hash();
        Ok(tx)
    }
    
    fn update_hash(&mut self) {
        self.tx_hash = hash_record_training_round_tx(self);
    }
    
    /// Sign the transaction
    pub fn sign(mut self, secret_key: &SecretKey) -> Self {
        let signature = sign_message(self.tx_hash.0.as_ref(), secret_key);
        self.signature = signature;
        self
    }
    
    /// Validate the transaction
    pub fn validate(&self) -> Result<(), BlockchainError> {
        if self.round_id.is_empty() || self.model_id.is_empty() {
            return Err(BlockchainError::InvalidTransaction);
        }
        if self.timestamp.value() < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        if self.poi_proof.is_empty() {
            return Err(BlockchainError::InvalidTransaction);
        }
        Ok(())
    }
}

/// Hash function for RecordTrainingRoundTx
pub fn hash_record_training_round_tx(tx: &RecordTrainingRoundTx) -> TxHash {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(tx.round_id.as_bytes());
    hasher.update(tx.model_id.as_bytes());
    hasher.update(&tx.version.to_le_bytes());
    hasher.update(tx.weights_cid.as_bytes());
    hasher.update(&tx.weights_hash);
    hasher.update(&tx.participant_count.to_le_bytes());
    hasher.update(&tx.final_loss.to_le_bytes());
    hasher.update(tx.poi_proof.as_bytes());
    hasher.update(&tx.timestamp.value().to_le_bytes());
    TxHash(hasher.finalize().into())
}

/// BlockTransaction enum wraps all transaction types for inclusion in blocks
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BlockTransaction {
    /// Standard value transfer transaction
    Transfer(Transaction),
    /// Reward distribution transaction (CEO-signed)
    Reward(RewardTx),
    /// Stake tokens for validator/compute participation
    Stake(StakeTx),
    /// Initiate unstaking with cooldown
    Unstake(UnstakeTx),
    /// Claim unstaked tokens after cooldown
    ClaimStake(ClaimStakeTx),
    /// Register as G8 Safety Committee candidate
    RegisterG8Candidate(RegisterG8CandidateTx),
    /// Vote for G8 candidate
    VoteG8(VoteG8Tx),
    /// Record G8 meeting (CEO-signed)
    RecordG8Meeting(RecordG8MeetingTx),
    /// Remove G8 member (CEO-signed)
    RemoveG8Member(RemoveG8MemberTx),
    /// Register model on-chain (CEO-signed)
    RegisterModel(RegisterModelTx),
    /// Update constitution (CEO-signed)
    UpdateConstitution(UpdateConstitutionTx),
    /// Record training round with PoI proof
    RecordTrainingRound(RecordTrainingRoundTx),
}

impl BlockTransaction {
    /// Get the transaction hash for this block transaction
    pub fn tx_hash(&self) -> TxHash {
        match self {
            BlockTransaction::Transfer(tx) => tx.tx_hash,
            BlockTransaction::Reward(tx) => tx.tx_hash,
            BlockTransaction::Stake(tx) => tx.tx_hash,
            BlockTransaction::Unstake(tx) => tx.tx_hash,
            BlockTransaction::ClaimStake(tx) => tx.tx_hash,
            BlockTransaction::RegisterG8Candidate(tx) => tx.tx_hash,
            BlockTransaction::VoteG8(tx) => tx.tx_hash,
            BlockTransaction::RecordG8Meeting(tx) => tx.tx_hash,
            BlockTransaction::RemoveG8Member(tx) => tx.tx_hash,
            BlockTransaction::RegisterModel(tx) => tx.tx_hash,
            BlockTransaction::UpdateConstitution(tx) => tx.tx_hash,
            BlockTransaction::RecordTrainingRound(tx) => tx.tx_hash,
        }
    }

    /// Validate the transaction
    pub fn validate(&self) -> Result<(), BlockchainError> {
        match self {
            BlockTransaction::Transfer(tx) => tx.validate(),
            BlockTransaction::Reward(tx) => tx.validate(),
            BlockTransaction::Stake(tx) => tx.validate(),
            BlockTransaction::Unstake(tx) => tx.validate(),
            BlockTransaction::ClaimStake(tx) => tx.validate(),
            BlockTransaction::RegisterG8Candidate(tx) => tx.validate(),
            BlockTransaction::VoteG8(tx) => tx.validate(),
            BlockTransaction::RecordG8Meeting(tx) => tx.validate(),
            BlockTransaction::RemoveG8Member(tx) => tx.validate(),
            BlockTransaction::RegisterModel(tx) => tx.validate(),
            BlockTransaction::UpdateConstitution(tx) => tx.validate(),
            BlockTransaction::RecordTrainingRound(tx) => tx.validate(),
        }
    }

    /// Check if this is a CEO-signed transaction
    pub fn is_ceo_signed(&self) -> bool {
        match self {
            BlockTransaction::Transfer(tx) => tx.is_ceo_transaction(),
            BlockTransaction::Reward(tx) => tx.is_authorized(),
            BlockTransaction::RecordG8Meeting(tx) => tx.ceo_signature.is_some(),
            BlockTransaction::RemoveG8Member(tx) => tx.ceo_signature.is_some(),
            BlockTransaction::RegisterModel(tx) => tx.ceo_signature.is_some(),
            BlockTransaction::UpdateConstitution(tx) => tx.ceo_signature.is_some(),
            _ => false,
        }
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
