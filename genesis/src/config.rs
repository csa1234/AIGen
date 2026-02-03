// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::types::{GenesisError, WalletAddress};
use ed25519_dalek::VerifyingKey;
use hex::FromHex;
use serde::{Deserialize, Serialize};

pub const CEO_WALLET: &str = "0x0000000000000000000000000000000000000001";
pub const CEO_PUBLIC_KEY_HEX: &str =
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

pub const DEFAULT_CHAIN_ID: &str = "aigen-mainnet-1";
pub const DEFAULT_NETWORK_MAGIC: u64 = 0xA1A1_A1A1;
pub const DEFAULT_GENESIS_TIMESTAMP: i64 = 0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisConfig {
    pub ceo_wallet: WalletAddress,
    pub ceo_public_key_bytes: [u8; 32],
    pub chain_id: String,
    pub genesis_timestamp: i64,
    pub network_magic: u64,
}

impl GenesisConfig {
    pub fn new(
        chain_id: String,
        genesis_timestamp: i64,
        network_magic: u64,
    ) -> Result<Self, GenesisError> {
        let ceo_wallet = WalletAddress::new(CEO_WALLET.to_string())?;
        let ceo_public_key_bytes = parse_ceo_public_key_bytes()?;
        Ok(Self {
            ceo_wallet,
            ceo_public_key_bytes,
            chain_id,
            genesis_timestamp,
            network_magic,
        })
    }

    pub fn ceo_verifying_key(&self) -> Result<VerifyingKey, GenesisError> {
        VerifyingKey::from_bytes(&self.ceo_public_key_bytes)
            .map_err(|_| GenesisError::InvalidPublicKey)
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_CHAIN_ID.to_string(),
            DEFAULT_GENESIS_TIMESTAMP,
            DEFAULT_NETWORK_MAGIC,
        )
        .expect("valid compile-time genesis configuration")
    }
}

pub fn parse_ceo_public_key_bytes() -> Result<[u8; 32], GenesisError> {
    let bytes =
        <[u8; 32]>::from_hex(CEO_PUBLIC_KEY_HEX).map_err(|_| GenesisError::InvalidPublicKey)?;
    let _ = VerifyingKey::from_bytes(&bytes).map_err(|_| GenesisError::InvalidPublicKey)?;
    Ok(bytes)
}
