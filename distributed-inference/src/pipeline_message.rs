// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tensor_transport::CompressedTensor;

/// Message to initiate inference on a pipeline
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InferenceStart {
    pub inference_id: Uuid,
    pub model_id: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub pipeline_route: Vec<u32>, // block_ids in execution order
}

impl InferenceStart {
    pub fn new(
        model_id: String,
        prompt: String,
        max_tokens: u32,
        pipeline_route: Vec<u32>,
    ) -> Self {
        Self {
            inference_id: Uuid::new_v4(),
            model_id,
            prompt,
            max_tokens,
            pipeline_route,
        }
    }

    pub fn first_block(&self) -> Option<u32> {
        self.pipeline_route.first().copied()
    }

    pub fn last_block(&self) -> Option<u32> {
        self.pipeline_route.last().copied()
    }
}

/// Activation data chunk passed between blocks
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActivationChunk {
    pub inference_id: Uuid,
    pub block_id: u32,
    pub next_block_id: u32,
    pub tensor: CompressedTensor,
    pub token_index: u32,
}

impl ActivationChunk {
    pub fn is_final_block(&self) -> bool {
        self.block_id == self.next_block_id
    }

    pub fn total_size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<CompressedTensor>()
            + self.tensor.data.len()
    }
}

/// Checkpoint for recovery
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Checkpoint {
    pub inference_id: Uuid,
    pub block_id: u32,
    pub checkpoint_hash: [u8; 32],
    pub tensor: CompressedTensor,
    pub timestamp: i64,
}

impl Checkpoint {
    pub fn new(inference_id: Uuid, block_id: u32, tensor: CompressedTensor) -> Self {
        let timestamp = chrono::Utc::now().timestamp();

        // Compute checkpoint hash
        let mut hasher = sha3::Sha3_256::new();
        hasher.update(inference_id.as_bytes());
        hasher.update(&block_id.to_le_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(&tensor.checksum);
        let checkpoint_hash: [u8; 32] = hasher.finalize().into();

        Self {
            inference_id,
            block_id,
            checkpoint_hash,
            tensor,
            timestamp,
        }
    }

    pub fn verify_integrity(&self) -> bool {
        let mut hasher = sha3::Sha3_256::new();
        hasher.update(self.inference_id.as_bytes());
        hasher.update(&self.block_id.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&self.tensor.checksum);
        let computed_hash: [u8; 32] = hasher.finalize().into();

        computed_hash == self.checkpoint_hash
    }
}

/// PoI proof for block execution
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proof {
    pub inference_id: Uuid,
    pub block_id: u32,
    pub output_hash: [u8; 32],
    pub nonce: u64,
    pub signature: Vec<u8>,
}

impl Proof {
    pub fn new(inference_id: Uuid, block_id: u32, output_hash: [u8; 32], nonce: u64) -> Self {
        Self {
            inference_id,
            block_id,
            output_hash,
            nonce,
            signature: Vec::new(),
        }
    }

    pub fn with_signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = signature;
        self
    }

    pub fn compute_output_hash(data: &[u8]) -> [u8; 32] {
        let mut hasher = sha3::Sha3_256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
}

/// Failover instruction to replica
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Failover {
    pub inference_id: Uuid,
    pub failed_block_id: u32,
    pub failed_node_id: String,
    pub replacement_node_id: String,
    pub resume_checkpoint: Checkpoint,
}

impl Failover {
    pub fn new(
        inference_id: Uuid,
        failed_block_id: u32,
        failed_node_id: String,
        replacement_node_id: String,
        resume_checkpoint: Checkpoint,
    ) -> Self {
        Self {
            inference_id,
            failed_block_id,
            failed_node_id,
            replacement_node_id,
            resume_checkpoint,
        }
    }
}

/// Unified pipeline message enum
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PipelineMessage {
    InferenceStart(InferenceStart),
    ActivationChunk(ActivationChunk),
    Checkpoint(Checkpoint),
    Proof(Proof),
    Failover(Failover),
}

impl PipelineMessage {
    pub fn serialize(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }

    /// Priority for message ordering (higher = more urgent)
    pub fn priority(&self) -> u8 {
        match self {
            PipelineMessage::Failover(_) => 255,
            PipelineMessage::Checkpoint(_) => 200,
            PipelineMessage::Proof(_) => 150,
            PipelineMessage::ActivationChunk(_) => 100,
            PipelineMessage::InferenceStart(_) => 50,
        }
    }

