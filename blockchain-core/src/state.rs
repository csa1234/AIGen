// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::crypto::hash_data;
use crate::transaction::{Transaction, RewardTx};
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstitutionState {
    pub version: u32,
    pub ipfs_hash: String,
    pub principles_hash: [u8; 32],
    pub updated_at: i64,
    pub updated_by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionState {
    pub user_address: String,
    pub tier: u8,
    pub start_timestamp: i64,
    pub expiry_timestamp: i64,
    pub requests_used: u64,
    pub last_reset_timestamp: i64,
    pub auto_renew: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchJobState {
    pub job_id: String,
    pub user_address: String,
    pub priority: u8,
    pub status: u8,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub completion_time: Option<i64>,
    pub model_id: String,
    pub result_hash: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelUpgradeProposalState {
    pub proposal_id: String,
    pub model_id: String,
    pub current_version: String,
    pub new_version: String,
    pub description: String,
    pub submitted_by: String,
    pub submitted_at: i64,
    pub status: u8, // 0=Pending, 1=Approved, 2=Rejected, 3=AutoApproved
    pub approved_at: Option<i64>,
    pub rejected_at: Option<i64>,
    pub rejection_reason: Option<String>,
    pub auto_approved: bool, // NEW: track if auto-approved by stakers
}

/// Training round state for federated learning tracking
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrainingRound {
    /// Unique round identifier
    pub round_id: String, // Uuid stored as string
    /// Participating node addresses
    pub participants: Vec<String>,
    /// Hash of the averaged delta (for verification)
    pub delta_hash: [u8; 32],
    /// Hash of Fisher matrix stored in IPFS
    pub fisher_hash: [u8; 32],
    /// Hash of replay buffer stored in IPFS
    pub replay_buffer_hash: [u8; 32],
    /// Timestamp when round was completed
    pub timestamp: i64,
    /// Model ID that was trained
    pub model_id: String,
    /// Number of training samples used
    pub sample_count: u64,
    /// Learning rate used
    pub learning_rate: u64, // Scaled by 1e6 for precision
    /// EWC lambda value (scaled by 100)
    pub ewc_lambda: u32,
    /// Status: 0=Pending, 1=Completed, 2=Failed
    pub status: u8,
}

/// Fisher matrix metadata for EWC
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FisherMatrixState {
    /// Model ID this Fisher matrix belongs to
    pub model_id: String,
    /// IPFS hash where the full Fisher matrix is stored
    pub ipfs_hash: String,
    /// Hash of the matrix data for verification
    pub data_hash: [u8; 32],
    /// Timestamp when computed
    pub computed_at: i64,
    /// Block height when recorded
    pub block_height: u64,
}

/// Replay buffer metadata for continual learning
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayBufferState {
    /// Model ID this replay buffer belongs to
    pub model_id: String,
    /// IPFS hash where the full replay buffer is stored
    pub ipfs_hash: String,
    /// Hash of the buffer data for verification
    pub data_hash: [u8; 32],
    /// Timestamp when persisted
    pub persisted_at: i64,
    /// Number of samples in the buffer
    pub sample_count: usize,
    /// Block height when recorded
    pub block_height: u64,
}

/// Benevolence model metadata for ML-based constitutional compliance
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BenevolenceModelState {
    /// Unique model identifier
    pub model_id: String,
    /// IPFS CID where the ONNX model is stored
    pub ipfs_cid: String,
    /// SHA3-256 hash of the model file for verification
    pub model_hash: [u8; 32],
    /// Model version string
    pub version: String,
    /// Benevolence threshold (0.0-1.0)
    pub threshold: f32,
    /// Timestamp when model was deployed
    pub deployed_at: i64,
    /// Address that deployed the model
    pub deployed_by: String,
}

/// On-chain record of model approval for governance transparency
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelApprovalRecord {
    /// Proposal ID being recorded
    pub proposal_id: String,
    /// Model ID associated with this proposal
    pub model_id: String,
    /// Status: 0=Pending, 1=Approved, 2=Rejected, 3=AutoApprovedPendingCeoWindow, 4=Vetoed
    pub status: u8,
    /// Whether this was auto-approved by staker consensus
    pub auto_approved: bool,
    /// Unix timestamp when auto-approval occurred
    pub auto_approved_at: Option<i64>,
    /// Unix timestamp for CEO veto deadline (auto_approved_at + 24h)
    pub ceo_veto_deadline: Option<i64>,
    /// Unix timestamp when finally approved
    pub approved_at: Option<i64>,
    /// Unix timestamp when rejected
    pub rejected_at: Option<i64>,
    /// Detailed rejection reasons for CEO review
    pub rejection_logs: Vec<String>,
    /// Constitutional violations found during review
    pub constitutional_violations: Vec<String>,
    /// Benevolence model score (0.0-1.0)
    pub benevolence_score: Option<f32>,
    /// JSON-serialized safety oracle votes
    pub safety_oracle_votes: Option<String>,
    /// SHA3-256 hash of training proof JSON
    pub training_proof_hash: Option<[u8; 32]>,
    /// UUID reference to on-chain TrainingRound
    pub training_round_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceVote {
    pub proposal_id: String,
    pub voter_address: String,
    pub vote: u8,
    pub comment: Option<String>,
    pub timestamp: i64,
    pub stake_weight: u64, // Snapshotted stake weight at vote time
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VoteTally {
    pub proposal_id: String,
    pub total_approve_weight: u64,
    pub total_reject_weight: u64,
    pub total_abstain_weight: u64,
    pub total_voting_power: u64,
    pub unique_voters: usize,
    pub approval_percentage: f64,
    pub participation_percentage: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StakeState {
    pub staker_address: String,
    pub staked_amount: Amount,
    pub staked_since: i64, // timestamp
    pub pending_unstake: Amount, // amount queued for unstake
    pub unstake_available_at: Option<i64>, // timestamp when unstake completes
    pub role: StakeRole, // Inference | Training | Both
    pub total_rewards_claimed: Amount,
    pub last_slash_timestamp: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StakeRole {
    Inference,  // MIN_STAKE_INFERENCE = 1000
    Training,   // MIN_STAKE_TRAINING = 5000
    Both,       // MIN_STAKE_TRAINING = 5000
}

/// G8 Safety Committee - Advisory governance body
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8MemberState {
    pub member_address: String,
    pub elected_at: i64,
    pub term_end: i64,
    pub stake_at_election: Amount,
    pub background_check_hash: [u8; 32],
    pub expertise_areas: Vec<String>,
    pub meetings_attended: u32,
    pub violations_reported: u32,
    pub removed: bool,
    pub removal_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8Candidate {
    pub address: String,
    pub stake_amount: Amount,
    pub background_check_hash: [u8; 32],
    pub expertise_statement: String,
    pub registered_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8Vote {
    pub voter_address: String,
    pub candidate_address: String,
    pub vote_weight: u64,
    pub timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8ElectionState {
    pub election_id: String,
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub candidates: Vec<G8Candidate>,
    pub votes: BTreeMap<String, Vec<G8Vote>>,
    pub status: u8,
    pub elected_members: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8Recommendation {
    pub recommendation_id: String,
    pub meeting_id: String,
    pub proposal_id: Option<String>,
    pub title: String,
    pub description: String,
    pub category: String,
    pub votes_for: u8,
    pub votes_against: u8,
    pub votes_abstain: u8,
    pub submitted_to_ceo: bool,
    pub ceo_response: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct G8MeetingState {
    pub meeting_id: String,
    pub scheduled_at: i64,
    pub attendees: Vec<String>,
    pub quorum_met: bool,
    pub topics: Vec<String>,
    pub recommendations: Vec<G8Recommendation>,
    pub minutes_ipfs_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChainStateSnapshot {
    pub accounts: BTreeMap<Address, AccountState>,
    pub subscriptions: BTreeMap<Address, SubscriptionState>,
    pub batch_jobs: BTreeMap<String, BatchJobState>,
    pub model_proposals: BTreeMap<String, ModelUpgradeProposalState>,
    pub governance_votes: Vec<GovernanceVote>,
    pub stakes: BTreeMap<Address, StakeState>,
    pub constitution: Option<ConstitutionState>,
    pub training_rounds: BTreeMap<String, TrainingRound>,
    pub fisher_matrices: BTreeMap<String, FisherMatrixState>,
    pub replay_buffers: BTreeMap<String, ReplayBufferState>,
    pub benevolence_models: BTreeMap<String, BenevolenceModelState>,
    pub model_approval_records: BTreeMap<String, ModelApprovalRecord>,
    pub g8_members: BTreeMap<String, G8MemberState>,
    pub g8_elections: BTreeMap<String, G8ElectionState>,
    pub g8_meetings: BTreeMap<String, G8MeetingState>,
}

#[derive(Debug, Default)]
pub struct ChainState {
    accounts: RwLock<BTreeMap<Address, AccountState>>,
    subscriptions: RwLock<BTreeMap<Address, SubscriptionState>>,
    batch_jobs: RwLock<BTreeMap<String, BatchJobState>>,
    model_proposals: RwLock<BTreeMap<String, ModelUpgradeProposalState>>,
    governance_votes: RwLock<Vec<GovernanceVote>>,
    stakes: RwLock<BTreeMap<Address, StakeState>>,
    validator_reward_address: RwLock<Address>,
    constitution: RwLock<Option<ConstitutionState>>,
    training_rounds: RwLock<BTreeMap<String, TrainingRound>>,
    fisher_matrices: RwLock<BTreeMap<String, FisherMatrixState>>,
    replay_buffers: RwLock<BTreeMap<String, ReplayBufferState>>,
    benevolence_models: RwLock<BTreeMap<String, BenevolenceModelState>>,
    model_approval_records: RwLock<BTreeMap<String, ModelApprovalRecord>>,
    g8_members: RwLock<BTreeMap<String, G8MemberState>>,
    g8_elections: RwLock<BTreeMap<String, G8ElectionState>>,
    g8_meetings: RwLock<BTreeMap<String, G8MeetingState>>,
}

impl Clone for ChainState {
    fn clone(&self) -> Self {
        let accounts = self.accounts.read().clone();
        let subscriptions = self.subscriptions.read().clone();
        let batch_jobs = self.batch_jobs.read().clone();
        let model_proposals = self.model_proposals.read().clone();
        let governance_votes = self.governance_votes.read().clone();
        let stakes = self.stakes.read().clone();
        let validator_reward_address = self.validator_reward_address.read().clone();
        let constitution = self.constitution.read().clone();
        let training_rounds = self.training_rounds.read().clone();
        let fisher_matrices = self.fisher_matrices.read().clone();
        let replay_buffers = self.replay_buffers.read().clone();
        let benevolence_models = self.benevolence_models.read().clone();
        let model_approval_records = self.model_approval_records.read().clone();
        let g8_members = self.g8_members.read().clone();
        let g8_elections = self.g8_elections.read().clone();
        let g8_meetings = self.g8_meetings.read().clone();
        ChainState {
            accounts: RwLock::new(accounts),
            subscriptions: RwLock::new(subscriptions),
            batch_jobs: RwLock::new(batch_jobs),
            model_proposals: RwLock::new(model_proposals),
            governance_votes: RwLock::new(governance_votes),
            stakes: RwLock::new(stakes),
            validator_reward_address: RwLock::new(validator_reward_address),
            constitution: RwLock::new(constitution),
            training_rounds: RwLock::new(training_rounds),
            fisher_matrices: RwLock::new(fisher_matrices),
            replay_buffers: RwLock::new(replay_buffers),
            benevolence_models: RwLock::new(benevolence_models),
            model_approval_records: RwLock::new(model_approval_records),
            g8_members: RwLock::new(g8_members),
            g8_elections: RwLock::new(g8_elections),
            g8_meetings: RwLock::new(g8_meetings),
        }
    }
}

impl ChainState {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(BTreeMap::new()),
            subscriptions: RwLock::new(BTreeMap::new()),
            batch_jobs: RwLock::new(BTreeMap::new()),
            model_proposals: RwLock::new(BTreeMap::new()),
            governance_votes: RwLock::new(Vec::new()),
            stakes: RwLock::new(BTreeMap::new()),
            validator_reward_address: RwLock::new(CEO_WALLET.to_string()),
            constitution: RwLock::new(None),
            training_rounds: RwLock::new(BTreeMap::new()),
            fisher_matrices: RwLock::new(BTreeMap::new()),
            replay_buffers: RwLock::new(BTreeMap::new()),
            benevolence_models: RwLock::new(BTreeMap::new()),
            model_approval_records: RwLock::new(BTreeMap::new()),
            g8_members: RwLock::new(BTreeMap::new()),
            g8_elections: RwLock::new(BTreeMap::new()),
            g8_meetings: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn snapshot(&self) -> ChainStateSnapshot {
        ChainStateSnapshot {
            accounts: self.accounts.read().clone(),
            subscriptions: self.subscriptions.read().clone(),
            batch_jobs: self.batch_jobs.read().clone(),
            model_proposals: self.model_proposals.read().clone(),
            governance_votes: self.governance_votes.read().clone(),
            stakes: self.stakes.read().clone(),
            constitution: self.constitution.read().clone(),
            training_rounds: self.training_rounds.read().clone(),
            fisher_matrices: self.fisher_matrices.read().clone(),
            replay_buffers: self.replay_buffers.read().clone(),
            benevolence_models: self.benevolence_models.read().clone(),
            model_approval_records: self.model_approval_records.read().clone(),
            g8_members: self.g8_members.read().clone(),
            g8_elections: self.g8_elections.read().clone(),
            g8_meetings: self.g8_meetings.read().clone(),
        }
    }

    pub fn restore(&self, snapshot: ChainStateSnapshot) {
        *self.accounts.write() = snapshot.accounts;
        *self.subscriptions.write() = snapshot.subscriptions;
        *self.batch_jobs.write() = snapshot.batch_jobs;
        *self.model_proposals.write() = snapshot.model_proposals;
        *self.governance_votes.write() = snapshot.governance_votes;
        *self.stakes.write() = snapshot.stakes;
        *self.constitution.write() = snapshot.constitution;
        *self.training_rounds.write() = snapshot.training_rounds;
        *self.fisher_matrices.write() = snapshot.fisher_matrices;
        *self.replay_buffers.write() = snapshot.replay_buffers;
        *self.benevolence_models.write() = snapshot.benevolence_models;
        *self.model_approval_records.write() = snapshot.model_approval_records;
        *self.g8_members.write() = snapshot.g8_members;
        *self.g8_elections.write() = snapshot.g8_elections;
        *self.g8_meetings.write() = snapshot.g8_meetings;
    }

    /// Set constitution state (CEO-only operation)
    pub fn set_constitution(&self, state: ConstitutionState) {
        *self.constitution.write() = Some(state);
    }

    /// Get constitution state
    pub fn get_constitution(&self) -> Option<ConstitutionState> {
        self.constitution.read().clone()
    }

    /// Verify constitution hash matches expected
    pub fn verify_constitution_hash(&self, expected_hash: [u8; 32]) -> bool {
        if let Some(ref state) = *self.constitution.read() {
            state.principles_hash == expected_hash
        } else {
            false
        }
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

    pub fn get_subscription(&self, address: &str) -> Option<SubscriptionState> {
        self.subscriptions.read().get(address).cloned()
    }

    pub fn set_subscription(&self, address: Address, subscription: SubscriptionState) {
        self.subscriptions.write().insert(address, subscription);
    }

    pub fn set_batch_job(&self, job_id: String, state: BatchJobState) {
        self.batch_jobs.write().insert(job_id, state);
    }

    pub fn get_batch_job(&self, job_id: &str) -> Option<BatchJobState> {
        self.batch_jobs.read().get(job_id).cloned()
    }

    pub fn list_batch_jobs_by_user(&self, user_address: &str) -> Vec<BatchJobState> {
        self.batch_jobs
            .read()
            .values()
            .filter(|job| job.user_address == user_address)
            .cloned()
            .collect()
    }

    pub fn list_pending_batch_jobs(&self) -> Vec<BatchJobState> {
        self.batch_jobs
            .read()
            .values()
            .filter(|job| job.status == 0 || job.status == 1)
            .cloned()
            .collect()
    }

    pub fn update_subscription_usage(
        &self,
        address: &str,
        requests_used: u64,
    ) -> Result<(), BlockchainError> {
        let mut subscriptions = self.subscriptions.write();
        let entry = subscriptions
            .get_mut(address)
            .ok_or(BlockchainError::InvalidTransaction)?;
        entry.requests_used = requests_used;
        Ok(())
    }

    pub fn remove_subscription(&self, address: &str) -> Result<(), BlockchainError> {
        let mut subscriptions = self.subscriptions.write();
        if subscriptions.remove(address).is_none() {
            return Err(BlockchainError::InvalidTransaction);
        }
        Ok(())
    }

    pub fn list_subscriptions(&self) -> Vec<(Address, SubscriptionState)> {
        self.subscriptions
            .read()
            .iter()
            .map(|(address, state)| (address.clone(), state.clone()))
            .collect()
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

        let involved = [
            sender_addr.clone(),
            receiver_addr.clone(),
            validator_addr.clone(),
            dev_addr.clone(),
        ];

        let mut accounts = self.accounts.write();
        let mut temp: HashMap<Address, AccountState> = HashMap::new();

        let get_state = |addr: &Address,
                         accounts: &BTreeMap<Address, AccountState>,
                         temp: &HashMap<Address, AccountState>| {
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

    /// Apply a reward transaction - mints tokens to recipient
    pub fn apply_reward_transaction(&self, reward_tx: &RewardTx) -> Result<(), BlockchainError> {
        // Validate the reward transaction
        reward_tx.validate()?;

        // Verify CEO authorization
        if !reward_tx.is_authorized() {
            return Err(BlockchainError::InvalidSignature);
        }

        // Mint tokens to recipient (no fee for reward txs)
        self.mint_tokens(&reward_tx.recipient, reward_tx.amount)?;

        Ok(())
    }

    pub fn calculate_state_root(&self) -> StateRoot {
        let accounts = self.accounts.read();
        let subscriptions = self.subscriptions.read();
        if accounts.is_empty() && subscriptions.is_empty() {
            return hash_data(&[][..]);
        }

        let mut leaves: Vec<[u8; 32]> =
            Vec::with_capacity(accounts.len().saturating_add(subscriptions.len()));
        for (addr, state) in accounts.iter() {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"acct");
            buf.extend_from_slice(addr.as_bytes());
            buf.extend_from_slice(&state.balance.amount().value().to_le_bytes());
            buf.extend_from_slice(&state.nonce.value().to_le_bytes());
            buf.push(state.is_contract as u8);
            if let Some(ch) = state.code_hash {
                buf.extend_from_slice(&ch);
            }
            leaves.push(hash_data(&buf));
        }

        for (addr, subscription) in subscriptions.iter() {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"sub");
            buf.extend_from_slice(addr.as_bytes());
            buf.push(subscription.tier);
            buf.extend_from_slice(&subscription.start_timestamp.to_le_bytes());
            buf.extend_from_slice(&subscription.expiry_timestamp.to_le_bytes());
            buf.extend_from_slice(&subscription.requests_used.to_le_bytes());
            buf.extend_from_slice(&subscription.last_reset_timestamp.to_le_bytes());
            buf.push(subscription.auto_renew as u8);
            leaves.push(hash_data(&buf));
        }

        while leaves.len() > 1 {
            let mut next = Vec::with_capacity(leaves.len().div_ceil(2));
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

    pub fn set_model_proposal(&self, proposal_id: String, proposal: ModelUpgradeProposalState) {
        self.model_proposals.write().insert(proposal_id, proposal);
    }

    pub fn get_model_proposal(&self, proposal_id: &str) -> Option<ModelUpgradeProposalState> {
        self.model_proposals.read().get(proposal_id).cloned()
    }

    pub fn list_pending_model_proposals(&self) -> Vec<ModelUpgradeProposalState> {
        self.model_proposals.read().values()
            .filter(|p| p.status == 0)
            .cloned()
            .collect()
    }

    /// Check and auto-approve model proposal if staker threshold met
    /// Uses GovernanceConfig for thresholds
    pub fn check_model_proposal_auto_approve(
        &self,
        proposal_id: &str,
    ) -> Result<bool, BlockchainError> {
        // Load governance config for thresholds
        let config = genesis::GovernanceConfig::load();
        
        if !config.auto_approve_enabled {
            return Ok(false); // Auto-approve disabled
        }
        
        let meets_threshold = self.check_auto_approve_threshold(
            proposal_id,
            config.min_approval_percentage,
            config.min_participation_percentage,
        );
        
        if meets_threshold {
            let mut proposals = self.model_proposals.write();
            if let Some(proposal) = proposals.get_mut(proposal_id) {
                if proposal.status == 0 { // Only auto-approve pending proposals
                    proposal.status = 3; // AutoApproved
                    proposal.auto_approved = true;
                    proposal.approved_at = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    );
                    eprintln!(
                        "Model proposal {} auto-approved by staker consensus (threshold: {}% approval, {}% participation)",
                        proposal_id,
                        config.min_approval_percentage,
                        config.min_participation_percentage
                    );
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Trigger auto-approval check for both SIPs and model proposals
    /// Call this after recording votes or when closing voting period
    pub fn trigger_auto_approval(&self, proposal_id: &str) -> Result<bool, BlockchainError> {
        // Try model proposal auto-approval first
        let model_result = self.check_model_proposal_auto_approve(proposal_id);
        
        // Also trigger SIP auto-approval through genesis
        let sip_result = genesis::trigger_auto_approval_check(proposal_id, self as &dyn genesis::ChainStateProvider);
        
        // Return true if either succeeded
        match (model_result, sip_result) {
            (Ok(true), _) => Ok(true),
            (_, Ok(true)) => Ok(true),
            (Err(e), _) => Err(e),
            (_, Err(_)) => Ok(false),
            _ => Ok(false),
        }
    }

    /// Record a governance vote (validates staker has stake and snapshots weight)
    pub fn record_governance_vote(&self, mut vote: GovernanceVote) -> Result<(), BlockchainError> {
        // Check if voter has stake (or is CEO)
        let stake_weight = if vote.voter_address != genesis::CEO_WALLET {
            let stake = self.get_stake(&vote.voter_address);
            if let Some(stake) = stake {
                if stake.staked_amount.is_zero() {
                    return Err(BlockchainError::InvalidTransaction); // No stake, can't vote
                }
                stake.staked_amount.value()
            } else {
                return Err(BlockchainError::InvalidTransaction); // No stake, can't vote
            }
        } else {
            // CEO votes with weight 0 (their approval is handled separately via signature)
            0
        };
        
        // Snapshot the stake weight at vote time
        vote.stake_weight = stake_weight;
        
        // Explicitly reject votes with zero stake weight (except CEO)
        if vote.voter_address != genesis::CEO_WALLET && stake_weight == 0 {
            return Err(BlockchainError::InvalidTransaction); // Zero stake weight not allowed for non-CEO voters
        }
        
        // Prevent duplicate votes from same address on same proposal
        let existing_votes = self.get_votes_for_proposal(&vote.proposal_id);
        if existing_votes.iter().any(|v| v.voter_address == vote.voter_address) {
            return Err(BlockchainError::InvalidTransaction); // Already voted
        }
        
        // After recording vote, try to auto-approve (handles both model proposals and SIPs)
        let proposal_id = vote.proposal_id.clone();
        
        self.governance_votes.write().push(vote);
        
        let _ = self.trigger_auto_approval(&proposal_id);
        
        Ok(())
    }

    pub fn get_votes_for_proposal(&self, proposal_id: &str) -> Vec<GovernanceVote> {
        self.governance_votes.read().iter()
            .filter(|v| v.proposal_id == proposal_id)
            .cloned()
            .collect()
    }

    /// Calculate weighted vote tally based on staked amounts
    pub fn tally_votes(&self, proposal_id: &str) -> VoteTally {
        let votes = self.get_votes_for_proposal(proposal_id);
        let stakes = self.stakes.read();
        
        let mut approve_weight = 0u64;
        let mut reject_weight = 0u64;
        let mut abstain_weight = 0u64;
        let mut unique_voters = 0;
        
        // Calculate total staked amount across all stakers for participation %
        let total_staked: u64 = stakes.values()
            .map(|s| s.staked_amount.value())
            .sum();
        
        for vote in votes.iter() {
            // Use the snapshotted stake weight from when vote was recorded
            let stake_weight = vote.stake_weight;
            
            if stake_weight > 0 {
                unique_voters += 1;
                match vote.vote {
                    0 => approve_weight += stake_weight, // Approve
                    1 => reject_weight += stake_weight,  // Reject
                    2 => abstain_weight += stake_weight, // Abstain
                    _ => {} // Invalid vote, skip
                }
            }
        }
        
        let total_voting_power = approve_weight + reject_weight + abstain_weight;
        let approval_percentage = if total_voting_power > 0 {
            (approve_weight as f64 / total_voting_power as f64) * 100.0
        } else {
            0.0
        };
        
        let participation_percentage = if total_staked > 0 {
            (total_voting_power as f64 / total_staked as f64) * 100.0
        } else {
            0.0
        };
        
        VoteTally {
            proposal_id: proposal_id.to_string(),
            total_approve_weight: approve_weight,
            total_reject_weight: reject_weight,
            total_abstain_weight: abstain_weight,
            total_voting_power,
            unique_voters,
            approval_percentage,
            participation_percentage,
        }
    }

    /// Check if proposal meets auto-approval threshold
    /// Default: 80% approval + 50% participation
    pub fn check_auto_approve_threshold(
        &self, 
        proposal_id: &str,
        min_approval_pct: f64,
        min_participation_pct: f64,
    ) -> bool {
        let tally = self.tally_votes(proposal_id);
        tally.approval_percentage >= min_approval_pct 
            && tally.participation_percentage >= min_participation_pct
    }

    // Stake management
    pub fn get_stake(&self, address: &str) -> Option<StakeState> {
        self.stakes.read().get(address).cloned()
    }

    pub fn get_staked_amount(&self, address: &str) -> Amount {
        self.stakes.read()
            .get(address)
            .map(|s| s.staked_amount)
            .unwrap_or(Amount::ZERO)
    }

    pub fn set_stake(&self, address: Address, stake: StakeState) {
        self.stakes.write().insert(address, stake);
    }

    pub fn list_all_stakes(&self) -> Vec<(Address, StakeState)> {
        self.stakes.read()
            .iter()
            .map(|(addr, stake)| (addr.clone(), stake.clone()))
            .collect()
    }

    pub fn list_stakes_by_role(&self, role: &StakeRole) -> Vec<(Address, StakeState)> {
        self.stakes.read()
            .iter()
            .filter(|(_, s)| &s.role == role || matches!(s.role, StakeRole::Both))
            .map(|(addr, stake)| (addr.clone(), stake.clone()))
            .collect()
    }

    pub fn apply_stake_slash(&self, address: &str, slash_amount: Amount, reason: &str) -> Result<(), BlockchainError> {
        let mut stakes = self.stakes.write();
        let stake = stakes.get_mut(address)
            .ok_or(BlockchainError::InvalidTransaction)?;

        let slash = slash_amount.value();
        let pending = stake.pending_unstake.value();
        let staked = stake.staked_amount.value();
        let total_available = pending.saturating_add(staked);

        // Ensure we don't slash more than available
        let actual_slash = slash.min(total_available);

        // Deduct from pending_unstake first, then from staked_amount
        let from_pending = actual_slash.min(pending);
        let from_staked = actual_slash.saturating_sub(from_pending);

        stake.pending_unstake = Amount::new(pending.saturating_sub(from_pending));
        stake.staked_amount = Amount::new(staked.saturating_sub(from_staked));

        stake.last_slash_timestamp = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        );

        // Apply distribution: 40% burn, 60% to CEO
        // Burn 40% by not minting it back (tokens already deducted from stake)
        let burn_amount = Amount::new(actual_slash.saturating_mul(40) / 100);
        let ceo_amount = Amount::new(actual_slash.saturating_sub(burn_amount.value()));

        // Mint CEO share directly (doesn't affect liquid balance of the slashed address)
        self.mint_tokens(CEO_WALLET, ceo_amount)?;

        eprintln!("Slashed {} AIGEN from {} (burned: {}, to CEO: {}, reason: {})",
            actual_slash, address, burn_amount.value(), ceo_amount.value(), reason);
        Ok(())
    }

    /// Apply a stake transaction - locks tokens and creates/updates stake state
    pub fn apply_stake_transaction(&self, stake_tx: &crate::transaction::StakeTx) -> Result<(), BlockchainError> {
        stake_tx.validate()?;

        // Get existing stake amount (if any)
        let existing_stake = self.get_staked_amount(&stake_tx.staker);

        // Compute prospective total stake
        let prospective_total = existing_stake.safe_add(stake_tx.amount)?;

        // Check minimum stake requirements on the resulting total
        let min_stake = match stake_tx.role {
            StakeRole::Inference => 1_000,
            StakeRole::Training | StakeRole::Both => 5_000,
        };

        if prospective_total.value() < min_stake {
            return Err(BlockchainError::InsufficientStake);
        }

        // Deduct from balance
        let balance = self.get_balance(&stake_tx.staker);
        if balance.amount().value() < stake_tx.amount.value() {
            return Err(BlockchainError::InsufficientBalance);
        }

        self.burn_tokens(&stake_tx.staker, stake_tx.amount)?;

        // Create or update stake state
        let mut stakes = self.stakes.write();
        let stake_state = stakes.entry(stake_tx.staker.clone()).or_insert_with(|| {
            StakeState {
                staker_address: stake_tx.staker.clone(),
                staked_amount: Amount::ZERO,
                staked_since: stake_tx.timestamp.value(),
                pending_unstake: Amount::ZERO,
                unstake_available_at: None,
                role: stake_tx.role.clone(),
                total_rewards_claimed: Amount::ZERO,
                last_slash_timestamp: None,
            }
        });

        stake_state.staked_amount = stake_state.staked_amount.safe_add(stake_tx.amount)?;
        stake_state.role = stake_tx.role.clone(); // Update role if changed

        Ok(())
    }

    /// Apply an unstake transaction - initiates unstaking with cooldown
    pub fn apply_unstake_transaction(&self, unstake_tx: &crate::transaction::UnstakeTx) -> Result<(), BlockchainError> {
        unstake_tx.validate()?;

        let mut stakes = self.stakes.write();
        let stake_state = stakes.get_mut(&unstake_tx.staker)
            .ok_or(BlockchainError::InvalidTransaction)?;

        // Check sufficient staked amount
        if stake_state.staked_amount.value() < unstake_tx.amount.value() {
            return Err(BlockchainError::InsufficientBalance);
        }

        // Move to pending unstake
        stake_state.staked_amount = stake_state.staked_amount.safe_sub(unstake_tx.amount)?;
        stake_state.pending_unstake = stake_state.pending_unstake.safe_add(unstake_tx.amount)?;
        stake_state.unstake_available_at = Some(
            unstake_tx.timestamp.value() + crate::transaction::UnstakeTx::UNSTAKE_COOLDOWN_SECS
        );

        Ok(())
    }

    /// Apply a claim stake transaction - returns unstaked tokens after cooldown
    pub fn apply_claim_stake_transaction(&self, claim_tx: &crate::transaction::ClaimStakeTx) -> Result<(), BlockchainError> {
        claim_tx.validate()?;

        let mut stakes = self.stakes.write();
        let stake_state = stakes.get_mut(&claim_tx.staker)
            .ok_or(BlockchainError::InvalidTransaction)?;

        // Check cooldown period
        if let Some(available_at) = stake_state.unstake_available_at {
            if claim_tx.timestamp.value() < available_at {
                return Err(BlockchainError::InvalidTransaction);
            }
        } else {
            return Err(BlockchainError::InvalidTransaction);
        }

        // Return tokens to balance
        let claim_amount = stake_state.pending_unstake;
        stake_state.pending_unstake = Amount::ZERO;
        stake_state.unstake_available_at = None;

        self.mint_tokens(&claim_tx.staker, claim_amount)?;

        // Remove stake state if fully unstaked
        if stake_state.staked_amount.is_zero() {
            stakes.remove(&claim_tx.staker);
        }

        Ok(())
    }

    /// Record a training round on-chain
    pub fn record_training_round(&self, round: TrainingRound) -> Result<(), BlockchainError> {
        let mut training_rounds = self.training_rounds.write();
        training_rounds.insert(round.round_id.clone(), round);
        Ok(())
    }

    /// Get a training round by ID
    pub fn get_training_round(&self, round_id: &str) -> Option<TrainingRound> {
        self.training_rounds.read().get(round_id).cloned()
    }

    /// List all training rounds for a model
    pub fn list_training_rounds_for_model(&self, model_id: &str) -> Vec<TrainingRound> {
        self.training_rounds
            .read()
            .values()
            .filter(|r| r.model_id == model_id)
            .cloned()
            .collect()
    }

    /// List all completed training rounds
    pub fn list_completed_training_rounds(&self) -> Vec<TrainingRound> {
        self.training_rounds
            .read()
            .values()
            .filter(|r| r.status == 1)
            .cloned()
            .collect()
    }

    /// Store Fisher matrix metadata on-chain
    pub fn record_fisher_matrix(&self, fisher: FisherMatrixState) -> Result<(), BlockchainError> {
        let mut fisher_matrices = self.fisher_matrices.write();
        fisher_matrices.insert(fisher.model_id.clone(), fisher);
        Ok(())
    }

    /// Get Fisher matrix metadata for a model
    pub fn get_fisher_matrix(&self, model_id: &str) -> Option<FisherMatrixState> {
        self.fisher_matrices.read().get(model_id).cloned()
    }

    /// Get Fisher matrix hash (for on-chain verification)
    pub fn get_fisher_matrix_hash(&self, model_id: &str) -> Option<[u8; 32]> {
        self.fisher_matrices
            .read()
            .get(model_id)
            .map(|f| f.data_hash)
    }

    /// Record a Fisher matrix for continual learning (internal method)
    pub fn store_fisher_matrix_internal(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64) {
        let state = FisherMatrixState {
            model_id,
            ipfs_hash,
            data_hash,
            computed_at: timestamp,
            block_height: 0,
        };
        let mut fisher_matrices = self.fisher_matrices.write();
        fisher_matrices.insert(state.model_id.clone(), state);
    }

    /// Get Fisher matrix metadata for continual learning
    pub fn get_fisher_matrix_for_cl(&self, model_id: &str) -> Option<FisherMatrixState> {
        self.fisher_matrices.read().get(model_id).cloned()
    }

    /// Record a replay buffer for continual learning (internal method)
    pub fn store_replay_buffer_metadata(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64, sample_count: usize) {
        let state = ReplayBufferState {
            model_id,
            ipfs_hash,
            data_hash,
            persisted_at: timestamp,
            sample_count,
            block_height: 0,
        };
        let mut replay_buffers = self.replay_buffers.write();
        replay_buffers.insert(state.model_id.clone(), state);
    }

    /// Get replay buffer metadata for continual learning
    pub fn get_replay_buffer_metadata(&self, model_id: &str) -> Option<ReplayBufferState> {
        self.replay_buffers.read().get(model_id).cloned()
    }

    /// Register a new benevolence model version on-chain
    pub fn store_benevolence_model(&self, model_id: String, ipfs_cid: String, hash: [u8; 32], version: String, threshold: f32, deployed_by: String) -> Result<(), BlockchainError> {
        let state = BenevolenceModelState {
            model_id: model_id.clone(),
            ipfs_cid,
            model_hash: hash,
            version,
            threshold,
            deployed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            deployed_by,
        };
        let mut benevolence_models = self.benevolence_models.write();
        benevolence_models.insert(model_id, state);
        Ok(())
    }

    /// Get benevolence model metadata by model ID
    pub fn get_benevolence_model(&self, model_id: &str) -> Option<BenevolenceModelState> {
        self.benevolence_models.read().get(model_id).cloned()
    }

    /// Get the latest (most recently deployed) benevolence model
    pub fn get_latest_benevolence_model(&self) -> Option<BenevolenceModelState> {
        self.benevolence_models.read()
            .values()
            .max_by_key(|m| m.deployed_at)
            .cloned()
    }

    /// List all benevolence models
    pub fn list_benevolence_models(&self) -> Vec<BenevolenceModelState> {
        self.benevolence_models.read().values().cloned().collect()
    }

    // ==================== Model Approval Records ====================

    /// Record a model approval on-chain
    pub fn record_model_approval(&self, record: ModelApprovalRecord) -> Result<(), BlockchainError> {
        self.model_approval_records.write().insert(record.proposal_id.clone(), record);
        Ok(())
    }

    /// Get a model approval record by proposal ID
    pub fn get_model_approval_record(&self, proposal_id: &str) -> Option<ModelApprovalRecord> {
        self.model_approval_records.read().get(proposal_id).cloned()
    }

    /// List all proposals pending CEO veto window expiration
    pub fn list_pending_approvals(&self) -> Vec<ModelApprovalRecord> {
        self.model_approval_records
            .read()
            .values()
            .filter(|r| r.status == 3) // AutoApprovedPendingCeoWindow
            .cloned()
            .collect()
    }

    /// List all rejected proposals with logs
    pub fn list_rejected_proposals(&self) -> Vec<ModelApprovalRecord> {
        self.model_approval_records
            .read()
            .values()
            .filter(|r| r.status == 2 || r.status == 4) // Rejected or Vetoed
            .cloned()
            .collect()
    }

    // ==================== G8 Safety Committee ====================

    /// Create a new G8 election
    pub fn create_g8_election(&self, election_id: String, start_timestamp: i64) -> Result<(), BlockchainError> {
        let config = genesis::GovernanceConfig::load();
        if !config.g8_enabled {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        let election = G8ElectionState {
            election_id: election_id.clone(),
            start_timestamp,
            end_timestamp: start_timestamp + (48 * 60 * 60), // 48 hours
            candidates: Vec::new(),
            votes: BTreeMap::new(),
            status: 0, // Open
            elected_members: Vec::new(),
        };
        self.g8_elections.write().insert(election_id, election);
        Ok(())
    }

    /// Register a candidate for G8 election
    pub fn register_g8_candidate(&self, tx: &crate::transaction::RegisterG8CandidateTx) -> Result<(), BlockchainError> {
        // Check stake >= 0.1% of total supply
        let total_staked: u64 = self.stakes.read().values()
            .map(|s| s.staked_amount.value())
            .sum();
        let candidate_stake = self.get_staked_amount(&tx.candidate);
        let config = genesis::GovernanceConfig::load();
        let min_stake = ((total_staked as f64) * config.g8_min_stake_percentage / 100.0) as u64;
        
        if candidate_stake.value() < min_stake {
            return Err(BlockchainError::InsufficientStake);
        }
        
        // Check not currently serving in G8
        if let Some(member) = self.g8_members.read().get(&tx.candidate) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if !member.removed && member.term_end > now {
                return Err(BlockchainError::InvalidTransaction);
            }
        }
        
        // Find open election
        let mut elections = self.g8_elections.write();
        let election = elections.values_mut()
            .find(|e| e.status == 0)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        // Check expertise statement length
        if tx.expertise_statement.len() > 500 {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        let candidate = G8Candidate {
            address: tx.candidate.clone(),
            stake_amount: candidate_stake,
            background_check_hash: tx.background_check_hash,
            expertise_statement: tx.expertise_statement.clone(),
            registered_at: tx.timestamp.value(),
        };
        election.candidates.push(candidate);
        Ok(())
    }

    /// Record a G8 election vote
    pub fn record_g8_vote(&self, tx: &crate::transaction::VoteG8Tx) -> Result<(), BlockchainError> {
        // Validate voter has stake
        let voter_stake = self.get_staked_amount(&tx.voter);
        if voter_stake.is_zero() {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        let mut elections = self.g8_elections.write();
        let election = elections.get_mut(&tx.election_id)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        // Check election is open
        if election.status != 0 {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        // Check for duplicate vote
        let existing_votes = election.votes.get(&tx.candidate_address);
        if let Some(votes) = existing_votes {
            if votes.iter().any(|v| v.voter_address == tx.voter) {
                return Err(BlockchainError::InvalidTransaction);
            }
        }
        
        let vote = G8Vote {
            voter_address: tx.voter.clone(),
            candidate_address: tx.candidate_address.clone(),
            vote_weight: voter_stake.value(),
            timestamp: tx.timestamp.value(),
        };
        
        election.votes.entry(tx.candidate_address.clone())
            .or_default()
            .push(vote);
        
        Ok(())
    }

    /// Finalize G8 election and elect top 8 candidates
    pub fn finalize_g8_election(&self, election_id: &str) -> Result<Vec<String>, BlockchainError> {
        let mut elections = self.g8_elections.write();
        let election = elections.get_mut(election_id)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        // Close voting
        if election.status != 0 {
            return Err(BlockchainError::InvalidTransaction);
        }
        election.status = 1; // Closed
        
        // Tally votes
        let mut candidate_votes: Vec<(String, u64)> = election.votes.iter()
            .map(|(addr, votes)| {
                let total_weight = votes.iter().map(|v| v.vote_weight).sum();
                (addr.clone(), total_weight)
            })
            .collect();
        
        // Sort by vote weight descending
        candidate_votes.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Select top 8
        let config = genesis::GovernanceConfig::load();
        let term_seconds = (config.g8_term_years as i64) * 365 * 24 * 60 * 60;
        let elected: Vec<String> = candidate_votes.iter().take(8).map(|(addr, _)| addr.clone()).collect();
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        // Create member states for elected candidates
        for addr in &elected {
            if let Some(candidate) = election.candidates.iter().find(|c| c.address == *addr) {
                let member = G8MemberState {
                    member_address: addr.clone(),
                    elected_at: now,
                    term_end: now + term_seconds,
                    stake_at_election: candidate.stake_amount,
                    background_check_hash: candidate.background_check_hash,
                    expertise_areas: Vec::new(),
                    meetings_attended: 0,
                    violations_reported: 0,
                    removed: false,
                    removal_reason: None,
                };
                self.g8_members.write().insert(addr.clone(), member);
            }
        }
        
        election.elected_members = elected.clone();
        election.status = 2; // Finalized
        
        Ok(elected)
    }

    /// Get G8 member by address
    pub fn get_g8_member(&self, address: &str) -> Option<G8MemberState> {
        self.g8_members.read().get(address).cloned()
    }

    /// List all active G8 members
    pub fn list_active_g8_members(&self) -> Vec<G8MemberState> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        self.g8_members.read()
            .values()
            .filter(|m| !m.removed && m.term_end > now)
            .cloned()
            .collect()
    }

    /// Check if address is eligible for G8 election
    pub fn check_g8_eligibility(&self, address: &str) -> Result<bool, BlockchainError> {
        let config = genesis::GovernanceConfig::load();
        if !config.g8_enabled {
            return Ok(false);
        }
        
        // Check stake >= 0.1% of total supply
        let total_staked: u64 = self.stakes.read().values()
            .map(|s| s.staked_amount.value())
            .sum();
        let stake = self.get_staked_amount(address);
        let min_stake = ((total_staked as f64) * config.g8_min_stake_percentage / 100.0) as u64;
        
        if stake.value() < min_stake {
            return Ok(false);
        }
        
        // Check not currently serving
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        if let Some(member) = self.g8_members.read().get(address) {
            if !member.removed && member.term_end > now {
                return Ok(false);
            }
            
            // Check cool-off period (1 year after term ended)
            let cooloff_seconds = (config.g8_cooloff_years as i64) * 365 * 24 * 60 * 60;
            if member.term_end + cooloff_seconds > now {
                return Ok(false);
            }
        }
        
        Ok(true)
    }

    /// Remove G8 member (CEO only)
    pub fn remove_g8_member(&self, tx: &crate::transaction::RemoveG8MemberTx) -> Result<(), BlockchainError> {
        // Verify CEO signature
        genesis::verify_ceo_transaction(tx)
            .map_err(|_| BlockchainError::InvalidSignature)?;
        
        // Check reason length (min 50 chars)
        if tx.reason.len() < 50 {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        let mut members = self.g8_members.write();
        let member = members.get_mut(&tx.member_address)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        member.removed = true;
        member.removal_reason = Some(tx.reason.clone());
        
        eprintln!("G8 member {} removed by CEO at timestamp {}: {}", 
            tx.member_address, tx.timestamp.value(), tx.reason);
        
        Ok(())
    }

    /// Schedule a G8 meeting
    pub fn schedule_g8_meeting(&self, meeting_id: String, scheduled_at: i64) -> Result<(), BlockchainError> {
        let config = genesis::GovernanceConfig::load();
        if !config.g8_enabled {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        let meeting = G8MeetingState {
            meeting_id: meeting_id.clone(),
            scheduled_at,
            attendees: Vec::new(),
            quorum_met: false,
            topics: Vec::new(),
            recommendations: Vec::new(),
            minutes_ipfs_hash: String::new(),
        };
        
        self.g8_meetings.write().insert(meeting_id, meeting);
        Ok(())
    }

    /// Record G8 meeting with CEO signature
    pub fn record_g8_meeting(&self, tx: &crate::transaction::RecordG8MeetingTx) -> Result<(), BlockchainError> {
        // Verify CEO signature
        genesis::verify_ceo_transaction(tx)
            .map_err(|_| BlockchainError::InvalidSignature)?;
        
        let config = genesis::GovernanceConfig::load();
        let mut meetings = self.g8_meetings.write();
        let meeting = meetings.get_mut(&tx.meeting_id)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        // Check quorum (>= 5 attendees)
        let quorum_met = tx.attendees.len() >= config.g8_quorum as usize;
        
        meeting.attendees = tx.attendees.clone();
        meeting.quorum_met = quorum_met;
        meeting.minutes_ipfs_hash = tx.minutes_ipfs_hash.clone();
        
        // Increment meetings_attended for each attendee
        if quorum_met {
            let mut members = self.g8_members.write();
            for attendee in &tx.attendees {
                if let Some(member) = members.get_mut(attendee) {
                    member.meetings_attended += 1;
                }
            }
        }
        
        Ok(())
    }

    /// Add recommendation to meeting
    pub fn add_g8_recommendation(&self, meeting_id: &str, recommendation: G8Recommendation) -> Result<(), BlockchainError> {
        let mut meetings = self.g8_meetings.write();
        let meeting = meetings.get_mut(meeting_id)
            .ok_or(BlockchainError::InvalidTransaction)?;
        
        if !meeting.quorum_met {
            return Err(BlockchainError::InvalidTransaction);
        }
        
        meeting.recommendations.push(recommendation);
        Ok(())
    }

    /// Get recommendations pending CEO response
    pub fn get_g8_recommendations_pending_ceo(&self) -> Vec<G8Recommendation> {
        self.g8_meetings.read()
            .values()
            .flat_map(|m| m.recommendations.iter())
            .filter(|r| r.submitted_to_ceo && r.ceo_response.is_none())
            .cloned()
            .collect()
    }

    /// Get G8 recommendations for a proposal
    pub fn get_g8_recommendations_for_proposal(&self, proposal_id: &str) -> Vec<G8Recommendation> {
        self.g8_meetings.read()
            .values()
            .flat_map(|m| m.recommendations.iter())
            .filter(|r| r.proposal_id.as_ref() == Some(&proposal_id.to_string()) && r.votes_for >= 5)
            .cloned()
            .collect()
    }

    /// Calculate G8 member compensation
    pub fn calculate_g8_compensation(&self, member_address: &str) -> Amount {
        let config = genesis::GovernanceConfig::load();
        let members = self.g8_members.read();
        let member = match members.get(member_address) {
            Some(m) => m,
            None => return Amount::ZERO,
        };
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        // Calculate days served (capped at term length)
        let days_served = ((now - member.elected_at).min(member.term_end - member.elected_at) / (24 * 60 * 60)) as u64;
        let year_days = 365u64;
        
        // Prorated base compensation: 10,000 AIGEN/year
        let base_compensation = (config.g8_compensation_base * days_served) / year_days;
        
        // Bonus: +2,000 AIGEN if no violations reported
        let bonus = if member.violations_reported == 0 {
            config.g8_compensation_bonus
        } else {
            0
        };
        
        Amount::new(base_compensation + bonus)
    }
}

impl genesis::ChainStateProvider for ChainState {
    fn check_auto_approve_threshold(
        &self,
        proposal_id: &str,
        min_approval_percentage: f64,
        min_participation_percentage: f64,
    ) -> bool {
        // Call the inherent method on ChainState to compute actual approval/participation
        let tally = self.tally_votes(proposal_id);
        tally.approval_percentage >= min_approval_percentage 
            && tally.participation_percentage >= min_participation_percentage
    }

    fn get_training_round(&self, round_id: &str) -> Option<genesis::TrainingRoundInfo> {
        self.training_rounds.read().get(round_id).map(|r| genesis::TrainingRoundInfo {
            round_id: r.round_id.clone(),
            participants: r.participants.clone(),
            delta_hash: r.delta_hash,
            fisher_hash: r.fisher_hash,
            model_id: r.model_id.clone(),
            status: r.status,
        })
    }

    fn get_fisher_matrix_hash(&self, model_id: &str) -> Option<[u8; 32]> {
        self.fisher_matrices.read().get(model_id).map(|m| m.data_hash)
    }

    fn record_model_approval(&self, record: genesis::ModelApprovalRecord) -> Result<(), genesis::GenesisError> {
        let blockchain_record = ModelApprovalRecord {
            proposal_id: record.proposal_id,
            model_id: record.model_id,
            status: record.status,
            auto_approved: record.auto_approved,
            auto_approved_at: record.auto_approved_at,
            ceo_veto_deadline: record.ceo_veto_deadline,
            approved_at: record.approved_at,
            rejected_at: record.rejected_at,
            rejection_logs: record.rejection_logs,
            constitutional_violations: record.constitutional_violations,
            benevolence_score: record.benevolence_score,
            safety_oracle_votes: record.safety_oracle_votes,
            training_proof_hash: record.training_proof_hash,
            training_round_id: record.training_round_id,
        };
        self.model_approval_records.write().insert(blockchain_record.proposal_id.clone(), blockchain_record);
        Ok(())
    }

    fn get_model_approval_record(&self, proposal_id: &str) -> Option<genesis::ModelApprovalRecord> {
        self.model_approval_records.read().get(proposal_id).map(|r| genesis::ModelApprovalRecord {
            proposal_id: r.proposal_id.clone(),
            model_id: r.model_id.clone(),
            status: r.status,
            auto_approved: r.auto_approved,
            auto_approved_at: r.auto_approved_at,
            ceo_veto_deadline: r.ceo_veto_deadline,
            approved_at: r.approved_at,
            rejected_at: r.rejected_at,
            rejection_logs: r.rejection_logs.clone(),
            constitutional_violations: r.constitutional_violations.clone(),
            benevolence_score: r.benevolence_score,
            safety_oracle_votes: r.safety_oracle_votes.clone(),
            training_proof_hash: r.training_proof_hash,
            training_round_id: r.training_round_id.clone(),
        })
    }

    fn get_g8_recommendations_for_proposal(&self, proposal_id: &str) -> Vec<genesis::G8Recommendation> {
        self.g8_meetings.read()
            .values()
            .flat_map(|m| m.recommendations.iter())
            .filter(|r| r.proposal_id.as_ref() == Some(&proposal_id.to_string()) && r.votes_for >= 5)
            .map(|r| genesis::G8Recommendation {
                recommendation_id: r.recommendation_id.clone(),
                meeting_id: r.meeting_id.clone(),
                proposal_id: r.proposal_id.clone(),
                title: r.title.clone(),
                description: r.description.clone(),
                category: r.category.clone(),
                votes_for: r.votes_for,
                votes_against: r.votes_against,
                votes_abstain: r.votes_abstain,
                submitted_to_ceo: r.submitted_to_ceo,
                ceo_response: r.ceo_response.clone(),
            })
            .collect()
    }
}
