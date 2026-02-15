// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Proof-of-Intelligence consensus scaffolding gated by Genesis shutdown checks.

pub mod poi;
pub mod state;
pub mod validator;

use std::sync::Arc;
use std::time::{Duration, Instant};

use blockchain_core::types::{Amount, BlockHeight};
use blockchain_core::ChainState;
use blockchain_core::{Block, BlockHash, Transaction};
use genesis::{check_shutdown, is_shutdown};
use tokio::sync::broadcast;
use tokio::sync::mpsc;

pub use crate::poi::verify_poi_proof;
use crate::poi::{calculate_poi_reward, check_shutdown_and_halt, ConsensusError, PoIBlockProducer};
use crate::state::{ConsensusEvent, ConsensusMetrics, ConsensusStateMachine};
use crate::validator::{QuorumResult, ValidatorRegistry, VoteRegistry};

pub use crate::poi::PoIProof;
pub use crate::poi::{
    set_inference_verification_config, CompressionMethod, ComputationMetadata,
    InferenceVerificationConfig, WorkType,
};
pub use crate::validator::ValidatorVote;

#[derive(Clone, Debug)]
pub enum NetworkMessage {
    Block(Block),
    ValidatorVote(ValidatorVote),
}

pub struct PoIConsensus {
    pub state_machine: ConsensusStateMachine,
    pub validator_registry: Arc<ValidatorRegistry>,
    pub vote_registry: Arc<VoteRegistry>,
    pub block_producer: PoIBlockProducer,
    pub metrics: Arc<ConsensusMetrics>,
    pub chain_state: Arc<ChainState>,
    shutdown_tx: broadcast::Sender<()>,
    network_publisher: Option<mpsc::Sender<NetworkMessage>>,
}

impl PoIConsensus {
    pub fn new(chain_state: Arc<ChainState>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);
        Self {
            state_machine: ConsensusStateMachine::new(),
            validator_registry: Arc::new(ValidatorRegistry::new()),
            vote_registry: Arc::new(VoteRegistry::new()),
            block_producer: PoIBlockProducer::default(),
            metrics: Arc::new(ConsensusMetrics::default()),
            chain_state,
            shutdown_tx,
            network_publisher: None,
        }
    }

    pub fn set_network_publisher(&mut self, tx: mpsc::Sender<NetworkMessage>) {
        self.network_publisher = Some(tx);
    }

    pub fn submit_validator_vote(&self, vote: ValidatorVote) -> Result<(), ConsensusError> {
        self.vote_registry
            .submit_vote(vote.clone(), &self.validator_registry)?;

        if let Some(tx) = &self.network_publisher {
            let _ = tx.try_send(NetworkMessage::ValidatorVote(vote));
        }

        Ok(())
    }

    pub async fn process_poi_submission(
        &mut self,
        proof: PoIProof,
        miner_address: String,
        transactions: Vec<Transaction>,
        previous_hash: BlockHash,
        next_height: BlockHeight,
    ) -> Result<Block, ConsensusError> {
        check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;
        self.metrics.inc_submitted();
        let started = Instant::now();

        if next_height.is_genesis() {
            return Err(ConsensusError::InvalidProof);
        }

        verify_poi_proof(&proof)?;

        let active = self.validator_registry.get_active_validators();
        let committee = crate::validator::select_validators_weighted(
            &active,
            self.block_producer.committee_size,
        )?;
        let committee_addrs: Vec<String> = committee.iter().map(|v| v.address.clone()).collect();

        self.state_machine
            .transition(ConsensusEvent::ProofSubmitted)?;

        let block = self.block_producer.produce_block(
            previous_hash,
            transactions,
            next_height.value(),
            proof,
            miner_address.clone(),
            committee_addrs.clone(),
        )?;

        self.vote_registry
            .set_committee(block.block_hash, committee_addrs);
        self.state_machine
            .transition(ConsensusEvent::ProofSubmitted)?;

        let quorum = self
            .wait_for_quorum(block.block_hash, Duration::from_secs(10), committee.len())
            .await?;

        match quorum {
            QuorumResult::Approved => {
                self.state_machine
                    .transition(ConsensusEvent::QuorumReached)?;
                let proof_bytes = block
                    .header
                    .poi_proof
                    .as_ref()
                    .ok_or(ConsensusError::InvalidProof)?;
                let decoded: PoIProof = serde_json::from_slice(proof_bytes)?;
                let reward = calculate_poi_reward(&decoded)?;
                self.metrics.add_rewards(reward.value());
                self.metrics.inc_accepted();
                self.metrics
                    .observe_validation_time_ms(started.elapsed().as_millis() as u64);
                self.state_machine
                    .transition(ConsensusEvent::QuorumReached)?;

                if let Some(tx) = &self.network_publisher {
                    let _ = tx.try_send(NetworkMessage::Block(block.clone()));
                }
                Ok(block)
            }
            QuorumResult::Rejected => {
                self.metrics.inc_rejected();
                self.metrics.inc_slashing();

                let _slashed = crate::validator::slash_for_poi_failure(
                    &self.validator_registry,
                    &self.chain_state,
                    &miner_address,
                )?;

                self.state_machine
                    .transition(ConsensusEvent::ProofInvalid)?;
                Err(ConsensusError::ProofRejected)
            }
            QuorumResult::Pending => {
                self.metrics.inc_rejected();
                Err(ConsensusError::Timeout)
            }
        }
    }

    pub async fn wait_for_quorum(
        &self,
        block_hash: BlockHash,
        timeout: Duration,
        total_validators: usize,
    ) -> Result<QuorumResult, ConsensusError> {
        check_shutdown_and_halt()?;

        let vote_registry = self.vote_registry.clone();
        tokio::time::timeout(timeout, async move {
            loop {
                if is_shutdown() {
                    return Err(ConsensusError::ShutdownActive);
                }
                let res = vote_registry.check_quorum(&block_hash, total_validators)?;
                if res != QuorumResult::Pending {
                    return Ok(res);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| ConsensusError::Timeout)?
    }

    pub fn start_shutdown_monitor(&self) {
        let tx = self.shutdown_tx.clone();
        let vote_registry = self.vote_registry.clone();
        tokio::spawn(async move {
            loop {
                if is_shutdown() {
                    vote_registry.clear();
                    let _ = tx.send(());
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    /// Slash training worker for invalid gradient proof
    pub fn slash_training_worker(
        &self,
        worker_address: &str,
    ) -> Result<Amount, ConsensusError> {
        let stake_state = self.chain_state.get_stake(worker_address)
            .ok_or(ConsensusError::RegistryError)?;

        // Slash 20% for training PoI failure
        let slash_amt = Amount::new(stake_state.staked_amount.value().saturating_mul(20) / 100);

        self.chain_state.apply_stake_slash(worker_address, slash_amt, "Training PoI failure")
            .map_err(|_| ConsensusError::RegistryError)?;

        Ok(slash_amt)
    }

    pub fn shutdown_subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
}
