use crate::poi::ConsensusError;
use blockchain_core::types::BlockHash;
use genesis::is_shutdown;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsensusState {
    Active,
    Shutdown,
    Syncing,
    ProposingBlock,
    ValidatingProof,
    Finalizing,
    Slashing,
}

impl ConsensusState {
    pub fn check_and_update(self) -> Self {
        if is_shutdown() {
            ConsensusState::Shutdown
        } else {
            self
        }
    }

    pub fn can_process_blocks(&self) -> bool {
        !matches!(self, ConsensusState::Shutdown) && !is_shutdown()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConsensusEvent {
    ProofSubmitted,
    ValidatorVoted,
    QuorumReached,
    ProofInvalid,
    ShutdownTriggered,
}

#[derive(Debug)]
pub struct ConsensusStateMachine {
    pub current_state: ConsensusState,
    pub block_candidate_hash: Option<BlockHash>,
    pub validator_votes: u64,
}

impl Default for ConsensusStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsensusStateMachine {
    pub fn new() -> Self {
        Self {
            current_state: ConsensusState::Active,
            block_candidate_hash: None,
            validator_votes: 0,
        }
    }

    pub fn transition(&mut self, event: ConsensusEvent) -> Result<ConsensusState, ConsensusError> {
        if is_shutdown() {
            self.current_state = ConsensusState::Shutdown;
            return Ok(self.current_state.clone());
        }

        let next = match (&self.current_state, event) {
            (_, ConsensusEvent::ShutdownTriggered) => ConsensusState::Shutdown,
            (ConsensusState::Active, ConsensusEvent::ProofSubmitted) => ConsensusState::ProposingBlock,
            (ConsensusState::ProposingBlock, ConsensusEvent::ProofSubmitted) => ConsensusState::ValidatingProof,
            (ConsensusState::ValidatingProof, ConsensusEvent::ValidatorVoted) => {
                self.validator_votes = self.validator_votes.saturating_add(1);
                ConsensusState::ValidatingProof
            }
            (ConsensusState::ValidatingProof, ConsensusEvent::QuorumReached) => ConsensusState::Finalizing,
            (ConsensusState::ValidatingProof, ConsensusEvent::ProofInvalid) => ConsensusState::Slashing,
            (ConsensusState::Slashing, ConsensusEvent::ProofInvalid) => ConsensusState::Active,
            (ConsensusState::Finalizing, ConsensusEvent::QuorumReached) => ConsensusState::Active,
            (_, _) => self.current_state.clone(),
        };

        self.current_state = next.clone();
        Ok(next)
    }
}

#[derive(Debug, Default)]
pub struct ConsensusMetrics {
    pub total_proofs_submitted: AtomicU64,
    pub total_proofs_accepted: AtomicU64,
    pub total_proofs_rejected: AtomicU64,
    pub total_slashing_events: AtomicU64,
    pub total_rewards_minted: AtomicU64,
    pub average_validation_time_ms: AtomicU64,
}

impl ConsensusMetrics {
    pub fn inc_submitted(&self) {
        self.total_proofs_submitted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_accepted(&self) {
        self.total_proofs_accepted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_rejected(&self) {
        self.total_proofs_rejected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_slashing(&self) {
        self.total_slashing_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_rewards(&self, amount: u64) {
        self.total_rewards_minted.fetch_add(amount, Ordering::Relaxed);
    }

    pub fn observe_validation_time_ms(&self, sample_ms: u64) {
        let prev = self.average_validation_time_ms.load(Ordering::Relaxed);
        let next = if prev == 0 { sample_ms } else { (prev + sample_ms) / 2 };
        self.average_validation_time_ms.store(next, Ordering::Relaxed);
    }
}
