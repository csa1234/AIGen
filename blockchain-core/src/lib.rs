// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Core blockchain primitives integrating with the immutable Genesis CEO controls.

pub mod block;
pub mod chain;
pub mod crypto;
pub mod g8_scheduler;
pub mod state;
pub mod transaction;
pub mod types;

pub use crate::block::{Block, BlockHeader};
pub use crate::chain::Blockchain;
pub use crate::crypto::{
    blake3_hash, calculate_merkle_root, constant_time_compare, derive_address_from_pubkey,
    generate_keypair, generate_merkle_proof, hash_block_header, hash_data, hash_transaction,
    keccak256, sign_message, validate_address_format, verify_merkle_proof, verify_signature,
    wallet_address_from_pubkey,
};
pub use crate::state::{
    AccountState, BatchJobState, ChainState, ChainStateSnapshot, StateRoot, SubscriptionState, StakeState, StakeRole,
};
pub use crate::transaction::{Transaction, TransactionPool, RewardTx, RewardType, StakeTx, UnstakeTx, ClaimStakeTx};
pub use crate::types::{
    validate_address, Address, Amount, Balance, BlockHash, BlockHeight, BlockVersion,
    BlockchainError, ChainId, Fee, Nonce, Timestamp, TxHash,
};

pub use genesis::{
    check_shutdown, emergency_shutdown, is_shutdown, CeoSignature, GenesisConfig, CEO_WALLET,
};
