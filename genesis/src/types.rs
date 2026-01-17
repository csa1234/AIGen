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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SipProposal {
    pub proposal_hash: [u8; 32],
    pub proposal_id: String,
    pub description: String,
    pub code_changes_hash: [u8; 32],
    pub proposer: WalletAddress,
    pub timestamp: i64,
    pub ceo_approved: bool,
}

impl SipProposal {
    pub fn message_to_sign_for_approval(&self) -> Vec<u8> {
        format!(
            "sip_approve:{}:{:x?}",
            self.proposal_id, self.proposal_hash
        )
        .into_bytes()
    }

    pub fn message_to_sign_for_veto(&self) -> Vec<u8> {
        format!("sip_veto:{}:{:x?}", self.proposal_id, self.proposal_hash).into_bytes()
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
