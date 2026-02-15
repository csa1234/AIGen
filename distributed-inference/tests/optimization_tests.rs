// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Optimization Tests for Distributed Inference Pipeline
//!
//! Comprehensive unit and integration tests for:
//! - 8-bit quantization roundtrip
//! - Dynamic batching
//! - Computation/communication overlap
//! - Topology optimization
//! - Distributed PoI verification
//! - Compression ratio tracking

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use consensus::poi::DistributedTaskProof;
use distributed_inference::{
    coordinator::{BatchingMetrics},
    topology_optimizer::{TopologyOptimizer, TopologyMetrics},
    tensor_transport::{TensorTransport},
};
use network::metrics::NetworkMetrics;
use uuid::Uuid;

/// Test quantization roundtrip: f32 -> i8 -> f32 preserves values within epsilon
#[test]
fn test_quantization_roundtrip() {
    // Create test data: random f32 values in range [-1.0, 1.0]
    let test_data: Vec<f32> = vec![
        -1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0,
        -0.123, 0.456, -0.789, 0.321, -0.654, 0.987
    ];
    
    // Quantize
    let quantized = TensorTransport::quantize_f32_to_i8(&test_data);
    
    // Dequantize
    let reconstructed = TensorTransport::dequantize_i8_to_f32(&quantized);
    
    // Verify values are preserved within epsilon (tolerance for quantization error)
    let epsilon = 0.02; // 2% error tolerance for 8-bit quantization
    for (original, reconstructed) in test_data.iter().zip(reconstructed.iter()) {
        let error = (original - reconstructed).abs();
        assert!(
            error <= epsilon,
            "Quantization error {} exceeds epsilon {} for value {}",
            error, epsilon, original
        );
    }
}

/// Test quantization with values outside [-1.0, 1.0] range (clamping)
#[test]
fn test_quantization_clamping() {
    // Values outside the range should be clamped
    let test_data: Vec<f32> = vec![-2.0, -1.5, 1.5, 2.0, 10.0, -10.0];
    
    let quantized = TensorTransport::quantize_f32_to_i8(&test_data);
    
    // All values should be clamped to [-127, 127] range
    for q in &quantized {
        assert!(*q >= i8::MIN && *q <= i8::MAX, "Quantized value {} out of range", *q);
    }
    
    // Verify extreme values are clamped
    assert_eq!(quantized[0], -127); // -2.0 clamped to -127
    assert_eq!(quantized[1], -127); // -1.5 clamped to -127
    assert_eq!(quantized[2], 127);  // 1.5 clamped to 127
    assert_eq!(quantized[3], 127);  // 2.0 clamped to 127
    assert_eq!(quantized[4], 127);  // 10.0 clamped to 127
    assert_eq!(quantized[5], -127);   // -10.0 clamped to -127
}

/// Test compression stats tracking
#[test]
fn test_compression_stats_tracking() {
    let metrics = Arc::new(NetworkMetrics::new());
    
    // Record some compression operations
    metrics.record_tensor_compression(1000, 200); // 5x compression
    metrics.record_tensor_compression(2000, 400); // 5x compression
    metrics.record_tensor_compression(3000, 600); // 5x compression
    
    // Get compression stats from tensor transport
    let (total_tensors, bytes_saved, avg_ratio) = TensorTransport::get_compression_stats();
    
    // Verify tensor transport tracks compression
    assert!(total_tensors > 0, "Total tensors should be positive");
    assert!(bytes_saved > 0, "Bytes saved should be positive");
    assert!(avg_ratio >= 1.0, "Compression ratio should be at least 1.0 (no compression)");
}

