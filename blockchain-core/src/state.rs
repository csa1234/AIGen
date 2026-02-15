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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainStateSnapshot {
    pub accounts: BTreeMap<Address, AccountState>,
    pub subscriptions: BTreeMap<Address, SubscriptionState>,
    pub batch_jobs: BTreeMap<String, BatchJobState>,
    pub model_proposals: BTreeMap<String, ModelUpgradeProposalState>,
    pub governance_votes: Vec<GovernanceVote>,
    pub stakes: BTreeMap<Address, StakeState>,
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
        ChainState {
            accounts: RwLock::new(accounts),
            subscriptions: RwLock::new(subscriptions),
            batch_jobs: RwLock::new(batch_jobs),
            model_proposals: RwLock::new(model_proposals),
            governance_votes: RwLock::new(governance_votes),
            stakes: RwLock::new(stakes),
            validator_reward_address: RwLock::new(validator_reward_address),
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
        }
    }

    pub fn restore(&self, snapshot: ChainStateSnapshot) {
        *self.accounts.write() = snapshot.accounts;
        *self.subscriptions.write() = snapshot.subscriptions;
        *self.batch_jobs.write() = snapshot.batch_jobs;
        *self.model_proposals.write() = snapshot.model_proposals;
        *self.governance_votes.write() = snapshot.governance_votes;
        *self.stakes.write() = snapshot.stakes;
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
}

impl genesis::ChainStateProvider for ChainState {
    fn check_auto_approve_threshold(
        &self,
        proposal_id: &str,
        min_approval_percentage: f64,
        min_participation_percentage: f64,
    ) -> bool {
        self.check_auto_approve_threshold(
            proposal_id,
            min_approval_percentage,
            min_participation_percentage,
        )
    }
}
