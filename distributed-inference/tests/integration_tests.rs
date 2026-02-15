// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use distributed_inference::{
    ActivationChunk, BlockAssignment, CompressedTensor,
    Failover, InferenceStart, LayerBlock, PipelineMessage, Proof,
    StaticBlockConfig, TensorMetadata, TensorTransport,
};
use distributed_inference::block_assignment::{BlockConfig, ReplicaConfig};
use network::protocol::NodeCapabilities;
use uuid::Uuid;

/// Helper function to create test node capabilities
fn test_capabilities() -> NodeCapabilities {
    NodeCapabilities {
        has_gpu: true,
        gpu_model: Some("RTX4090".to_string()),
        supports_inference: true,
        supports_training: false,
        max_fragment_size_mb: 512,
    }
}

/// Test block assignment creation and basic operations
#[test]
fn test_block_assignment_creation() {
    let mut assignment = BlockAssignment::new(3);

    assignment.add_block(LayerBlock {
        block_id: 0,
        layer_range: (0, 9),
        model_id: "mistral-7b".to_string(),
        estimated_vram_gb: 2.5,
    });

    assignment.add_block(LayerBlock {
        block_id: 1,
        layer_range: (10, 19),
        model_id: "mistral-7b".to_string(),
        estimated_vram_gb: 2.5,
    });

    assert_eq!(assignment.blocks.len(), 2);
    assert_eq!(assignment.get_block_ids(), vec![0, 1]);

    let block = assignment.get_block(0).unwrap();
    assert_eq!(block.layer_range, (0, 9));
    assert_eq!(block.estimated_vram_gb, 2.5);
}

/// Test replica assignment and health tracking
#[test]
fn test_replica_assignment() {
    let mut assignment = BlockAssignment::new(3);

    assignment.add_block(LayerBlock {
        block_id: 0,
        layer_range: (0, 9),
        model_id: "mistral-7b".to_string(),
        estimated_vram_gb: 2.5,
    });

    assignment.assign_replica(0, "node-a".to_string(), libp2p::PeerId::random(), test_capabilities());
    assignment.assign_replica(0, "node-b".to_string(), libp2p::PeerId::random(), test_capabilities());
    assignment.assign_replica(0, "node-c".to_string(), libp2p::PeerId::random(), test_capabilities());

    let replicas = assignment.get_replicas(0);
    assert_eq!(replicas.len(), 3);
    assert!(replicas.iter().all(|r| r.is_healthy));

    // Mark one replica as unhealthy
    assignment.mark_replica_unhealthy(0, "node-b");

    let healthy = assignment.get_healthy_replicas(0);
    assert_eq!(healthy.len(), 2);
    assert!(!healthy.iter().any(|r| r.node_id == "node-b"));

    // Mark it healthy again
    assignment.mark_replica_healthy(0, "node-b");
    let healthy = assignment.get_healthy_replicas(0);
    assert_eq!(healthy.len(), 3);
}

/// Test best replica selection based on load scores
#[test]
fn test_replica_selection() {
    let mut assignment = BlockAssignment::new(3);

    assignment.add_block(LayerBlock {
        block_id: 0,
        layer_range: (0, 9),
        model_id: "mistral-7b".to_string(),
        estimated_vram_gb: 2.5,
    });

    assignment.assign_replica(0, "node-a".to_string(), libp2p::PeerId::random(), test_capabilities());
    assignment.assign_replica(0, "node-b".to_string(), libp2p::PeerId::random(), test_capabilities());
    assignment.assign_replica(0, "node-c".to_string(), libp2p::PeerId::random(), test_capabilities());

    // Set different load scores
    assignment.update_replica_load(0, "node-a", 0.8);
    assignment.update_replica_load(0, "node-b", 0.3);
    assignment.update_replica_load(0, "node-c", 0.5);

    let best = assignment.select_best_replica(0);
    assert_eq!(best, Some("node-b".to_string()));

    // Test when all replicas have similar load
    assignment.update_replica_load(0, "node-a", 0.5);
    assignment.update_replica_load(0, "node-b", 0.5);
    assignment.update_replica_load(0, "node-c", 0.5);

    let best = assignment.select_best_replica(0);
    assert!(best.is_some());
    assert!(vec!["node-a", "node-b", "node-c"].contains(&best.as_ref().unwrap().as_str()));
}