/// Test block latency metrics recording
#[test]
fn test_block_latency_metrics() {
    let metrics = Arc::new(NetworkMetrics::new());
    
    // Record task completions for different blocks
    metrics.record_block_task_completion(0, 100, 50);
    metrics.record_block_task_completion(0, 120, 60);
    metrics.record_block_task_completion(1, 150, 70);
    metrics.record_block_task_completion(1, 180, 80);
    
    // Get block latency metrics
    let block0_metrics = metrics.get_block_latency_metrics(0).unwrap();
    let block1_metrics = metrics.get_block_latency_metrics(1).unwrap();
    
    // Verify compute times are recorded
    assert!(block0_metrics.avg_compute_time_ms > 0.0, "Block 0 should have compute time");
    assert!(block1_metrics.avg_compute_time_ms > 0.0, "Block 1 should have compute time");
    
    // Verify transfer times are recorded
    assert!(block0_metrics.avg_transfer_time_ms > 0.0, "Block 0 should have transfer time");
    assert!(block1_metrics.avg_transfer_time_ms > 0.0, "Block 1 should have transfer time");
    
    // Verify task counts
    assert_eq!(block0_metrics.tasks_completed, 2);
    assert_eq!(block1_metrics.tasks_completed, 2);
    
    // Get all block metrics
    let all_metrics = metrics.get_all_block_latency_metrics();
    assert_eq!(all_metrics.len(), 2);
}

/// Test block task failure recording
#[test]
fn test_block_task_failure_recording() {
    let metrics = Arc::new(NetworkMetrics::new());
    
    // Record task completion then failure
    metrics.record_block_task_completion(0, 100, 50);
    metrics.record_block_task_failure(0);
    metrics.record_block_task_failure(0);
    
    let block_metrics = metrics.get_block_latency_metrics(0).unwrap();
    assert_eq!(block_metrics.tasks_completed, 1);
    assert_eq!(block_metrics.tasks_failed, 2);
}

/// Test topology optimizer TSP-like optimization
#[test]
fn test_topology_optimization() {
    use distributed_inference::replica_manager::{BlockReplica, ReplicaManager};
    use libp2p::PeerId;
    use network::protocol::NodeCapabilities;
    use std::str::FromStr;
    
    // Create a replica manager
    let replica_manager = Arc::new(ReplicaManager::new());
    
    // Create a topology optimizer
    let optimizer = TopologyOptimizer::new(replica_manager.clone());
    
    // Create test replicas
    let create_replica = |block_id: u32, node_id: &str| -> BlockReplica {
        BlockReplica {
            block_id,
            node_id: node_id.to_string(),
            peer_id: PeerId::from_str("12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN").unwrap(),
            capabilities: NodeCapabilities {
                has_gpu: false,
                gpu_model: None,
                supports_inference: true,
                supports_training: false,
                max_fragment_size_mb: 512,
            },
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.5,
            rtt_map: HashMap::new(),
        }
    };
    
    // Set up RTT matrix for 4 nodes in a ring:
    // Node 0 --10ms-- Node 1 --10ms-- Node 2 --10ms-- Node 3
    //  |________________________________________________|
    //                    50ms (long connection)
    optimizer.update_rtt_measurement("node-0", "node-1", Duration::from_millis(10));
    optimizer.update_rtt_measurement("node-1", "node-2", Duration::from_millis(10));
    optimizer.update_rtt_measurement("node-2", "node-3", Duration::from_millis(10));
    optimizer.update_rtt_measurement("node-0", "node-3", Duration::from_millis(50));
    optimizer.update_rtt_measurement("node-0", "node-2", Duration::from_millis(30));
    optimizer.update_rtt_measurement("node-1", "node-3", Duration::from_millis(30));
    
    // Create replicas
    let replicas = vec![
        create_replica(0, "node-0"),
        create_replica(1, "node-1"),
        create_replica(2, "node-2"),
        create_replica(3, "node-3"),
    ];
    
    // Optimize the topology
    let optimized = optimizer.optimize_ring_topology(replicas);
    
    // Verify all replicas are included
    assert_eq!(optimized.len(), 4);
    
    // Verify the order is optimized (should prefer short connections)
    // The optimal order should be: node-0 -> node-1 -> node-2 -> node-3 (or reverse)
    // which gives total RTT of 10+10+10 = 30ms, not the 50ms direct connection
    let node_ids: Vec<_> = optimized.iter().map(|r| r.node_id.clone()).collect();
    assert!(node_ids.contains(&"node-0".to_string()));
    assert!(node_ids.contains(&"node-1".to_string()));
    assert!(node_ids.contains(&"node-2".to_string()));
    assert!(node_ids.contains(&"node-3".to_string()));
    
    // Calculate total latency for the optimized route
    let total_latency = optimizer.calculate_total_pipeline_latency(&optimized);
    
    // Should be close to 30ms (10+10+10) not 50ms (the direct connection)
    assert!(
        total_latency.as_millis() <= 35,
        "Optimized latency {}ms should be close to 30ms",
        total_latency.as_millis()
    );
    
    // Check metrics were recorded
    let (optimizations, avg_rtt, _improvement) = optimizer.get_metrics().get_stats();
    assert_eq!(optimizations, 1);
    assert!(avg_rtt > 0.0);
}

