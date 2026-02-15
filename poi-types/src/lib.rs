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