/// Test minimum replica requirements
#[test]
fn test_minimum_replicas() {
    let mut assignment = BlockAssignment::new(2);

    assignment.add_block(LayerBlock {
        block_id: 0,
        layer_range: (0, 9),
        model_id: "mistral-7b".to_string(),
        estimated_vram_gb: 2.5,
    });

    // Initially no replicas
    assert!(!assignment.has_minimum_replicas(0));

    // Add one replica (still not enough)
    assignment.assign_replica(0, "node-a".to_string(), libp2p::PeerId::random(), test_capabilities());
    assert!(!assignment.has_minimum_replicas(0));

    // Add second replica (now enough)
    assignment.assign_replica(0, "node-b".to_string(), libp2p::PeerId::random(), test_capabilities());
    assert!(assignment.has_minimum_replicas(0));

    // Mark one as unhealthy
    assignment.mark_replica_unhealthy(0, "node-a");
    assert!(!assignment.has_minimum_replicas(0));
}

/// Test total healthy replicas count
#[test]
fn test_total_healthy_replicas() {
    let mut assignment = BlockAssignment::new(2);

    // Add two blocks
    for i in 0..2 {
        assignment.add_block(LayerBlock {
            block_id: i,
            layer_range: (i * 10, i * 10 + 9),
            model_id: "mistral-7b".to_string(),
            estimated_vram_gb: 2.5,
        });

        assignment.assign_replica(i, format!("node-{}-a", i), libp2p::PeerId::random(), test_capabilities());
        assignment.assign_replica(i, format!("node-{}-b", i), libp2p::PeerId::random(), test_capabilities());
    }

    assert_eq!(assignment.total_healthy_replicas(), 4);

    // Mark some as unhealthy
    assignment.mark_replica_unhealthy(0, "node-0-a");
    assignment.mark_replica_unhealthy(1, "node-1-b");

    assert_eq!(assignment.total_healthy_replicas(), 2);
}

/// Test tensor compression roundtrip with checksum validation
#[test]
fn test_tensor_compression_roundtrip() {
    // Create sample tensor data (simulating 1x512x4096 f32 tensor)
    let num_elements = 1 * 512 * 4096;
    let data: Vec<u8> = (0..num_elements * 4)
        .map(|i| (i % 256) as u8)
        .collect();

    let metadata = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };

    // Compress at level 1 (low latency)
    let compressed = TensorTransport::compress(&data, metadata.clone(), 1)
        .expect("compression should succeed");

    // Verify compressed data is smaller than original
    assert!(
        compressed.data.len() < data.len() || compressed.data.len() == data.len(),
        "compressed size {} should be <= original {}",
        compressed.data.len(),
        data.len()
    );

    // Decompress
    let (decompressed, recovered_metadata) =
        TensorTransport::decompress(&compressed).expect("decompression should succeed");

    // Verify data integrity
    assert_eq!(decompressed, data, "decompressed data should match original");
    assert_eq!(recovered_metadata.shape, metadata.shape);
    assert_eq!(recovered_metadata.dtype, metadata.dtype);
    assert_eq!(recovered_metadata.layer_range, metadata.layer_range);
}

/// Test tensor compression at different levels
#[test]
fn test_compression_levels() {
    let num_elements = 1024 * 1024; // 1M elements for compressible data
    let data: Vec<u8> = (0..num_elements * 4)
        .map(|i| ((i / 4) % 256) as u8)
        .collect();

    let metadata = TensorMetadata {
        shape: vec![num_elements],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };

    for level in 1..=3 {
        let compressed = TensorTransport::compress(&data, metadata.clone(), level)
            .expect(&format!("compression at level {} should succeed", level));

        assert_eq!(compressed.compression_level, level);

        // Verify roundtrip
        let (decompressed, _) = TensorTransport::decompress(&compressed)
            .expect(&format!("decompression at level {} should succeed", level));

        assert_eq!(decompressed, data);
    }
}

/// Test serialization and deserialization of compressed tensors
#[test]
fn test_tensor_serialization() {
    let metadata = TensorMetadata {
        shape: vec![10, 20, 30],
        dtype: "f16".to_string(),
        layer_range: (5, 14),
        original_dtype: None,
    };

    let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

    // Serialize
    let serialized = TensorTransport::serialize(&compressed).unwrap();

    // Deserialize
    let deserialized = TensorTransport::deserialize(&serialized).unwrap();

    // Verify
    let (decompressed, _) = TensorTransport::decompress(&deserialized).unwrap();
    assert_eq!(decompressed, data);
}

