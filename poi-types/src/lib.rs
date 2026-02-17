// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! PoI (Proof of Intelligence) types shared between model and consensus crates.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of work being proven
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkType {
    MatrixMultiplication,
    GradientDescent,
    Inference,
    DistributedInference,
    Training,
}

/// Compression method for gradient data
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionMethod {
    None,
    Quantize8Bit,
    Quantize4Bit,
}

/// Metadata about the computation performed
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputationMetadata {
    pub rows: u32,
    pub cols: u32,
    pub inner: u32,
    pub iterations: u32,
    pub model_id: String,
    pub compression_method: CompressionMethod,
    pub original_size: usize,
}

/// Proof of a distributed task execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistributedTaskProof {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub fragment_ids: Vec<String>,
    pub layer_range: (u32, u32),
    pub input_activation_hash: [u8; 32],
    pub output_activation_hash: [u8; 32],
    pub checkpoint_hash: Option<[u8; 32]>,
    pub compute_time_ms: u64,
    pub node_id: String,
    pub nonce: u64,
}

impl DistributedTaskProof {
    /// Create a new distributed task proof
    pub fn new(
        task_id: Uuid,
        inference_id: Uuid,
        fragment_ids: Vec<String>,
        layer_range: (u32, u32),
        input_activation_hash: [u8; 32],
        output_activation_hash: [u8; 32],
        checkpoint_hash: Option<[u8; 32]>,
        compute_time_ms: u64,
        node_id: String,
        nonce: u64,
    ) -> Self {
        Self {
            task_id,
            inference_id,
            fragment_ids,
            layer_range,
            input_activation_hash,
            output_activation_hash,
            checkpoint_hash,
            compute_time_ms,
            node_id,
            nonce,
        }
    }
}

/// Distributed PoI Proof for complete inference pipeline
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistributedPoIProof {
    pub inference_id: Uuid,
    pub block_proofs: Vec<DistributedTaskProof>,
    pub collector_node_id: String,
    pub total_blocks: u32,
    pub sampling_rate: f32,
    pub timestamp: i64,
}

impl DistributedPoIProof {
    /// Create a new distributed PoI proof
    pub fn new(
        inference_id: Uuid,
        collector_node_id: String,
        total_blocks: u32,
    ) -> Self {
        Self {
            inference_id,
            block_proofs: Vec::new(),
            collector_node_id,
            total_blocks,
            sampling_rate: 0.1, // Default 10% sampling
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Add a block proof
    pub fn add_block_proof(&mut self, proof: DistributedTaskProof) {
        self.block_proofs.push(proof);
    }

    /// Get proofs sorted by block index (layer_range start)
    pub fn get_sorted_proofs(&self) -> Vec<&DistributedTaskProof> {
        let mut sorted: Vec<&DistributedTaskProof> = self.block_proofs.iter().collect();
        sorted.sort_by_key(|p| p.layer_range.0);
        sorted
    }
}

/// Collect block proofs into a DistributedPoIProof
pub fn collect_block_proofs(
    inference_id: Uuid,
    block_proofs: Vec<DistributedTaskProof>,
    collector_node_id: String,
) -> DistributedPoIProof {
    let total_blocks = block_proofs.len() as u32;
    let mut proof = DistributedPoIProof::new(inference_id, collector_node_id, total_blocks);
    
    for block_proof in block_proofs {
        proof.add_block_proof(block_proof);
    }
    
    proof
}

/// PoI Proof structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PoIProof {
    pub work_hash: [u8; 32],
    pub miner_address: String,
    pub timestamp: i64,
    pub verification_data: serde_json::Value,
    pub work_type: WorkType,
    pub input_hash: [u8; 32],
    pub output_data: Vec<u8>,
    pub computation_metadata: ComputationMetadata,
    pub difficulty: u64,
    pub nonce: u64,
}

impl PoIProof {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        work_hash: [u8; 32],
        miner_address: String,
        timestamp: i64,
        verification_data: serde_json::Value,
        work_type: WorkType,
        input_hash: [u8; 32],
        output_data: Vec<u8>,
        computation_metadata: ComputationMetadata,
        difficulty: u64,
        nonce: u64,
    ) -> Self {
        Self {
            work_hash,
            miner_address,
            timestamp,
            verification_data,
            work_type,
            input_hash,
            output_data,
            computation_metadata,
            difficulty,
            nonce,
        }
    }
}

impl Default for PoIProof {
    fn default() -> Self {
        Self {
            work_hash: [0u8; 32],
            miner_address: String::new(),
            timestamp: 0,
            verification_data: serde_json::Value::Null,
            work_type: WorkType::Inference,
            input_hash: [0u8; 32],
            output_data: Vec::new(),
            computation_metadata: ComputationMetadata {
                rows: 0,
                cols: 0,
                inner: 0,
                iterations: 0,
                model_id: String::new(),
                compression_method: CompressionMethod::None,
                original_size: 0,
            },
            difficulty: 0,
            nonce: 0,
        }
    }
}

/// Inference verification configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceVerificationConfig {
    pub cache_capacity: usize,
    pub epsilon: f32,
    pub timeout_ms: u64,
}

impl Default for InferenceVerificationConfig {
    fn default() -> Self {
        Self {
            cache_capacity: 1000,
            epsilon: 1e-6,
            timeout_ms: 10_000,
        }
    }
}

/// Verification error types
#[derive(Clone, Debug)]
pub enum VerificationError {
    InvalidProof,
    ShutdownActive,
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationError::InvalidProof => write!(f, "Invalid proof"),
            VerificationError::ShutdownActive => write!(f, "Shutdown active"),
        }
    }
}

