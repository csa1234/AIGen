// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Distributed Inference Pipeline System
//!
//! This crate implements layer-block pipeline parallelism for distributed AI inference.
//! Models are split into sequential blocks (e.g., 80 layers → 8 blocks of 10 layers),
//! with each block assigned to specific nodes with K=3 replicas for fault tolerance.
//!
//! # Architecture
//!
//! - **BlockAssignment**: Manages layer block definitions and replica tracking
//! - **TensorTransport**: Handles serialization/compression with bincode + zstd
//! - **PipelineMessage**: Protocol definitions for inter-block communication
//!
//! # Communication Flow
//!
//! 1. Coordinator sends `InferenceStart` to block0 nodes via `/aigen/block/0/1.0.0`
//! 2. Block0 processes layers 0-9, sends `ActivationChunk` to block1 via `/aigen/block/1/1.0.0`
//! 3. Each block processes and forwards until final block returns output
//! 4. Checkpoints saved every N tokens for recovery
//! 5. On failure, `Failover` message triggers replica takeover
//!
//! # Usage Example
//!
//! ```rust,ignore
//! use distributed_inference::{BlockAssignment, TensorTransport, PipelineMessage};
//!
//! // Initialize block assignment
//! let mut assignment = BlockAssignment::new(3); // K=3 replicas
//! assignment.add_block(LayerBlock {
//!     block_id: 0,
//!     layer_range: (0, 9),
//!     model_id: "mistral-7b".to_string(),
//!     estimated_vram_gb: 2.5,
//! });
//!
//! // Compress activation tensor
//! let tensor = TensorTransport::compress(
//!     &activation_data,
//!     TensorMetadata { shape: vec![1, 512, 4096], dtype: "f32".to_string(), layer_range: (0, 9) },
//!     1, // compression level
//! )?;
//!
//! // Create activation chunk message
//! let msg = PipelineMessage::ActivationChunk(ActivationChunk {
//!     inference_id: Uuid::new_v4(),
//!     block_id: 0,
//!     next_block_id: 1,
//!     tensor,
//!     token_index: 0,
//! });
//!
//! // Serialize and send via libp2p
//! let bytes = msg.serialize()?;
//! gossipsub.publish(topic_block(1), bytes)?;
//! ```

pub mod block_assignment;
pub mod coordinator;
pub mod tensor_transport;
pub mod pipeline_message;
pub mod orchestrator;
pub mod replica_manager;
pub mod health_monitor;
pub mod route_selector;
pub mod checkpoint_manager;
pub mod failover_coordinator;
pub mod topology_optimizer;

pub use block_assignment::{BlockAssignment, BlockReplica, LayerBlock, StaticBlockConfig, BlockConfig, ReplicaConfig};
pub use coordinator::{ActiveInference, StaticPipelineCoordinator};
pub use tensor_transport::{CompressedTensor, TensorMetadata, TensorTransport, TransportError};
pub use pipeline_message::{
    ActivationChunk, Checkpoint, Failover, InferenceStart, PipelineMessage, Proof,
};
pub use orchestrator::{OrchestratorNode, OrchestratorConfig, OrchestratorState, ReplicaPlan, OrchestratorError, PipelineAuthToken, verify_pipeline_auth_token};
pub use replica_manager::{ReplicaManager, BlockReplica as BlockReplicaInfo, ReplicaHealth, HealthStatus};
pub use health_monitor::{HealthMonitor, HealthStats, LoadProvider, SimpleLoadProvider};
pub use route_selector::{RouteSelector, RouteStats, DcsIntegratedRouteSelector, RouteSelectionError};
pub use checkpoint_manager::{CheckpointManager, CheckpointStorageBackend, CheckpointKey, CheckpointConfig, CheckpointStats, CheckpointManagerError};
pub use failover_coordinator::{FailoverCoordinator, FailoverMetrics, FailoverStats, FailoverConfig};
pub use topology_optimizer::{TopologyOptimizer, TopologyMetrics, TopologyOptimizedRouteSelector};