/// Test pipeline message serialization for all message types
#[test]
fn test_pipeline_message_serialization() {
    let inference_id = Uuid::new_v4();

    // Test InferenceStart
    let start = InferenceStart::new(
        "mistral-7b".to_string(),
        "Hello world".to_string(),
        100,
        vec![0, 1, 2, 3, 4, 5, 6, 7],
    );
    let msg = PipelineMessage::InferenceStart(start.clone());
    let serialized = msg.serialize().unwrap();
    let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

    match deserialized {
        PipelineMessage::InferenceStart(s) => {
            assert_eq!(s.model_id, start.model_id);
            assert_eq!(s.prompt, start.prompt);
            assert_eq!(s.max_tokens, start.max_tokens);
            assert_eq!(s.pipeline_route, start.pipeline_route);
        }
        _ => panic!("wrong message type"),
    }

    // Test ActivationChunk
    let metadata = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    let tensor_data: Vec<u8> = vec![0; 100];
    let compressed = TensorTransport::compress(&tensor_data, metadata, 1).unwrap();

    let chunk = ActivationChunk {
        inference_id,
        block_id: 0,
        next_block_id: 1,
        tensor: compressed.clone(),
        token_index: 0,
    };
    let msg = PipelineMessage::ActivationChunk(chunk);
    let serialized = msg.serialize().unwrap();
    let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

    match deserialized {
        PipelineMessage::ActivationChunk(c) => {
            assert_eq!(c.block_id, 0);
            assert_eq!(c.next_block_id, 1);
            assert_eq!(c.token_index, 0);
        }
        _ => panic!("wrong message type"),
    }

    // Test Checkpoint
    let checkpoint = distributed_inference::Checkpoint::new(inference_id, 0, compressed.clone());
    let msg = PipelineMessage::Checkpoint(checkpoint);
    let serialized = msg.serialize().unwrap();
    let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

    match deserialized {
        PipelineMessage::Checkpoint(c) => {
            assert_eq!(c.inference_id, inference_id);
            assert_eq!(c.block_id, 0);
            assert!(c.verify_integrity());
        }
        _ => panic!("wrong message type"),
    }

    // Test Proof
    let output_hash = Proof::compute_output_hash(b"test output");
    let proof = Proof::new(inference_id, 0, output_hash, 12345)
        .with_signature(vec![1, 2, 3, 4, 5]);
    let msg = PipelineMessage::Proof(proof);
    let serialized = msg.serialize().unwrap();
    let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

    match deserialized {
        PipelineMessage::Proof(p) => {
            assert_eq!(p.inference_id, inference_id);
            assert_eq!(p.block_id, 0);
            assert_eq!(p.nonce, 12345);
            assert_eq!(p.signature, vec![1, 2, 3, 4, 5]);
        }
        _ => panic!("wrong message type"),
    }

    // Test Failover
    let checkpoint = distributed_inference::Checkpoint::new(inference_id, 0, compressed);
    let failover = Failover::new(
        inference_id,
        0,
        "failed-node".to_string(),
        "replacement-node".to_string(),
        checkpoint,
    );
    let msg = PipelineMessage::Failover(failover);
    let serialized = msg.serialize().unwrap();
    let deserialized = PipelineMessage::deserialize(&serialized).unwrap();

    match deserialized {
        PipelineMessage::Failover(f) => {
            assert_eq!(f.inference_id, inference_id);
            assert_eq!(f.failed_block_id, 0);
            assert_eq!(f.failed_node_id, "failed-node");
            assert_eq!(f.replacement_node_id, "replacement-node");
        }
        _ => panic!("wrong message type"),
    }
}

/// Test message priorities
#[test]
fn test_message_priorities() {
    let inference_id = Uuid::new_v4();

    let start = InferenceStart::new("model".to_string(), "test".to_string(), 10, vec![]);
    assert_eq!(PipelineMessage::InferenceStart(start).priority(), 50);

    let metadata = TensorMetadata {
        shape: vec![1],
        dtype: "f32".to_string(),
        layer_range: (0, 0),
        original_dtype: None,
    };
    let compressed = TensorTransport::compress(&vec![0; 100], metadata.clone(), 1).unwrap();

    let activation = ActivationChunk {
        inference_id,
        block_id: 0,
        next_block_id: 1,
        tensor: compressed.clone(),
        token_index: 0,
    };
    assert_eq!(PipelineMessage::ActivationChunk(activation).priority(), 100);

    let proof = Proof::new(inference_id, 0, [0u8; 32], 0);
    assert_eq!(PipelineMessage::Proof(proof).priority(), 150);

    let checkpoint = distributed_inference::Checkpoint::new(inference_id, 0, compressed.clone());
    assert_eq!(PipelineMessage::Checkpoint(checkpoint).priority(), 200);

    let failover = Failover::new(
        inference_id,
        0,
        "failed".to_string(),
        "replacement".to_string(),
        distributed_inference::Checkpoint::new(inference_id, 0, compressed),
    );
    assert_eq!(PipelineMessage::Failover(failover).priority(), 255);
}

