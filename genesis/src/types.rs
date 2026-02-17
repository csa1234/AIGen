// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CeoSignature(pub Signature);

impl CeoSignature {
    pub fn inner(&self) -> &Signature {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct WalletAddress(pub String);

impl WalletAddress {
    pub fn new(address: String) -> Result<Self, GenesisError> {
        validate_wallet_address(&address)?;
        Ok(Self(address))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_wallet_address(addr: &str) -> Result<(), GenesisError> {
    const PREFIX: &str = "0x";
    if !addr.starts_with(PREFIX) {
        return Err(GenesisError::InvalidWalletAddress);
    }
    let hex_part = &addr[PREFIX.len()..];
    if hex_part.len() != 40 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(GenesisError::InvalidWalletAddress);
    }
    Ok(())
}

/// Status of a SIP proposal in the governance lifecycle.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum SipStatus {
    /// Proposal is pending review and voting
    Pending,
    /// Proposal has been approved (either manually by CEO or auto-approved with expired veto window)
    Approved,
    /// Proposal has been vetoed by CEO
    Vetoed,
    /// Proposal has been deployed to the network
    Deployed,
    /// Proposal was auto-approved by staker consensus, awaiting CEO 24h veto window expiration
    AutoApprovedPendingCeoWindow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SipProposal {
    pub proposal_hash: [u8; 32],
    pub proposal_id: String,
    pub description: String,
    pub code_changes_hash: [u8; 32],
    pub proposer: WalletAddress,
    pub timestamp: i64,
    pub ceo_approved: bool,
    pub auto_approved: bool,     // NEW: true if auto-approved by staker consensus
    pub approved_at: Option<i64>, // NEW: timestamp when approved (manual or auto)
    pub model_output_sample: Option<String>, // NEW: Sample output for constitutional review
    pub constitutional_violations: Vec<String>, // NEW: Violation descriptions if rejected
    pub constitution_version: Option<u32>, // NEW: Version of constitution used for review
    pub benevolence_score: Option<f32>, // NEW: ML-based benevolence scoring (0.0-1.0)
    pub benevolence_model_version: Option<String>, // NEW: Version of benevolence model used
    pub safety_oracle_votes: Option<String>, // NEW: JSON-serialized SafetyOracleResult
    /// SHA3-256 hash of the training proof JSON for PoI validation
    pub training_proof_hash: Option<[u8; 32]>,
    /// UUID reference to on-chain TrainingRound in ChainState
    pub training_round_id: Option<String>,
    /// JSON string of the training proof for PoI validation
    pub training_proof_json: Option<String>,
    /// Model ID associated with this proposal for PoI validation
    pub model_id: Option<String>,
    /// Unix timestamp when auto-approval occurred
    pub auto_approved_at: Option<i64>,
    /// Unix timestamp (auto_approved_at + 24h) for CEO veto window expiration
    pub ceo_veto_deadline: Option<i64>,
    /// Detailed rejection reasons for CEO review (constitutional violations, benevolence failures, oracle rejections, PoI failures)
    pub rejection_logs: Vec<String>,
    /// Current status of the proposal
    pub status: SipStatus,
}

impl SipProposal {
    /// Creates the message to be signed for approval.
    /// Includes training proof hash if present for cryptographic verification.
    pub fn message_to_sign_for_approval(&self) -> Vec<u8> {
        let training_proof_part = self.training_proof_hash
            .map(|h| format!(":{:x?}", h))
            .unwrap_or_default();
        format!("sip_approve:{}:{:x?}{}", self.proposal_id, self.proposal_hash, training_proof_part).into_bytes()
    }

    /// Creates the message to be signed for veto.
    /// Includes training proof hash if present for cryptographic verification.
    pub fn message_to_sign_for_veto(&self) -> Vec<u8> {
        let training_proof_part = self.training_proof_hash
            .map(|h| format!(":{:x?}", h))
            .unwrap_or_default();
        format!("sip_veto:{}:{:x?}{}", self.proposal_id, self.proposal_hash, training_proof_part).into_bytes()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShutdownCommand {
    pub timestamp: i64,
    pub reason: String,
    pub ceo_signature: CeoSignature,
    pub nonce: u64,
    pub network_magic: u64,
}

impl ShutdownCommand {
    pub fn message_to_sign(&self) -> Vec<u8> {
        format!(
            "shutdown:{}:{}:{}:{}",
            self.network_magic, self.timestamp, self.nonce, self.reason
        )
        .into_bytes()
    }
}

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum GenesisError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("unauthorized caller")]
    UnauthorizedCaller,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("invalid wallet address")]
    InvalidWalletAddress,
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("shutdown active")]
    ShutdownActive,
    #[error("replay attack detected")]
    ReplayAttack,
    #[error("proposal not found")]
    ProposalNotFound,
    #[error("duplicate proposal id")]
    DuplicateProposal,
    #[error("invalid merkle root")]
    InvalidMerkleRoot,
    #[error("invalid previous hash linkage")]
    InvalidPreviousHash,
    #[error("invalid network magic")]
    InvalidNetworkMagic,
    #[error("i/o error: {0}")]
    IoError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
}

impl From<std::io::Error> for GenesisError {
    fn from(e: std::io::Error) -> Self {
        GenesisError::IoError(e.to_string())
    }
}

impl From<serde_json::Error> for GenesisError {
    fn from(e: serde_json::Error) -> Self {
        GenesisError::SerializationError(e.to_string())
    }
}
