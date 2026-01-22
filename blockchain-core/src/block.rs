use crate::crypto::{calculate_merkle_root, hash_block_header};
use crate::transaction::Transaction;
use crate::types::{Amount, BlockHash, BlockHeight, BlockVersion, BlockchainError, Timestamp};
use genesis::{is_shutdown, verify_ceo_signature, CeoSignature, GenesisConfig, GenesisError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockVerifyContext {
    NewBlock,
    Historical,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: BlockVersion,
    pub previous_hash: BlockHash,
    pub merkle_root: BlockHash,
    pub timestamp: Timestamp,
    pub poi_proof: Option<Vec<u8>>, // placeholder for PoI data
    pub ceo_signature: Option<CeoSignature>,
    pub shutdown_flag: bool,
    pub block_height: BlockHeight,
    pub priority: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub block_hash: BlockHash,
}

impl Block {
    pub fn new(
        previous_hash: [u8; 32],
        transactions: Vec<Transaction>,
        block_height: u64,
        version: u32,
        poi_proof: Option<Vec<u8>>,
    ) -> Self {
        let shutdown_flag = is_shutdown();
        let merkle_root = BlockHash(calculate_merkle_root(&transactions));
        let header = BlockHeader {
            version: BlockVersion(version),
            previous_hash: BlockHash(previous_hash),
            merkle_root,
            timestamp: Timestamp(chrono::Utc::now().timestamp()),
            poi_proof,
            ceo_signature: None,
            shutdown_flag,
            block_height: BlockHeight(block_height),
            priority: false,
        };
        let block_hash = hash_block_header(&header);
        Block {
            header,
            transactions,
            block_hash,
        }
    }

    pub fn new_ceo_block(
        previous_hash: BlockHash,
        transactions: Vec<Transaction>,
        ceo_signature: CeoSignature,
    ) -> Result<Block, BlockchainError> {
        let merkle_root = BlockHash(calculate_merkle_root(&transactions));
        let header = BlockHeader {
            version: BlockVersion(BlockVersion::CURRENT),
            previous_hash,
            merkle_root,
            timestamp: Timestamp(chrono::Utc::now().timestamp()),
            poi_proof: None,
            ceo_signature: Some(ceo_signature),
            shutdown_flag: is_shutdown(),
            block_height: BlockHeight(0),
            priority: true,
        };
        let block_hash = hash_block_header(&header);
        Ok(Block {
            header,
            transactions,
            block_hash,
        })
    }

    pub fn genesis_block(config: &GenesisConfig) -> Self {
        // Simple genesis block with no transactions yet; CEO allocation handled off-chain.
        let previous_hash = BlockHash([0u8; 32]);
        let transactions = Vec::new();
        let shutdown_flag = is_shutdown();
        let merkle_root = BlockHash(calculate_merkle_root(&transactions));
        let header = BlockHeader {
            version: BlockVersion(BlockVersion::CURRENT),
            previous_hash,
            merkle_root,
            timestamp: Timestamp(config.genesis_timestamp),
            poi_proof: None,
            ceo_signature: None,
            shutdown_flag,
            block_height: BlockHeight::GENESIS,
            priority: true,
        };
        let block_hash = hash_block_header(&header);
        Block {
            header,
            transactions,
            block_hash,
        }
    }

    pub fn is_genesis(&self) -> bool {
        self.header.block_height.is_genesis()
    }

    pub fn calculate_block_reward(&self) -> Amount {
        if let Some(proof) = &self.header.poi_proof {
            Amount::new(proof.len() as u64)
        } else {
            Amount::ZERO
        }
    }

    pub fn validate_header(&self) -> Result<(), BlockchainError> {
        if !self.header.version.is_supported() {
            return Err(BlockchainError::UnsupportedBlockVersion(
                self.header.version.0,
            ));
        }

        let now = chrono::Utc::now().timestamp();
        self.header.timestamp.validate_reasonable_now(now)?;

        if self.header.block_height.is_genesis() && self.header.previous_hash.0 != [0u8; 32] {
            return Err(BlockchainError::Genesis(GenesisError::InvalidPreviousHash));
        }

        let expected_merkle = calculate_merkle_root(&self.transactions);
        if self.header.merkle_root.0 != expected_merkle {
            return Err(BlockchainError::Genesis(GenesisError::InvalidMerkleRoot));
        }

        Ok(())
    }

    pub fn validate_transactions(&self) -> Result<(), BlockchainError> {
        for tx in &self.transactions {
            tx.validate()?;
        }
        Ok(())
    }

    pub fn verify(&self) -> Result<(), BlockchainError> {
        self.verify_with_context(BlockVerifyContext::NewBlock)
    }

    pub fn verify_with_context(&self, ctx: BlockVerifyContext) -> Result<(), BlockchainError> {
        self.validate_header()?;
        self.validate_transactions()?;

        if let Some(sig) = &self.header.ceo_signature {
            verify_ceo_signature(self.block_hash.0.as_ref(), sig)?;
            // CEO-signed blocks are administrative overrides.
            // They may be accepted even during shutdown for recovery.
            return Ok(());
        }

        if self.header.poi_proof.is_none() {
            return Err(BlockchainError::InvalidTransaction);
        }

        if ctx == BlockVerifyContext::NewBlock {
            let current_shutdown = is_shutdown();
            if current_shutdown != self.header.shutdown_flag {
                return Err(BlockchainError::Genesis(GenesisError::ShutdownActive));
            }
        }

        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, BlockchainError> {
        Ok(bincode::serialize(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BlockchainError> {
        Ok(bincode::deserialize(bytes)?)
    }
}