/// Test topology optimizer with empty or single-node input
#[test]
fn test_topology_optimization_edge_cases() {
    use distributed_inference::replica_manager::{BlockReplica, ReplicaManager};
    use libp2p::PeerId;
    use network::protocol::NodeCapabilities;
    use std::str::FromStr;
    
    let replica_manager = Arc::new(ReplicaManager::new());
    let optimizer = TopologyOptimizer::new(replica_manager);
    
    // Test empty input
    let empty: Vec<BlockReplica> = vec![];
    let result = optimizer.optimize_ring_topology(empty);
    assert!(result.is_empty());
    
    // Test single node
    let single = vec![BlockReplica {
        block_id: 0,
        node_id: "node-0".to_string(),
        peer_id: PeerId::from_str("12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN").unwrap(),
        capabilities: NodeCapabilities {
            has_gpu: false,
            gpu_model: None,
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 512,
        },
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.5,
        rtt_map: HashMap::new(),
    }];
    let result = optimizer.optimize_ring_topology(single);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].node_id, "node-0");
}

/// Test distributed PoI proof creation and verification
#[test]
fn test_distributed_poi_verification() {
    use consensus::poi::{verify_distributed_inference_proof, collect_block_proofs};
    
    // Create block proofs
    let block_proofs = vec![
        DistributedTaskProof::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec!["frag-1".to_string(), "frag-2".to_string()],
            (0, 9),
            [0u8; 32],
            [1u8; 32], // This becomes input for next block
            None,
            100,
            "node-0".to_string(),
            12345,
        ),
        DistributedTaskProof::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec!["frag-3".to_string(), "frag-4".to_string()],
            (10, 19),
            [1u8; 32], // Matches previous block's output
            [2u8; 32], // This becomes input for next block
            None,
            150,
            "node-1".to_string(),
            12346,
        ),
        DistributedTaskProof::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec!["frag-5".to_string()],
            (20, 29),
            [2u8; 32], // Matches previous block's output
            [3u8; 32],
            Some([4u8; 32]),
            200,
            "node-2".to_string(),
            12347,
        ),
    ];
    
    // Collect into distributed proof
    let inference_id = Uuid::new_v4();
    let proof = collect_block_proofs(
        inference_id,
        block_proofs,
        "coordinator-node".to_string(),
    );
    
    // Verify the proof
    let result = verify_distributed_inference_proof(&proof, None);
    assert!(result.is_ok());
    assert!(result.unwrap(), "Proof should be valid");
    
    // Verify proof properties
    assert_eq!(proof.inference_id, inference_id);
    assert_eq!(proof.block_proofs.len(), 3);
    assert_eq!(proof.total_blocks, 3);
    assert_eq!(proof.collector_node_id, "coordinator-node");
}

/// Test distributed PoI verification with broken hash chain
#[test]
fn test_distributed_poi_broken_chain() {
    use consensus::poi::{verify_distributed_inference_proof, collect_block_proofs};
    
    // Create block proofs with mismatched hashes (broken chain)
    let block_proofs = vec![
        DistributedTaskProof::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec!["frag-1".to_string()],
            (0, 9),
            [0u8; 32],
            [1u8; 32], // Output
            None,
            100,
            "node-0".to_string(),
            12345,
        ),
        DistributedTaskProof::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec!["frag-2".to_string()],
            (10, 19),
            [99u8; 32], // Does NOT match previous output [1u8; 32]
            [2u8; 32],
            None,
            150,
            "node-1".to_string(),
            12346,
        ),
    ];
    
    let proof = collect_block_proofs(
        Uuid::new_v4(),
        block_proofs,
        "coordinator-node".to_string(),
    );
    
    // Verification should fail due to broken chain
    let result = verify_distributed_inference_proof(&proof, None);
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Proof with broken chain should be invalid");
}

