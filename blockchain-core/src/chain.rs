// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::block::{Block, BlockVerifyContext};
use crate::g8_scheduler::run_g8_scheduler;
use crate::state::{AccountState, ChainState};
use crate::transaction::{BlockTransaction, Transaction, TransactionPool};
use crate::types::{Amount, BlockchainError, ChainId, TxHash};
use genesis::{
    approve_sip, check_shutdown, emergency_shutdown, is_ceo_wallet, is_shutdown,
    verify_ceo_transaction, GenesisConfig, GenesisError, ShutdownCommand, CEO_WALLET,
};

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Deserialize)]
struct ChainPoIProof {
    miner_address: String,
    difficulty: u64,
    work_type: ChainWorkType,
    verification_data: serde_json::Value,
    work_hash: [u8; 32],
    timestamp: i64,
    nonce: u64,
    input_hash: [u8; 32],
    output_data: Vec<u8>,
    computation_metadata: serde_json::Value,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
enum ChainWorkType {
    MatrixMultiplication,
    GradientDescent,
    Inference,
}

fn validate_poi_difficulty(work_hash: &[u8; 32], difficulty: u64) -> Result<u64, BlockchainError> {
    if difficulty == 0 {
        return Err(BlockchainError::InvalidTransaction);
    }
    let mut prefix = [0u8; 16];
    prefix.copy_from_slice(&work_hash[..16]);
    let value = u128::from_be_bytes(prefix);
    let target = u128::MAX / difficulty as u128;
    if value > target {
        return Err(BlockchainError::InvalidTransaction);
    }
    Ok(difficulty)
}

/// Apply a block transaction to the chain state, routing to appropriate handlers
fn apply_block_transaction(state: &ChainState, tx: &BlockTransaction, block_height: u64) -> Result<(), BlockchainError> {
    match tx {
        BlockTransaction::Transfer(transfer_tx) => {
            state.apply_transaction(transfer_tx)
        }
        BlockTransaction::Reward(reward_tx) => {
            state.apply_reward_transaction(reward_tx)
        }
        BlockTransaction::Stake(stake_tx) => {
            state.apply_stake_transaction(stake_tx)
        }
        BlockTransaction::Unstake(unstake_tx) => {
            state.apply_unstake_transaction(unstake_tx)
        }
        BlockTransaction::ClaimStake(claim_tx) => {
            state.apply_claim_stake_transaction(claim_tx)
        }
        BlockTransaction::RegisterG8Candidate(g8_tx) => {
            // Validate signature eligibility
            g8_tx.validate()?;
            state.register_g8_candidate(g8_tx)
        }
        BlockTransaction::VoteG8(vote_tx) => {
            // Validate signature eligibility
            vote_tx.validate()?;
            state.record_g8_vote(vote_tx)
        }
        BlockTransaction::RecordG8Meeting(meeting_tx) => {
            // Validate CEO signature
            meeting_tx.validate()?;
            state.record_g8_meeting(meeting_tx)
        }
        BlockTransaction::RemoveG8Member(remove_tx) => {
            // Validate CEO signature
            remove_tx.validate()?;
            state.remove_g8_member(remove_tx)
        }
        BlockTransaction::RegisterModel(register_tx) => {
            // Validate and apply model registration (CEO-signed)
            register_tx.validate()?;
            state.apply_register_model_tx(register_tx, block_height)
        }
        BlockTransaction::UpdateConstitution(constitution_tx) => {
            // Validate and apply constitution update (CEO-signed)
            constitution_tx.validate()?;
            state.apply_update_constitution_tx(constitution_tx)
        }
        BlockTransaction::RecordTrainingRound(training_tx) => {
            // Validate and apply training round with PoI proof verification
            training_tx.validate()?;
            state.apply_record_training_round_tx(training_tx)
        }
        BlockTransaction::SubmitDaoProposal(dao_proposal_tx) => {
            // Validate and apply DAO proposal submission
            dao_proposal_tx.validate()?;
            state.apply_submit_dao_proposal_tx(dao_proposal_tx)
        }
        BlockTransaction::VoteDaoProposal(vote_tx) => {
            // Validate and apply DAO proposal vote
            vote_tx.validate()?;
            state.apply_vote_dao_proposal_tx(vote_tx)
        }
        BlockTransaction::CeoVetoDaoProposal(veto_tx) => {
            // Validate and apply CEO veto on DAO proposal
            veto_tx.validate()?;
            state.apply_ceo_veto_dao_proposal_tx(veto_tx)
        }
        BlockTransaction::Impeachment(impeachment_tx) => {
            // Validate and apply impeachment transaction
            impeachment_tx.validate()?;
            state.apply_impeachment_tx(impeachment_tx)
        }
        BlockTransaction::ImpeachmentVote(impeachment_vote_tx) => {
            // Validate and apply impeachment vote transaction
            impeachment_vote_tx.validate()?;
            state.apply_impeachment_vote_tx(impeachment_vote_tx)
        }
    }
}

fn calculate_poi_reward_local(proof: &ChainPoIProof) -> Result<Amount, BlockchainError> {
    let difficulty = validate_poi_difficulty(&proof.work_hash, proof.difficulty)?;
    let base = 100u64;
    let factor = (difficulty / 1000).max(1);
    let mut reward = base.saturating_mul(factor);

    match proof.work_type {
        ChainWorkType::GradientDescent => {
            reward = (reward as u128 * 120 / 100) as u64;
        }
        ChainWorkType::Inference => {
            reward = (reward as u128 * 110 / 100) as u64;
        }
        ChainWorkType::MatrixMultiplication => {}
    }

    Ok(Amount::new(reward))
}

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub shutdown: bool,
    pub genesis_config: GenesisConfig,
    pub chain_id: ChainId,
    pub state: ChainState,
    pub pending_transactions: TransactionPool,
    /// Pending extended transactions (CEO-signed operations like RegisterModel, UpdateConstitution)
    pub pending_extended_transactions: Vec<crate::transaction::BlockTransaction>,
}

