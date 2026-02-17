// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::shutdown_propagation::ShutdownMessage;
use blockchain_core::{Block, Transaction};
use consensus::{PoIProof, ValidatorVote};
use serde::{Deserialize, Serialize};

use crate::shutdown_propagation::NetworkError;

use crate::events::TensorChunk;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub has_gpu: bool,
    pub gpu_model: Option<String>,
    pub supports_inference: bool,
    pub supports_training: bool,
    pub max_fragment_size_mb: u32,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            has_gpu: false,
            gpu_model: None,
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 1024,
        }
    }
}

/// Reference to an activation tensor stored on a node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationRef {
    pub location: String, // Format: "node_id/task_id"
    pub size_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    ShutdownSignal(ShutdownMessage),
    Block(Block),
    Transaction(Transaction),
    PoIProof(PoIProof),
    ValidatorVote(ValidatorVote),
    ModelShardRequest {
        model_id: String,
        shard_index: u32,
    },
    ModelShardResponse {
        model_id: String,
        shard_index: u32,
        data: Vec<u8>,
        hash: [u8; 32],
    },
    ModelFragmentRequest {
        model_id: String,
        fragment_index: u32,
    },
    ModelFragmentResponse {
        model_id: String,
        fragment_index: u32,
        data: Vec<u8>,
        hash: [u8; 32],
        is_compressed: bool,
    },
    ModelAnnouncement {
        model_id: String,
        shard_index: u32,
        node_id: String,
    },
    ModelFragmentAnnouncement {
        model_id: String,
        fragment_index: u32,
        node_id: String,
    },
    ModelQuery {
        model_id: String,
    },
    TensorRequest {
        poi_proof_hash: [u8; 32],
        chunk_index: u32,
    },
    TensorResponse(TensorChunk),
    VramCapabilityAnnouncement {
        node_id: String,
        vram_total_gb: f32,
        vram_free_gb: f32,
        vram_allocated_gb: f32,
        cpu_cores: u32,
        region: Option<String>,
        capabilities: NodeCapabilities,
        timestamp: i64,
    },
    Ping,
    Pong,
    ComputeTask {
        task_id: Uuid,
        inference_id: Uuid,
        model_id: String,
        layer_range: (u32, u32),
        required_fragments: Vec<String>,
        assigned_node: String,
        input_activation_ref: Option<ActivationRef>,
        // Tensor parallelism fields
        tensor_shard_index: u32,
        total_tensor_shards: u32,
    },
    TaskResult {
        task_id: Uuid,
        output_activation: Vec<u8>,
        poi_proof: consensus::PoIProof,
        compute_time_ms: u64,
        fragment_ids: Vec<String>,
    },
    TaskFailure {
        task_id: Uuid,
        error: String,
    },
    ActivationChunk {
        task_id: Uuid,
        inference_id: Uuid,
        chunk_index: u32,
        total_chunks: u32,
        data: Vec<u8>, // compressed activation data
        compression_level: i32,
        checkpoint_hash: [u8; 32],
        shape: Vec<i64>, // Tensor shape for reconstruction
        layer_range: (u32, u32), // Layer range for reconstruction
        // NEW: Quantization metadata for 8-bit compression
        quantization_min: f32,
        quantization_max: f32,
        is_quantized: bool,
    },
    /// Heartbeat message sent every 500ms by active nodes
    Heartbeat {
        node_id: String,
        timestamp: i64,
        active_tasks: Vec<Uuid>, // Currently executing task IDs
        load_score: f32,
    },
    /// Replica join request with authentication token
    ReplicaJoinRequest {
        node_id: String,
        block_id: u32,
        peer_id: Vec<u8>, // Serialized PeerId
        capabilities: NodeCapabilities,
        auth_token: Vec<u8>, // Serialized PipelineAuthToken
    },
    /// Checkpoint message for intermediate state persistence
    Checkpoint {
        task_id: Uuid,
        inference_id: Uuid,
        checkpoint_hash: [u8; 32],
        layer_range: (u32, u32),
        timestamp: i64,
    },
    /// Reassign fragment/task to different replica after failure
    ReassignFragment {
        task_id: Uuid,
        inference_id: Uuid,
        old_node: String,
        new_node: String,
        checkpoint_ref: Option<CheckpointRef>,
    },
    /// Bid announcement for market-based task assignment
    BidAnnouncement {
        node_id: String,
        bid_price_per_task: u64,
        valid_until: i64,
    },
    /// Failover notification to resume from checkpoint
    Failover {
        task_id: Uuid,
        inference_id: Uuid,
        failed_node: String,
        replacement_node: String,
        resume_from_checkpoint: [u8; 32],
    },
    /// Pipeline inference messages for block-based distributed execution
    PipelineInference(Vec<u8>), // Serialized PipelineMessage from distributed-inference crate
    /// Federated learning: Training burst broadcast when buffer >= 900
    TrainingBurst {
        model_id: String,
        buffer_size: usize,
        trigger_timestamp: i64,
    },
    /// Federated learning: Encrypted delta share sent peer-to-peer
    DeltaShare {
        node_id: String,
        share_index: u32,
        encrypted_share: Vec<u8>,
        recipient_node: String,
        round_id: Uuid,
    },
    /// Federated learning: Aggregated share sent to aggregator node
    AggregatedShare {
        node_id: String,
        aggregated_share: Vec<u8>,
        round_id: Uuid,
    },
    /// Federated learning: Final averaged delta broadcast by aggregator
    GlobalDelta {
        round_id: Uuid,
        delta_avg: Vec<u8>,
        signature: Vec<u8>,
    },
    /// Federated learning: PoI proof for training round
    TrainingProof {
        round_id: Uuid,
        poi_proof: consensus::PoIProof,
        participants: Vec<String>,
    },
    /// Canary deployment rollback notification
    CanaryRollback {
        model_id: String,
        canary_version: String,
        reason: String,
        traffic_at_rollback: f32,
        timestamp: i64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRef {
    pub checkpoint_hash: [u8; 32],
    pub layer_range: (u32, u32),
    pub location: String, // Node ID where checkpoint is stored
}

impl NetworkMessage {
    pub fn priority(&self) -> u8 {
        match self {
            NetworkMessage::ShutdownSignal(_) => 255,
            NetworkMessage::ValidatorVote(_) => 200,
            NetworkMessage::Checkpoint { .. } => 200,
            NetworkMessage::Failover { .. } => 190,
            NetworkMessage::Heartbeat { .. } => 180,
            NetworkMessage::ReplicaJoinRequest { .. } => 175, // High priority for join requests
            NetworkMessage::Block(_) => 100,
            NetworkMessage::VramCapabilityAnnouncement { .. } => 90,
            NetworkMessage::ModelAnnouncement { .. } => 80,
            NetworkMessage::ModelFragmentAnnouncement { .. } => 80,
            NetworkMessage::ModelQuery { .. } => 80,
            NetworkMessage::ModelShardRequest { .. } => 85,
            NetworkMessage::ModelShardResponse { .. } => 85,
            NetworkMessage::ModelFragmentRequest { .. } => 85,
            NetworkMessage::ModelFragmentResponse { .. } => 85,
            NetworkMessage::CanaryRollback { .. } => 200, // High priority for rollback notifications
            NetworkMessage::PoIProof(_) => 75,
            NetworkMessage::Transaction(_) => 50,
            NetworkMessage::TensorRequest { .. } => 150,
            NetworkMessage::TensorResponse(_) => 150,
            NetworkMessage::ComputeTask { .. } => 150,
            NetworkMessage::TaskResult { .. } => 150,
            NetworkMessage::TaskFailure { .. } => 150,
            NetworkMessage::ActivationChunk { .. } => 140,
            NetworkMessage::ReassignFragment { .. } => 160,
            NetworkMessage::BidAnnouncement { .. } => 85, // Medium priority for bids
            NetworkMessage::Ping | NetworkMessage::Pong => 10,
            NetworkMessage::PipelineInference(_) => 150,
            // Federated learning messages (150-180 range)
            NetworkMessage::TrainingBurst { .. } => 170,
            NetworkMessage::DeltaShare { .. } => 165,
            NetworkMessage::AggregatedShare { .. } => 160,
            NetworkMessage::GlobalDelta { .. } => 175,
            NetworkMessage::TrainingProof { .. } => 155,
        }
    }

    pub fn serialize(&self) -> Result<Vec<u8>, NetworkError> {
        serde_json::to_vec(self).map_err(|e| NetworkError::Serialization(e.to_string()))
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, NetworkError> {
        serde_json::from_slice(bytes).map_err(|e| NetworkError::Serialization(e.to_string()))
    }

    /// Create a PipelineInference message from distributed-inference PipelineMessage
    pub fn from_pipeline_message(msg: &[u8]) -> Self {
        NetworkMessage::PipelineInference(msg.to_vec())
    }

    /// Extract pipeline message bytes
    pub fn as_pipeline_message(&self) -> Option<&[u8]> {
        match self {
            NetworkMessage::PipelineInference(bytes) => Some(bytes),
            _ => None,
        }
    }
}
