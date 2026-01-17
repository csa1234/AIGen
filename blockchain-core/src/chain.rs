use crate::block::{Block, BlockVerifyContext};
use crate::state::{AccountState, ChainState};
use crate::transaction::{Transaction, TransactionPool};
use crate::types::{Amount, BlockchainError, ChainId, TxHash};
use genesis::{
    approve_sip, check_shutdown, emergency_shutdown, is_ceo_wallet, is_shutdown,
    verify_ceo_transaction, CEO_WALLET, GenesisConfig, GenesisError, ShutdownCommand,
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

fn calculate_poi_reward_local(proof: &ChainPoIProof) -> Amount {
    let base = 100u64;
    let factor = (proof.difficulty / 1000).max(1);
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

    Amount::new(reward)
}

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub shutdown: bool,
    pub genesis_config: GenesisConfig,
    pub chain_id: ChainId,
    pub state: ChainState,
    pub pending_transactions: TransactionPool,
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

            for tx in &block.transactions {
                self.state.apply_transaction(tx)?;
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
            let reward = calculate_poi_reward_local(&proof);
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
        self.shutdown =
            is_shutdown() || self.blocks.last().map(|b| b.header.shutdown_flag).unwrap_or(false);

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
                temp_state
                    .set_validator_reward_address(CEO_WALLET.to_string());
                for tx in &block.transactions {
                    temp_state.apply_transaction(tx)?;
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
                let reward = calculate_poi_reward_local(&proof);
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
                let command: ShutdownCommand = bincode::deserialize(payload)
                    .map_err(|e| BlockchainError::Genesis(GenesisError::SerializationError(e.to_string())))?;
                emergency_shutdown(command)?;
                Ok(())
            }
            "APPROVE_SIP" => {
                let proposal_id = hex::encode(tx.tx_hash.0);
                let sig = tx
                    .ceo_signature
                    .clone()
                    .ok_or(BlockchainError::Genesis(GenesisError::InvalidSignature))?;
                approve_sip(&proposal_id, sig)?;
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