/// Test static config loading and conversion
#[test]
fn test_static_config_conversion() {
    let static_config = StaticBlockConfig {
        total_blocks: 3,
        replication_factor: 2,
        blocks: vec![
            BlockConfig {
                block_id: 0,
                layer_range: (0, 9),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig {
                        node_id: "node-a".to_string(),
                        priority: 1,
                    },
                    ReplicaConfig {
                        node_id: "node-b".to_string(),
                        priority: 2,
                    },
                ],
            },
            BlockConfig {
                block_id: 1,
                layer_range: (10, 19),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig {
                        node_id: "node-c".to_string(),
                        priority: 1,
                    },
                    ReplicaConfig {
                        node_id: "node-d".to_string(),
                        priority: 2,
                    },
                ],
            },
            BlockConfig {
                block_id: 2,
                layer_range: (20, 29),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig {
                        node_id: "node-e".to_string(),
                        priority: 1,
                    },
                    ReplicaConfig {
                        node_id: "node-f".to_string(),
                        priority: 2,
                    },
                ],
            },
        ],
    };

    let assignment = static_config.to_block_assignment();

    assert_eq!(assignment.blocks.len(), 3);
    assert_eq!(assignment.get_block_ids(), vec![0, 1, 2]);
    assert_eq!(assignment.get_replicas(0).len(), 2);
    assert_eq!(assignment.get_replicas(1).len(), 2);
    assert_eq!(assignment.get_replicas(2).len(), 2);
    assert!(assignment.has_minimum_replicas(0));
    assert!(assignment.has_minimum_replicas(1));
    assert!(assignment.has_minimum_replicas(2));
}

/// Test checkpoint integrity verification
#[test]
fn test_checkpoint_integrity() {
    let inference_id = Uuid::new_v4();

    let metadata = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };

    let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

    let checkpoint = distributed_inference::Checkpoint::new(inference_id, 0, compressed);

    // Should verify successfully
    assert!(checkpoint.verify_integrity());

    // Create a corrupted checkpoint
    let mut corrupted = checkpoint.clone();
    corrupted.checkpoint_hash[0] ^= 0xFF;

    // Should fail verification
    assert!(!corrupted.verify_integrity());
}

/// Test proof output hash computation
#[test]
fn test_proof_output_hash() {
    let data1 = b"test data 1";
    let data2 = b"test data 2";

    let hash1 = Proof::compute_output_hash(data1);
    let hash2 = Proof::compute_output_hash(data2);
    let hash1_again = Proof::compute_output_hash(data1);

    // Same data should produce same hash
    assert_eq!(hash1, hash1_again);

    // Different data should produce different hashes
    assert_ne!(hash1, hash2);

    // Hash should be 32 bytes (SHA3-256)
    assert_eq!(hash1.len(), 32);
}