    /// Get inference ID if applicable
    pub fn inference_id(&self) -> Option<Uuid> {
        match self {
            PipelineMessage::InferenceStart(msg) => Some(msg.inference_id),
            PipelineMessage::ActivationChunk(msg) => Some(msg.inference_id),
            PipelineMessage::Checkpoint(msg) => Some(msg.inference_id),
            PipelineMessage::Proof(msg) => Some(msg.inference_id),
            PipelineMessage::Failover(msg) => Some(msg.inference_id),
        }
    }

    /// Get block ID if applicable
    pub fn block_id(&self) -> Option<u32> {
        match self {
            PipelineMessage::InferenceStart(_) => None,
            PipelineMessage::ActivationChunk(msg) => Some(msg.block_id),
            PipelineMessage::Checkpoint(msg) => Some(msg.block_id),
            PipelineMessage::Proof(msg) => Some(msg.block_id),
            PipelineMessage::Failover(msg) => Some(msg.failed_block_id),
        }
    }

    /// Check if this is a high-priority message
    pub fn is_high_priority(&self) -> bool {
        self.priority() >= 200
    }

    /// Get message type name
    pub fn type_name(&self) -> &'static str {
        match self {
            PipelineMessage::InferenceStart(_) => "InferenceStart",
            PipelineMessage::ActivationChunk(_) => "ActivationChunk",
            PipelineMessage::Checkpoint(_) => "Checkpoint",
            PipelineMessage::Proof(_) => "Proof",
            PipelineMessage::Failover(_) => "Failover",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor_transport::{TensorMetadata, TensorTransport};

    #[test]
    fn test_inference_start_creation() {
        let msg = InferenceStart::new(
            "mistral-7b".to_string(),
            "Hello world".to_string(),
            100,
            vec![0, 1, 2, 3, 4, 5, 6, 7],
        );

        assert_eq!(msg.model_id, "mistral-7b");
        assert_eq!(msg.prompt, "Hello world");
        assert_eq!(msg.max_tokens, 100);
        assert_eq!(msg.pipeline_route, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(msg.first_block(), Some(0));
        assert_eq!(msg.last_block(), Some(7));
    }

    #[test]
    fn test_activation_chunk_properties() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![0; 100];
        let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

        let chunk = ActivationChunk {
            inference_id: Uuid::new_v4(),
            block_id: 0,
            next_block_id: 1,
            tensor: compressed,
            token_index: 0,
        };

        assert!(!chunk.is_final_block());
        assert!(chunk.total_size_bytes() > 0);
    }

    #[test]
    fn test_checkpoint_integrity() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![0; 100];
        let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

        let checkpoint = Checkpoint::new(Uuid::new_v4(), 0, compressed);

        assert!(checkpoint.verify_integrity());
    }

