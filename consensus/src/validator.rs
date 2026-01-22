use crate::poi::ConsensusError;
use base64::Engine;
use blockchain_core::types::{Amount, BlockHash, BlockHeight};
use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use genesis::is_shutdown;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SlashReason {
    InvalidProof,
    NoResponse,
    ByzantineBehavior,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Validator {
    pub address: String,
    pub staked_amount: Amount,
    pub slashed_amount: Amount,
    pub last_validation_block: BlockHeight,
    pub is_active: bool,
    pub public_key: String,
    pub reputation_score: f64,
}

impl Validator {
    pub const MIN_STAKE: u64 = 1000;

    pub fn stake_value(&self) -> u64 {
        self.staked_amount.value()
    }
}

#[derive(Debug, Default)]
pub struct ValidatorRegistry {
    validators: DashMap<String, Validator>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self {
            validators: DashMap::new(),
        }
    }

    pub fn register_validator(
        &self,
        address: String,
        stake: Amount,
        pubkey: String,
    ) -> Result<(), ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        if stake.value() < Validator::MIN_STAKE {
            return Err(ConsensusError::RegistryError);
        }

        let v = Validator {
            address: address.clone(),
            staked_amount: stake,
            slashed_amount: Amount::ZERO,
            last_validation_block: BlockHeight::GENESIS,
            is_active: true,
            public_key: pubkey,
            reputation_score: 1.0,
        };

        self.validators.insert(address, v);
        Ok(())
    }

    pub fn deregister_validator(&self, address: &str) -> Result<Amount, ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        let removed = self.validators.remove(address);
        if let Some((_, v)) = removed {
            Ok(v.staked_amount)
        } else {
            Err(ConsensusError::RegistryError)
        }
    }

    pub fn slash_validator(
        &self,
        address: &str,
        amount: Amount,
        _reason: SlashReason,
    ) -> Result<(), ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        let mut entry = self
            .validators
            .get_mut(address)
            .ok_or(ConsensusError::RegistryError)?;

        let current = entry.staked_amount.value();
        let slash = amount.value().min(current);
        entry.staked_amount = Amount::new(current.saturating_sub(slash));
        entry.slashed_amount = Amount::new(entry.slashed_amount.value().saturating_add(slash));

        if entry.staked_amount.value() < Validator::MIN_STAKE {
            entry.is_active = false;
        }

        Ok(())
    }

    pub fn get_active_validators(&self) -> Vec<Validator> {
        self.validators
            .iter()
            .filter_map(|kv| {
                let v = kv.value();
                if v.is_active && v.staked_amount.value() >= Validator::MIN_STAKE {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_validator_stake(&self, address: &str) -> Option<Amount> {
        self.validators.get(address).map(|v| v.staked_amount)
    }

    pub fn deactivate_validator(&self, address: &str) -> Result<(), ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        let mut entry = self
            .validators
            .get_mut(address)
            .ok_or(ConsensusError::RegistryError)?;
        entry.is_active = false;
        Ok(())
    }
}

pub fn select_validators_weighted(
    candidates: &[Validator],
    count: usize,
) -> Result<Vec<Validator>, ConsensusError> {
    if is_shutdown() {
        return Ok(Vec::new());
    }

    let filtered: Vec<Validator> = candidates
        .iter()
        .filter(|v| v.is_active)
        .filter(|v| v.staked_amount.value() >= Validator::MIN_STAKE)
        .filter(|v| v.reputation_score >= 0.5)
        .cloned()
        .collect();

    if filtered.is_empty() || count == 0 {
        return Ok(Vec::new());
    }

    let total_stake: u64 = filtered.iter().map(|v| v.staked_amount.value()).sum();
    if total_stake == 0 {
        return Ok(Vec::new());
    }

    let mut rng = rand::thread_rng();
    let mut chosen = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let target = count.min(filtered.len());
    while chosen.len() < target {
        let mut roll = rng.gen_range(0..total_stake);
        for v in filtered.iter() {
            let mut w = v.staked_amount.value();
            if v.reputation_score > 0.9 {
                w = (w as u128 * 110 / 100) as u64;
            }

            if roll < w {
                if seen.insert(v.address.clone()) {
                    chosen.push(v.clone());
                }
                break;
            }

            roll = roll.saturating_sub(w);
        }
    }

    Ok(chosen)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorVote {
    pub validator_address: String,
    pub block_hash: BlockHash,
    pub vote: bool,
    pub signature: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuorumResult {
    Approved,
    Rejected,
    Pending,
}

#[derive(Debug, Default)]
pub struct VoteRegistry {
    votes: DashMap<BlockHash, Vec<ValidatorVote>>,
    committees: DashMap<BlockHash, Vec<String>>,
}

impl VoteRegistry {
    pub fn new() -> Self {
        Self {
            votes: DashMap::new(),
            committees: DashMap::new(),
        }
    }

    pub fn set_committee(&self, block_hash: BlockHash, committee: Vec<String>) {
        self.committees.insert(block_hash, committee);
    }

    pub fn clear(&self) {
        self.votes.clear();
        self.committees.clear();
    }

    pub fn submit_vote(
        &self,
        vote: ValidatorVote,
        registry: &ValidatorRegistry,
    ) -> Result<(), ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        let committee = self
            .committees
            .get(&vote.block_hash)
            .ok_or(ConsensusError::InvalidVote)?;
        if !committee
            .value()
            .iter()
            .any(|a| a == &vote.validator_address)
        {
            return Err(ConsensusError::InvalidVote);
        }

        let validator = registry
            .validators
            .get(&vote.validator_address)
            .ok_or(ConsensusError::InvalidVote)?;

        verify_validator_vote_signature(&vote, &validator.public_key)?;

        self.votes
            .entry(vote.block_hash)
            .and_modify(|votes| {
                if !votes
                    .iter()
                    .any(|v| v.validator_address == vote.validator_address)
                {
                    votes.push(vote.clone());
                }
            })
            .or_insert_with(|| vec![vote]);

        Ok(())
    }

    pub fn check_quorum(
        &self,
        block_hash: &BlockHash,
        total_validators: usize,
    ) -> Result<QuorumResult, ConsensusError> {
        if is_shutdown() {
            return Err(ConsensusError::ShutdownActive);
        }

        let votes = if let Some(v) = self.votes.get(block_hash) {
            v
        } else {
            return Ok(QuorumResult::Pending);
        };

        if total_validators == 0 {
            return Ok(QuorumResult::Rejected);
        }

        let yes = votes.value().iter().filter(|v| v.vote).count();
        let no = votes.value().iter().filter(|v| !v.vote).count();

        let threshold = (total_validators * 2).div_ceil(3);
        if yes >= threshold {
            return Ok(QuorumResult::Approved);
        }
        if no >= threshold {
            return Ok(QuorumResult::Rejected);
        }
        Ok(QuorumResult::Pending)
    }
}

fn verify_validator_vote_signature(
    vote: &ValidatorVote,
    pubkey: &str,
) -> Result<(), ConsensusError> {
    let pk_bytes = hex::decode(pubkey).map_err(|e| ConsensusError::Serialization(e.to_string()))?;
    let pk_arr: [u8; 32] = pk_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ConsensusError::InvalidVote)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| ConsensusError::InvalidVote)?;

    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&vote.signature)
        .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| ConsensusError::InvalidVote)?;

    let msg = bincode::serialize(&(vote.validator_address.clone(), vote.block_hash, vote.vote))?;
    vk.verify(&msg, &sig)
        .map_err(|_| ConsensusError::InvalidVote)?;

    Ok(())
}

pub fn execute_slashing(
    registry: &ValidatorRegistry,
    miner_address: &str,
    stake: Amount,
    reason: SlashReason,
) -> Result<Amount, ConsensusError> {
    if is_shutdown() {
        return Err(ConsensusError::ShutdownActive);
    }

    let pct = match reason {
        SlashReason::InvalidProof => 10u64,
        SlashReason::NoResponse => 5u64,
        SlashReason::ByzantineBehavior => 50u64,
    };
    let slash_amt = Amount::new(stake.value().saturating_mul(pct) / 100);
    registry.slash_validator(miner_address, slash_amt, reason)?;
    Ok(slash_amt)
}