/// Test topology metrics recording
#[test]
fn test_topology_metrics() {
    let metrics = TopologyMetrics::default();
    
    // Record several optimizations
    metrics.record_optimization(100, 150); // 50ms improvement from 150ms baseline
    metrics.record_optimization(90, 150);   // 60ms improvement
    metrics.record_optimization(80, 150);   // 70ms improvement
    
    // Get stats
    let (optimizations, avg_rtt, improvement) = metrics.get_stats();
    
    assert_eq!(optimizations, 3);
    assert!(avg_rtt > 0.0);
    assert!(improvement > 0.0);
    
    // Average RTT should be around 90ms
    assert!(avg_rtt >= 80.0 && avg_rtt <= 100.0);
    
    // Improvement should be positive (we're optimizing)
    assert!(improvement > 0.0);
}

/// Integration test: Simulate distributed inference with all optimizations
#[test]
fn test_distributed_inference_optimization_integration() {
    // This test simulates a full pipeline with multiple optimizations active
    
    use network::metrics::NetworkMetrics;
    
    let metrics = Arc::new(NetworkMetrics::new());
    
    // Simulate 10 inference tasks
    for i in 0..10 {
        // Record block task completions with varying times
        let compute_time = 100 + (i % 5) * 10; // 100-140ms
        let transfer_time = 50 + (i % 3) * 5;  // 50-60ms
        
        metrics.record_block_task_completion(0, compute_time, transfer_time);
        metrics.record_block_task_completion(1, compute_time + 20, transfer_time + 10);
        metrics.record_block_task_completion(2, compute_time + 40, transfer_time + 20);
        
        // Record tensor compression
        let original_size = 1000;
        let compressed_size = 200 + i * 10; // Improving compression over time
        metrics.record_tensor_compression(original_size as usize, compressed_size as usize);
    }
    
    // Verify block metrics
    let block0 = metrics.get_block_latency_metrics(0).unwrap();
    assert_eq!(block0.tasks_completed, 10);
    
    let block1 = metrics.get_block_latency_metrics(1).unwrap();
    assert_eq!(block1.tasks_completed, 10);
    
    let block2 = metrics.get_block_latency_metrics(2).unwrap();
    assert_eq!(block2.tasks_completed, 10);
    
    // All blocks should have compute and transfer times recorded
    assert!(block0.avg_compute_time_ms > 0.0);
    assert!(block1.avg_compute_time_ms > 0.0);
    assert!(block2.avg_compute_time_ms > 0.0);
    
    assert!(block0.avg_transfer_time_ms > 0.0);
    assert!(block1.avg_transfer_time_ms > 0.0);
    assert!(block2.avg_transfer_time_ms > 0.0);
    
    // Get all block metrics
    let all_metrics = metrics.get_all_block_latency_metrics();
    assert_eq!(all_metrics.len(), 3);
}

/// Test batching metrics
#[test]
fn test_batching_metrics() {
    let metrics = Arc::new(BatchingMetrics::default());
    
    // Record batches
    metrics.record_batch(5, 50);  // 5 items, 50ms wait
    metrics.record_batch(10, 100); // 10 items, 100ms wait
    metrics.record_batch(8, 75);  // 8 items, 75ms wait
    
    // Get stats
    let (batches, _dispatched, _completed, _timed_out, avg_size, avg_wait) = metrics.get_stats();
    
    assert_eq!(batches, 3);
    assert!(avg_size >= 7.0 && avg_size <= 8.0, "Expected avg_size between 7 and 8, got {}", avg_size); // Average of 5, 10, 8
    assert!(avg_wait >= 70 && avg_wait <= 85, "Expected avg_wait between 70 and 85, got {}", avg_wait); // Average of 50, 100, 75
}

/// Test compression ratio calculation
#[test]
fn test_compression_ratio_calculation() {
    let metrics = Arc::new(NetworkMetrics::new());
    
    // Test various compression ratios
    metrics.record_tensor_compression(1000, 500);  // 2.0x
    metrics.record_tensor_compression(1000, 250);  // 4.0x
    metrics.record_tensor_compression(1000, 200);  // 5.0x
    metrics.record_tensor_compression(1000, 100);  // 10.0x
    
    // Get global metrics
    let global = metrics.get_global_metrics();
    let ratio = global.get_compression_ratio();
    
    // Average should be around 5.25x
    assert!(ratio >= 4.0 && ratio <= 6.0, "Compression ratio {} out of expected range", ratio);
}
