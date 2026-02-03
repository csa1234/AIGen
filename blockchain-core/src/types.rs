// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::fmt;
use std::str::FromStr;

use genesis::GenesisError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Address = String;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockHeight(pub u64);

impl BlockHeight {
    pub const GENESIS: BlockHeight = BlockHeight(0);

    pub fn new_genesis() -> Self {
        BlockHeight::GENESIS
    }

    pub fn new_non_genesis(height: u64) -> Result<Self, BlockchainError> {
        if height == 0 {
            return Err(BlockchainError::InvalidBlockHeight);
        }
        Ok(BlockHeight(height))
    }

    pub fn new(height: u64) -> Self {
        BlockHeight(height)
    }

    pub fn is_genesis(&self) -> bool {
        self.0 == 0
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn next(&self) -> Result<BlockHeight, BlockchainError> {
        self.0
            .checked_add(1)
            .map(BlockHeight)
            .ok_or(BlockchainError::BlockHeightOverflow)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockVersion(pub u32);

impl BlockVersion {
    pub const CURRENT: u32 = 1;

    pub fn new(version: u32) -> Result<Self, BlockchainError> {
        if Self::is_supported_raw(version) {
            Ok(BlockVersion(version))
        } else {
            Err(BlockchainError::UnsupportedBlockVersion(version))
        }
    }

    pub fn current() -> Self {
        BlockVersion(Self::CURRENT)
    }

    pub fn is_supported(&self) -> bool {
        Self::is_supported_raw(self.0)
    }

    fn is_supported_raw(v: u32) -> bool {
        v == Self::CURRENT
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(pub u64);

impl Amount {
    pub const ZERO: Amount = Amount(0);

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn new(value: u64) -> Self {
        Amount(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn checked_add(self, other: Amount) -> Option<Amount> {
        self.0.checked_add(other.0).map(Amount)
    }

    pub fn checked_sub(self, other: Amount) -> Option<Amount> {
        self.0.checked_sub(other.0).map(Amount)
    }

    pub fn safe_add(self, other: Amount) -> Result<Amount, BlockchainError> {
        self.checked_add(other)
            .ok_or(BlockchainError::AmountOverflow)
    }

    pub fn safe_sub(self, other: Amount) -> Result<Amount, BlockchainError> {
        self.checked_sub(other)
            .ok_or(BlockchainError::AmountUnderflow)
    }

    pub fn saturating_add(self, other: Amount) -> Amount {
        Amount(self.0.saturating_add(other.0))
    }

    pub fn saturating_sub(self, other: Amount) -> Amount {
        Amount(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nonce(pub u64);

impl Nonce {
    pub const ZERO: Nonce = Nonce(0);

    pub fn new(value: u64) -> Self {
        Nonce(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    pub fn next(&self) -> Nonce {
        Nonce(self.0.wrapping_add(1))
    }

    pub fn checked_next(&self) -> Option<Nonce> {
        self.0.checked_add(1).map(Nonce)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub const MAX_FUTURE_SKEW_SECS: i64 = 300;
    pub const MAX_PAST_SKEW_SECS: i64 = 86_400 * 365;

    pub fn new(value: i64) -> Result<Self, BlockchainError> {
        if value < 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(Timestamp(value))
    }

    pub fn value(&self) -> i64 {
        self.0
    }

    pub fn validate_reasonable_now(&self, now: i64) -> Result<(), BlockchainError> {
        let ts = self.0;
        if ts <= 0 {
            return Err(BlockchainError::InvalidTimestamp);
        }
        if ts > now + Self::MAX_FUTURE_SKEW_SECS || ts < now - Self::MAX_PAST_SKEW_SECS {
            return Err(BlockchainError::InvalidTimestamp);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockHash(pub [u8; 32]);

impl fmt::Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockHash(0x{})", hex::encode(self.0))
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(v: [u8; 32]) -> Self {
        BlockHash(v)
    }
}

impl FromStr for BlockHash {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|_| BlockchainError::InvalidHashFormat)?;
        if bytes.len() != 32 {
            return Err(BlockchainError::InvalidHashFormat);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(BlockHash(arr))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxHash(pub [u8; 32]);

impl fmt::Debug for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxHash(0x{})", hex::encode(self.0))
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl From<[u8; 32]> for TxHash {
    fn from(v: [u8; 32]) -> Self {
        TxHash(v)
    }
}

impl FromStr for TxHash {
    type Err = BlockchainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|_| BlockchainError::InvalidHashFormat)?;
        if bytes.len() != 32 {
            return Err(BlockchainError::InvalidHashFormat);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(TxHash(arr))
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId(pub u64);

impl ChainId {
    pub fn new(id: u64) -> Self {
        ChainId(id)
    }

    pub fn from_str_id(id: &str) -> Self {
        let hash = blake3::hash(id.as_bytes());
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        ChainId(u64::from_le_bytes(bytes))
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Balance(pub Amount);

impl Balance {
    pub fn zero() -> Self {
        Balance(Amount::ZERO)
    }

    pub fn amount(&self) -> Amount {
        self.0
    }

    pub fn safe_add(self, other: Amount) -> Result<Balance, BlockchainError> {
        Ok(Balance(self.0.safe_add(other)?))
    }

    pub fn safe_sub(self, other: Amount) -> Result<Balance, BlockchainError> {
        Ok(Balance(self.0.safe_sub(other)?))
    }

    pub fn checked_sub(self, other: Amount) -> Option<Balance> {
        self.0.checked_sub(other).map(Balance)
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fee {
    pub base_fee: Amount,
    pub priority_fee: Amount,
    pub burn_amount: Amount,
}

impl Fee {
    pub fn new(base_fee: Amount, priority_fee: Amount) -> Self {
        let total = base_fee.saturating_add(priority_fee);
        let burn_raw = total.value().saturating_mul(40) / 100;
        Fee {
            base_fee,
            priority_fee,
            burn_amount: Amount::new(burn_raw),
        }
    }

    pub fn total_fee(&self) -> Amount {
        self.base_fee.saturating_add(self.priority_fee)
    }

    pub fn validator_share(&self) -> Amount {
        let total = self.total_fee().value();
        Amount::new(total.saturating_mul(50) / 100)
    }

    pub fn dev_share(&self) -> Amount {
        let total = self.total_fee().value();
        let validator = self.validator_share().value();
        let burn = self.burn_amount.value();
        Amount::new(total.saturating_sub(validator).saturating_sub(burn))
    }
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum BlockchainError {
    #[error("genesis error: {0}")]
    Genesis(GenesisError),
    #[error("invalid block height")]
    InvalidBlockHeight,
    #[error("block height overflow")]
    BlockHeightOverflow,
    #[error("unsupported block version: {0}")]
    UnsupportedBlockVersion(u32),
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("invalid amount")]
    InvalidAmount,
    #[error("invalid nonce")]
    InvalidNonce,
    #[error("invalid transaction")]
    InvalidTransaction,
    #[error("invalid fee")]
    InvalidFee,
    #[error("amount overflow")]
    AmountOverflow,
    #[error("amount underflow")]
    AmountUnderflow,
    #[error("invalid address format")]
    InvalidAddress,
    #[error("invalid hash format")]
    InvalidHashFormat,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("duplicate nonce")]
    DuplicateNonce,
    #[error("serialization error: {0}")]
    SerializationError(String),
}

impl From<GenesisError> for BlockchainError {
    fn from(err: GenesisError) -> Self {
        BlockchainError::Genesis(err)
    }
}

impl From<bincode::Error> for BlockchainError {
    fn from(err: bincode::Error) -> Self {
        BlockchainError::SerializationError(err.to_string())
    }
}

impl From<serde_json::Error> for BlockchainError {
    fn from(err: serde_json::Error) -> Self {
        BlockchainError::SerializationError(err.to_string())
    }
}

pub fn validate_address(addr: &str) -> Result<(), BlockchainError> {
    if !addr.starts_with("0x") {
        return Err(BlockchainError::InvalidAddress);
    }
    let hex_part = &addr[2..];
    if hex_part.len() != 40 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BlockchainError::InvalidAddress);
    }
    Ok(())
}

pub fn validate_address_strict(addr: &str) -> Result<(), BlockchainError> {
    validate_address(addr)
}