    #[test]
    fn test_proof_creation() {
        let output_hash = Proof::compute_output_hash(b"test data");
        let proof = Proof::new(Uuid::new_v4(), 0, output_hash, 12345)
            .with_signature(vec![1, 2, 3, 4, 5]);

        assert_eq!(proof.block_id, 0);
        assert_eq!(proof.nonce, 12345);
        assert_eq!(proof.signature, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_failover_creation() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![0; 100];
        let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();
        let checkpoint = Checkpoint::new(Uuid::new_v4(), 0, compressed);

        let inference_id = checkpoint.inference_id;
        let failover = Failover::new(
            inference_id,
            0,
            "failed-node".to_string(),
            "replacement-node".to_string(),
            checkpoint,
        );

        assert_eq!(failover.inference_id, inference_id);
        assert_eq!(failover.failed_block_id, 0);
        assert_eq!(failover.failed_node_id, "failed-node");
        assert_eq!(failover.replacement_node_id, "replacement-node");
    }

    #[test]
    fn test_pipeline_message_serialization() {
        let msg = PipelineMessage::InferenceStart(InferenceStart::new(
            "mistral-7b".to_string(),
            "Hello".to_string(),
            50,
            vec![0, 1, 2],
        ));

        let serialized = msg.serialize().unwrap();
        let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

        match deserialized {
            PipelineMessage::InferenceStart(start) => {
                assert_eq!(start.model_id, "mistral-7b");
                assert_eq!(start.prompt, "Hello");
                assert_eq!(start.max_tokens, 50);
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_message_priorities() {
        let inference_start =
            PipelineMessage::InferenceStart(InferenceStart::new("model".to_string(), "test".to_string(), 10, vec![]));
        assert_eq!(inference_start.priority(), 50);
        assert!(!inference_start.is_high_priority());

        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };
        let compressed = TensorTransport::compress(&vec![0; 100], metadata, 1).unwrap();

        let activation = PipelineMessage::ActivationChunk(ActivationChunk {
            inference_id: Uuid::new_v4(),
            block_id: 0,
            next_block_id: 1,
            tensor: compressed.clone(),
            token_index: 0,
        });
        assert_eq!(activation.priority(), 100);
        assert!(!activation.is_high_priority());

        let proof = PipelineMessage::Proof(Proof::new(
            Uuid::new_v4(),
            0,
            [0u8; 32],
            0,
        ));
        assert_eq!(proof.priority(), 150);
        assert!(!proof.is_high_priority());

        let checkpoint = PipelineMessage::Checkpoint(Checkpoint::new(
            Uuid::new_v4(),
            0,
            compressed.clone(),
        ));
        assert_eq!(checkpoint.priority(), 200);
        assert!(checkpoint.is_high_priority());

        let failover = PipelineMessage::Failover(Failover::new(
            Uuid::new_v4(),
            0,
            "failed".to_string(),
            "replacement".to_string(),
            Checkpoint::new(Uuid::new_v4(), 0, compressed),
        ));
        assert_eq!(failover.priority(), 255);
        assert!(failover.is_high_priority());
    }

    #[test]
    fn test_message_type_names() {
        let metadata = TensorMetadata {
            shape: vec![1],
            dtype: "f32".to_string(),
            layer_range: (0, 0),
            original_dtype: None,
        };
        let compressed = TensorTransport::compress(&vec![0; 10], metadata, 1).unwrap();

        assert_eq!(
            PipelineMessage::InferenceStart(InferenceStart::new("".to_string(), "".to_string(), 0, vec![])).type_name(),
            "InferenceStart"
        );
        assert_eq!(
            PipelineMessage::ActivationChunk(ActivationChunk {
                inference_id: Uuid::new_v4(),
                block_id: 0,
                next_block_id: 1,
                tensor: compressed.clone(),
                token_index: 0,
            }).type_name(),
            "ActivationChunk"
        );
        assert_eq!(
            PipelineMessage::Checkpoint(Checkpoint::new(Uuid::new_v4(), 0, compressed.clone())).type_name(),
            "Checkpoint"
        );
        assert_eq!(
            PipelineMessage::Proof(Proof::new(Uuid::new_v4(), 0, [0u8; 32], 0)).type_name(),
            "Proof"
        );
        assert_eq!(
            PipelineMessage::Failover(Failover::new(
                Uuid::new_v4(),
                0,
                "".to_string(),
                "".to_string(),
                Checkpoint::new(Uuid::new_v4(), 0, compressed),
            )).type_name(),
            "Failover"
        );
    }

    #[test]
    fn test_message_inference_ids() {
        let inference_id = Uuid::new_v4();

        let start = PipelineMessage::InferenceStart(InferenceStart {
            inference_id,
            model_id: "".to_string(),
            prompt: "".to_string(),
            max_tokens: 0,
            pipeline_route: vec![],
        });
        assert_eq!(start.inference_id(), Some(inference_id));

        let metadata = TensorMetadata {
            shape: vec![1],
            dtype: "f32".to_string(),
            layer_range: (0, 0),
            original_dtype: None,
        };
        let compressed = TensorTransport::compress(&vec![0; 10], metadata, 1).unwrap();

        let chunk = PipelineMessage::ActivationChunk(ActivationChunk {
            inference_id,
            block_id: 0,
            next_block_id: 1,
            tensor: compressed.clone(),
            token_index: 0,
        });
        assert_eq!(chunk.inference_id(), Some(inference_id));

        let checkpoint = PipelineMessage::Checkpoint(Checkpoint::new(inference_id, 0, compressed.clone()));
        assert_eq!(checkpoint.inference_id(), Some(inference_id));

        let proof = PipelineMessage::Proof(Proof::new(inference_id, 0, [0u8; 32], 0));
        assert_eq!(proof.inference_id(), Some(inference_id));

        let failover = PipelineMessage::Failover(Failover::new(
            inference_id,
            0,
            "".to_string(),
            "".to_string(),
            Checkpoint::new(inference_id, 0, compressed),
        ));
        assert_eq!(failover.inference_id(), Some(inference_id));
    }
}