impl Blockchain {
    pub fn new() -> Self {
        Self::new_with_genesis(GenesisConfig::default())
    }

    pub fn new_with_genesis(genesis_config: GenesisConfig) -> Self {
        let genesis_block = Block::genesis_block(&genesis_config);

        let chain_id = ChainId::from_str_id(&genesis_config.chain_id);

        let state = ChainState::new();
        // Any genesis allocations would be reflected here; for now we credit CEO with zero.
        state.set_balance(CEO_WALLET.to_string(), crate::types::Balance::zero());

        Blockchain {
            blocks: vec![genesis_block],
            shutdown: is_shutdown(),
            genesis_config,
            chain_id,
            state,
            pending_transactions: TransactionPool::new(),
            pending_extended_transactions: Vec::new(),
        }
    }

    pub fn add_block(&mut self, block: Block) -> Result<(), BlockchainError> {
        if block.header.ceo_signature.is_none() {
            check_shutdown()?;
        }

        let last_hash = self
            .blocks
            .last()
            .map(|b| b.block_hash)
            .unwrap_or(crate::types::BlockHash([0u8; 32]));

        if block.header.previous_hash != last_hash {
            return Err(BlockchainError::Genesis(GenesisError::InvalidPreviousHash));
        }

        block.verify()?;

        // Transactional state apply with rollback.
        let snapshot = self.state.snapshot();

        let apply_res: Result<(), BlockchainError> = (|| {
            self.state
                .set_validator_reward_address(CEO_WALLET.to_string());

            // Process standard transactions
            for tx in &block.transactions {
                self.state.apply_transaction(tx)?;
            }

            // Process extended transactions (G8 and other special txs)
            for ext_tx in &block.extended_transactions {
                apply_block_transaction(&self.state, ext_tx, block.header.block_height.value())?;
            }

            if block.header.ceo_signature.is_some() {
                return Ok(());
            }

            let proof_bytes = block
                .header
                .poi_proof
                .as_ref()
                .ok_or(BlockchainError::InvalidTransaction)?;
            let proof: ChainPoIProof = serde_json::from_slice(proof_bytes)?;
            let reward = calculate_poi_reward_local(&proof)?;
            self.state.mint_tokens(&proof.miner_address, reward)?;

            let committee = proof
                .verification_data
                .get("committee")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if !committee.is_empty() {
                let per = Amount::new(reward.value() / committee.len() as u64);
                for a in committee {
                    if let Some(addr) = a.as_str() {
                        self.state.mint_tokens(addr, per)?;
                    }
                }
            }

            Ok(())
        })();

        if let Err(e) = apply_res {
            self.state.restore(snapshot);
            return Err(e);
        }

        // Commit by pushing block.
        self.blocks.push(block);
        self.shutdown = is_shutdown()
            || self
                .blocks
                .last()
                .map(|b| b.header.shutdown_flag)
                .unwrap_or(false);

        // Run G8 scheduler for term-expiry triggers and auto-finalization
        let current_time = chrono::Utc::now().timestamp();
        if let Err(e) = run_g8_scheduler(current_time, &self.state) {
            eprintln!("G8 scheduler error: {:?}", e);
            // Don't fail the block for scheduler errors
        }

        Ok(())
    }

