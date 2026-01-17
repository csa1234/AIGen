//! Core blockchain primitives integrating with the immutable Genesis CEO controls.

pub mod crypto;
pub mod transaction;
pub mod block;
pub mod chain;
pub mod types;
pub mod state;

pub use crate::block::{Block, BlockHeader};
pub use crate::transaction::{Transaction, TransactionPool};
pub use crate::chain::Blockchain;
pub use crate::state::{AccountState, ChainState, StateRoot};
pub use crate::types::{
    Address,
    Amount,
    Balance,
    BlockHash,
    BlockHeight,
    BlockVersion,
    ChainId,
    Fee,
    Nonce,
    Timestamp,
    TxHash,
    BlockchainError,
    validate_address,
};
pub use crate::crypto::{
    hash_data,
    calculate_merkle_root,
    hash_block_header,
    hash_transaction,
    verify_merkle_proof,
    generate_merkle_proof,
    generate_keypair,
    sign_message,
    verify_signature,
    wallet_address_from_pubkey,
    derive_address_from_pubkey,
    validate_address_format,
    keccak256,
    blake3_hash,
    constant_time_compare,
};

pub use genesis::{
    GenesisConfig,
    CeoSignature,
    emergency_shutdown,
    is_shutdown,
    check_shutdown,
    CEO_WALLET,
};
