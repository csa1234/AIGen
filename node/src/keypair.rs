// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use libp2p::identity::Keypair;
use serde::{Serialize, Deserialize};

/// Authentication token for pipeline replica join/registration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineAuthToken {
    pub node_id: String,
    pub block_id: u32,
    pub timestamp: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl PipelineAuthToken {
    /// Token expiry time in seconds (5 minutes)
    pub const EXPIRY_SECONDS: u64 = 300;
    
    /// Create a new token (without signature - used for signing)
    fn new_unsigned(node_id: String, block_id: u32, public_key: Vec<u8>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Self {
            node_id,
            block_id,
            timestamp,
            public_key,
            signature: Vec::new(),
        }
    }
    
    /// Serialize token data for signing (excludes signature)
    fn signable_bytes(&self) -> Vec<u8> {
        let data = (
            &self.node_id,
            self.block_id,
            self.timestamp,
            &self.public_key,
        );
        serde_json::to_vec(&data).unwrap_or_default()
    }
    
    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp) > Self::EXPIRY_SECONDS
    }
}

/// Generate a pipeline authentication token using Ed25519
pub fn generate_pipeline_auth_token(
    keypair: &Keypair,
    node_id: String,
    block_id: u32,
) -> Result<PipelineAuthToken> {
    // Extract public key
    let public_key = keypair.public().encode_protobuf();
    
    // Create unsigned token
    let mut token = PipelineAuthToken::new_unsigned(node_id, block_id, public_key);
    
    // Sign the token data
    let signable = token.signable_bytes();
    let signature = keypair
        .sign(&signable)
        .map_err(|e| anyhow!("failed to sign pipeline auth token: {}", e))?;
    
    token.signature = signature;
    
    Ok(token)
}

/// Verify a pipeline authentication token using Ed25519
pub fn verify_pipeline_auth_token(token: &PipelineAuthToken) -> Result<()> {
    // Check expiry
    if token.is_expired() {
        return Err(anyhow!("pipeline auth token has expired"));
    }
    
    // Reconstruct public key from protobuf encoding
    let public_key = libp2p::identity::PublicKey::from_protobuf_encoding(&token.public_key)
        .map_err(|e| anyhow!("failed to decode public key: {}", e))?;
    
    // Create unsigned token copy for verification
    let mut unsigned = token.clone();
    unsigned.signature = Vec::new();
    let signable = unsigned.signable_bytes();
    
    // Verify signature
    if !public_key.verify(&signable, &token.signature) {
        return Err(anyhow!("pipeline auth token signature verification failed"));
    }
    
    Ok(())
}

pub fn generate_keypair() -> Keypair {
    Keypair::generate_ed25519()
}

pub fn keypair_exists(path: &Path) -> bool {
    path.exists()
}

pub fn default_keypair_path(data_dir: &Path) -> PathBuf {
    data_dir.join("node_keypair.bin")
}

pub fn save_keypair(keypair: &Keypair, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create keypair parent directory: {}",
                parent.display()
            )
        })?;
    }

    let bytes = keypair
        .to_protobuf_encoding()
        .context("failed to encode keypair")?;

    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write keypair file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms).with_context(|| {
            format!(
                "failed to set permissions on keypair file: {}",
                path.display()
            )
        })?;
    }

    #[cfg(windows)]
    {
        // Best-effort: Windows ACL hardening is not implemented here without extra dependencies.
        // The file will inherit the directory ACLs.
    }

    Ok(())
}

pub fn load_keypair(path: &Path) -> Result<Keypair> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read keypair file: {}", path.display()))?;

    let keypair = Keypair::from_protobuf_encoding(&bytes)
        .map_err(|e| anyhow!("failed to decode keypair protobuf: {e}"))?;

    validate_keypair(&keypair)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .with_context(|| format!("failed to stat keypair file: {}", path.display()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            eprintln!(
                "keypair file permissions are too permissive: path={}, mode={:o}",
                path.display(),
                mode
            );
        }
    }

    Ok(keypair)
}

fn validate_keypair(keypair: &Keypair) -> Result<()> {
    let msg = b"aigen-keypair-validation".as_ref();
    let sig = keypair
        .sign(msg)
        .map_err(|e| anyhow!("keypair failed to sign test message: {e}"))?;
    let pk = keypair.public();
    if !pk.verify(msg, &sig) {
        return Err(anyhow!("keypair signature verification failed"));
    }
    Ok(())
}