    pub fn validate_chain(&self) -> Result<(), BlockchainError> {
        let temp_state = ChainState::new();
        temp_state.set_balance(CEO_WALLET.to_string(), crate::types::Balance::zero());

        for (idx, block) in self.blocks.iter().enumerate() {
            if idx > 0 {
                let prev = &self.blocks[idx - 1];
                if block.header.previous_hash != prev.block_hash {
                    return Err(BlockchainError::Genesis(GenesisError::InvalidPreviousHash));
                }
            }
            block.verify_with_context(BlockVerifyContext::Historical)?;

            // Apply transactions to temp state
            let snapshot = temp_state.snapshot();
            let apply_res: Result<(), BlockchainError> = (|| {
                temp_state.set_validator_reward_address(CEO_WALLET.to_string());
                for tx in &block.transactions {
                    temp_state.apply_transaction(tx)?;
                }

                // Process extended transactions (G8 and other special txs)
                for ext_tx in &block.extended_transactions {
                    apply_block_transaction(&temp_state, ext_tx, block.header.block_height.value())?;
                }

                if block.header.ceo_signature.is_some() {
                    return Ok(());
                }

                let proof_bytes = block
                    .header
                    .poi_proof
                    .as_ref()
                    .ok_or(BlockchainError::InvalidTransaction)?;
                let proof: ChainPoIProof = serde_json::from_slice(proof_bytes)?;
                let reward = calculate_poi_reward_local(&proof)?;
                temp_state.mint_tokens(&proof.miner_address, reward)?;

                let committee = proof
                    .verification_data
                    .get("committee")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if !committee.is_empty() {
                    let per = Amount::new(reward.value() / committee.len() as u64);
                    for a in committee {
                        if let Some(addr) = a.as_str() {
                            temp_state.mint_tokens(addr, per)?;
                        }
                    }
                }

                Ok(())
            })();

            if let Err(e) = apply_res {
                temp_state.restore(snapshot);
                return Err(e);
            }
        }

        // Compare reconstructed state root with live state.
        let expected = self.state.calculate_state_root();
        let actual = temp_state.calculate_state_root();
        if expected != actual {
            return Err(BlockchainError::InvalidTransaction);
        }

        Ok(())
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.state.get_balance(address).amount().value()
    }

    pub fn get_account_state(&self, address: &str) -> AccountState {
        self.state.get_account_state(address)
    }

    pub fn get_state_root(&self) -> [u8; 32] {
        self.state.calculate_state_root()
    }

    pub fn is_ceo_wallet(&self, address: &str) -> bool {
        is_ceo_wallet(address)
    }

    pub fn add_transaction_to_pool(&mut self, tx: Transaction) -> Result<(), BlockchainError> {
        tx.validate()?;
        self.state.validate_transaction(&tx)?;
        self.pending_transactions.push(tx);
        self.pending_transactions.validate_pool()?;
        Ok(())
    }

    pub fn get_pending_transactions(&self, limit: usize) -> Vec<Transaction> {
        self.pending_transactions.peek_n(limit)
    }

    /// Add an extended transaction (CEO-signed) to the pending pool
    pub fn add_extended_transaction_to_pool(&mut self, tx: crate::transaction::BlockTransaction) -> Result<(), BlockchainError> {
        tx.validate()?;
        self.pending_extended_transactions.push(tx);
        Ok(())
    }

    /// Get pending extended transactions for block inclusion
    pub fn get_pending_extended_transactions(&self) -> Vec<crate::transaction::BlockTransaction> {
        self.pending_extended_transactions.clone()
    }

    /// Clear processed extended transactions from the pool
    pub fn clear_extended_transactions(&mut self, count: usize) {
        self.pending_extended_transactions.drain(0..count);
    }

    pub fn remove_transactions_from_pool(&mut self, hashes: &[TxHash]) {
        for h in hashes {
            let _ = self.pending_transactions.remove_by_hash(h);
        }
    }

    pub fn process_ceo_command(&mut self, tx: &Transaction) -> Result<(), BlockchainError> {
        if !tx.is_ceo_transaction() {
            return Err(BlockchainError::Genesis(GenesisError::UnauthorizedCaller));
        }

        verify_ceo_transaction(tx)?;

        // Decode CEO payload type from receiver field for now.
        match tx.receiver.as_str() {
            "SHUTDOWN" => {
                let payload = tx
                    .payload
                    .as_ref()
                    .ok_or(BlockchainError::InvalidTransaction)?;
                let command: ShutdownCommand = bincode::deserialize(payload).map_err(|e| {
                    BlockchainError::Genesis(GenesisError::SerializationError(e.to_string()))
                })?;
                emergency_shutdown(command)?;
                Ok(())
            }
            "APPROVE_SIP" => {
                let proposal_id = hex::encode(tx.tx_hash.0);
                let sig = tx
                    .ceo_signature
                    .clone()
                    .ok_or(BlockchainError::Genesis(GenesisError::InvalidSignature))?;
                approve_sip(&proposal_id, sig, &self.state)?;
                Ok(())
            }
            "MINT" => {
                self.state.mint_tokens(&tx.sender, tx.amount)?;
                Ok(())
            }
            "BURN" => {
                self.state.burn_tokens(&tx.sender, tx.amount)?;
                Ok(())
            }
            "TRANSFER" => {
                self.state.transfer(&tx.sender, &tx.receiver, tx.amount)?;
                Ok(())
            }
            _ => Err(GenesisError::UnauthorizedCaller),
        }
        .map_err(BlockchainError::Genesis)
    }

    pub fn fork_choice_rule<'a>(&'a self, other: &'a Blockchain) -> &'a Blockchain {
        if other.blocks.len() > self.blocks.len() {
            other
        } else {
            self
        }
    }
}

impl Default for Blockchain {
    fn default() -> Self {
        Self::new()
    }
}