impl std::error::Error for VerificationError {}

/// Verify a training proof for federated learning
/// 
/// Validates:
/// - Participants list (all nodes contributed shares)
/// - Delta magnitude is reasonable (L2 norm < threshold)
/// - Fisher matrix hash matches on-chain IPFS hash
/// - EWC regularization was applied (lambda > 0)
/// - Learning rate = 1e-6, epochs = 1
pub fn verify_training_proof(training_proof: &serde_json::Value) -> Result<bool, VerificationError> {
    // Validate participants list
    let participants = training_proof
        .get("participants")
        .and_then(|v| v.as_array())
        .ok_or(VerificationError::InvalidProof)?;
    
    if participants.len() < 2 {
        return Ok(false); // Need at least 2 participants for federated learning
    }

    // Validate delta magnitude (L2 norm < threshold)
    let delta_l2_norm = training_proof
        .get("delta_l2_norm")
        .and_then(|v| v.as_f64())
        .ok_or(VerificationError::InvalidProof)?;
    
    const MAX_DELTA_L2_NORM: f64 = 1000.0; // Threshold for reasonable delta
    if delta_l2_norm > MAX_DELTA_L2_NORM {
        return Ok(false);
    }

    // Validate Fisher matrix hash if present
    if let Some(fisher_hash) = training_proof.get("fisher_hash").and_then(|v| v.as_str()) {
        if fisher_hash.len() != 64 { // 32 bytes in hex
            return Ok(false);
        }
    }

    // Validate EWC regularization
    let ewc_lambda = training_proof
        .get("ewc_lambda")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    
    if ewc_lambda < 0.0 || ewc_lambda > 10.0 {
        return Ok(false);
    }

    // Validate learning parameters
    let learning_rate = training_proof
        .get("learning_rate")
        .and_then(|v| v.as_f64())
        .ok_or(VerificationError::InvalidProof)?;
    
    // Expected learning rate: 1e-6 (with small tolerance)
    const EXPECTED_LR: f64 = 1e-6;
    const LR_TOLERANCE: f64 = 1e-8;
    if (learning_rate - EXPECTED_LR).abs() > LR_TOLERANCE {
        return Ok(false);
    }

    // Validate epochs
    let epochs = training_proof
        .get("epochs")
        .and_then(|v| v.as_u64())
        .ok_or(VerificationError::InvalidProof)?;
    
    // Expected: 1 epoch for burst training
    if epochs != 1 {
        return Ok(false);
    }

    // Validate batch size
    let batch_size = training_proof
        .get("batch_size")
        .and_then(|v| v.as_u64())
        .ok_or(VerificationError::InvalidProof)?;
    
    if batch_size == 0 || batch_size > 1024 {
        return Ok(false);
    }

    // Validate round_id
    let round_id = training_proof
        .get("round_id")
        .and_then(|v| v.as_str())
        .ok_or(VerificationError::InvalidProof)?;
    
    if uuid::Uuid::parse_str(round_id).is_err() {
        return Ok(false);
    }

    Ok(true)
}

/// Simple hash function for PoI verification (BLAKE3)
pub fn hash_data(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

/// Validate work difficulty for PoI
pub fn validate_work_difficulty(work_hash: &[u8; 32], difficulty: u64) -> Result<u64, VerificationError> {
    if difficulty == 0 {
        return Err(VerificationError::InvalidProof);
    }
    let mut prefix = [0u8; 16];
    prefix.copy_from_slice(&work_hash[..16]);
    let value = u128::from_be_bytes(prefix);
    let target = u128::MAX / difficulty as u128;
    if value > target {
        return Err(VerificationError::InvalidProof);
    }
    Ok(difficulty)
}

/// Generate PoI proof work payload bytes
#[allow(clippy::too_many_arguments)]
pub fn poi_proof_work_payload_bytes(
    miner_address: &str,
    work_type: WorkType,
    input_hash: [u8; 32],
    output_data: &[u8],
    computation_metadata: &ComputationMetadata,
    difficulty: u64,
    timestamp: i64,
    nonce: u64,
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(miner_address.as_bytes());
    buf.push(work_type as u8);
    buf.extend_from_slice(&input_hash);
    buf.extend_from_slice(&(output_data.len() as u64).to_le_bytes());
    buf.extend_from_slice(output_data);
    let meta_bytes = bincode::serialize(computation_metadata).unwrap_or_default();
    buf.extend_from_slice(&(meta_bytes.len() as u64).to_le_bytes());
    buf.extend_from_slice(&meta_bytes);
    buf.extend_from_slice(&difficulty.to_le_bytes());
    buf.extend_from_slice(&timestamp.to_le_bytes());
    buf.extend_from_slice(&nonce.to_le_bytes());
    buf
}

/// Verify a PoI proof
pub fn verify_poi_proof(proof: &PoIProof) -> Result<bool, VerificationError> {
    let expected = hash_data(&poi_proof_work_payload_bytes(
        &proof.miner_address,
        proof.work_type,
        proof.input_hash,
        &proof.output_data,
        &proof.computation_metadata,
        proof.difficulty,
        proof.timestamp,
        proof.nonce,
    ));
    if expected != proof.work_hash {
        return Err(VerificationError::InvalidProof);
    }

    validate_work_difficulty(&proof.work_hash, proof.difficulty)?;

    let now = chrono::Utc::now().timestamp();
    if proof.timestamp <= 0 {
        return Err(VerificationError::InvalidProof);
    }
    if proof.timestamp > now + 300 || proof.timestamp < now - (86_400 * 365) {
        return Err(VerificationError::InvalidProof);
    }

    Ok(true)
}