/// Test tensor metadata byte size calculation
#[test]
fn test_tensor_metadata_byte_size() {
    let f32_meta = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    assert_eq!(f32_meta.byte_size(), 1 * 512 * 4096 * 4);

    let f16_meta = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f16".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    assert_eq!(f16_meta.byte_size(), 1 * 512 * 4096 * 2);

    let i8_meta = TensorMetadata {
        shape: vec![100, 100],
        dtype: "i8".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    assert_eq!(i8_meta.byte_size(), 100 * 100);
}

/// Test batch tensor operations
#[test]
fn test_batch_tensor_operations() {
    let tensors: Vec<(Vec<u8>, TensorMetadata)> = (0..5)
        .map(|i| {
            let data: Vec<u8> = (0..1000).map(|j| (i * 50 + j) as u8).collect();
            let metadata = TensorMetadata {
                shape: vec![10, 10, 10],
                dtype: "f32".to_string(),
                layer_range: (i as u32 * 10, i as u32 * 10 + 9),
                original_dtype: None,
            };
            (data, metadata)
        })
        .collect();

    // Batch compress
    let compressed = TensorTransport::batch_compress(tensors.clone(), 1).unwrap();
    assert_eq!(compressed.len(), 5);

    // Batch decompress
    let decompressed_refs: Vec<&CompressedTensor> = compressed.iter().collect();
    let decompressed = TensorTransport::batch_decompress(decompressed_refs).unwrap();
    assert_eq!(decompressed.len(), 5);

    // Verify each tensor
    for (i, ((orig_data, orig_meta), (decomp_data, decomp_meta))) in
        tensors.into_iter().zip(decompressed.into_iter()).enumerate()
    {
        assert_eq!(decomp_data, orig_data, "data mismatch at index {}", i);
        assert_eq!(decomp_meta.shape, orig_meta.shape);
        assert_eq!(decomp_meta.layer_range, orig_meta.layer_range);
    }
}

/// Test tensor validation
#[test]
fn test_tensor_validation() {
    let metadata = TensorMetadata {
        shape: vec![10, 20],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };

    let data: Vec<u8> = vec![0; 1000];
    let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

    // Should validate successfully
    assert!(TensorTransport::validate(&compressed).is_ok());

    // Test invalid tensor (empty shape)
    let invalid_meta = TensorMetadata {
        shape: vec![],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    let invalid = TensorTransport::compress(&data, invalid_meta, 1).unwrap();
    assert!(TensorTransport::validate(&invalid).is_err());
}

/// Test inference start routing helpers
#[test]
fn test_inference_start_routing() {
    let start = InferenceStart::new(
        "mistral-7b".to_string(),
        "Hello".to_string(),
        100,
        vec![0, 1, 2, 3, 4, 5, 6, 7],
    );

    assert_eq!(start.first_block(), Some(0));
    assert_eq!(start.last_block(), Some(7));

    // Test with empty route
    let empty = InferenceStart::new("model".to_string(), "test".to_string(), 10, vec![]);
    assert_eq!(empty.first_block(), None);
    assert_eq!(empty.last_block(), None);
}

/// Test activation chunk properties
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

    // Not final block
    let chunk1 = ActivationChunk {
        inference_id: Uuid::new_v4(),
        block_id: 0,
        next_block_id: 1,
        tensor: compressed.clone(),
        token_index: 0,
    };
    assert!(!chunk1.is_final_block());

    // Final block (block_id == next_block_id)
    let chunk2 = ActivationChunk {
        inference_id: Uuid::new_v4(),
        block_id: 7,
        next_block_id: 7,
        tensor: compressed.clone(),
        token_index: 10,
    };
    assert!(chunk2.is_final_block());

    // Total size should be positive
    assert!(chunk1.total_size_bytes() > 0);
    assert!(chunk2.total_size_bytes() > 0);
}

/// Test message inference ID extraction
#[test]
fn test_message_inference_id_extraction() {
    let inference_id = Uuid::new_v4();

    let metadata = TensorMetadata {
        shape: vec![1],
        dtype: "f32".to_string(),
        layer_range: (0, 0),
        original_dtype: None,
    };
    let compressed = TensorTransport::compress(&vec![0; 10], metadata, 1).unwrap();

    let start = PipelineMessage::InferenceStart(InferenceStart {
        inference_id,
        model_id: "".to_string(),
        prompt: "".to_string(),
        max_tokens: 0,
        pipeline_route: vec![],
    });
    assert_eq!(start.inference_id(), Some(inference_id));

    let chunk = PipelineMessage::ActivationChunk(ActivationChunk {
        inference_id,
        block_id: 0,
        next_block_id: 1,
        tensor: compressed.clone(),
        token_index: 0,
    });
    assert_eq!(chunk.inference_id(), Some(inference_id));

    let checkpoint =
        PipelineMessage::Checkpoint(distributed_inference::Checkpoint::new(inference_id, 0, compressed.clone()));
    assert_eq!(checkpoint.inference_id(), Some(inference_id));

    let proof = PipelineMessage::Proof(Proof::new(inference_id, 0, [0u8; 32], 0));
    assert_eq!(proof.inference_id(), Some(inference_id));

    let failover = PipelineMessage::Failover(Failover::new(
        inference_id,
        0,
        "".to_string(),
        "".to_string(),
        distributed_inference::Checkpoint::new(inference_id, 0, compressed),
    ));
    assert_eq!(failover.inference_id(), Some(inference_id));
}
