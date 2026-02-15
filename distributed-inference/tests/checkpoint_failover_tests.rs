// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Integration tests for checkpoint and failover functionality

use std::time::Duration;

use uuid::Uuid;

use distributed_inference::checkpoint_manager::{
    CheckpointConfig, CheckpointManager, CheckpointStorageBackend,
};
use distributed_inference::failover_coordinator::{
    FailoverConfig, FailoverMetrics,
};
use distributed_inference::tensor_transport::{TensorMetadata, TensorTransport};

fn create_test_tensor() -> distributed_inference::tensor_transport::CompressedTensor {
    let metadata = TensorMetadata {
        shape: vec![1, 512, 4096],
        dtype: "f32".to_string(),
        layer_range: (0, 9),
        original_dtype: None,
    };
    let data: Vec<u8> = vec![0; 100];
    TensorTransport::compress(&data, metadata, 1).unwrap()
}

#[tokio::test]
async fn test_checkpoint_save_and_load() {
    let manager = CheckpointManager::new();
    let inference_id = Uuid::new_v4();
    let tensor = create_test_tensor();

    // Save checkpoint
    let hash = manager
        .save_checkpoint(inference_id, 0, 10, tensor)
        .await
        .unwrap();

    assert_ne!(hash, [0u8; 32]);

    // Load latest
    let loaded = manager.get_latest_checkpoint(inference_id, 0);
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().checkpoint_hash, hash);
}

#[tokio::test]
async fn test_checkpoint_before_token() {
    let manager = CheckpointManager::new();
    let inference_id = Uuid::new_v4();

    // Save checkpoints at tokens 0, 10, 20
    for token in [0, 10, 20] {
        let tensor = create_test_tensor();
        manager
            .save_checkpoint(inference_id, 0, token, tensor)
            .await
            .unwrap();
    }

    // Get checkpoint before token 15 - should return token 10
    let checkpoint = manager.get_checkpoint_before_token(inference_id, 0, 15);
    assert!(checkpoint.is_some());
}

#[tokio::test]
async fn test_checkpoint_cleanup() {
    let manager = CheckpointManager::new();
    let inference_id = Uuid::new_v4();

    // Save some checkpoints
    for token in [0, 10, 20] {
        let tensor = create_test_tensor();
        manager
            .save_checkpoint(inference_id, 0, token, tensor)
            .await
            .unwrap();
    }

    assert_eq!(manager.get_stats().total_checkpoints, 3);

    // Cleanup
    manager.cleanup_inference(inference_id).await;

    assert_eq!(manager.get_stats().total_checkpoints, 0);
    assert!(manager.get_latest_checkpoint(inference_id, 0).is_none());
}

#[tokio::test]
async fn test_checkpoint_interval_config() {
    let config = CheckpointConfig {
        checkpoint_interval: 10,
        max_checkpoints_per_inference: 5,
        storage_backend: CheckpointStorageBackend::Memory,
        max_total_checkpoints: 1000,
    };
    let manager = CheckpointManager::with_config(config);

    assert!(manager.should_checkpoint(0).await);
    assert!(manager.should_checkpoint(10).await);
    assert!(manager.should_checkpoint(20).await);
    assert!(!manager.should_checkpoint(5).await);
    assert!(!manager.should_checkpoint(15).await);
}

#[tokio::test]
async fn test_checkpoint_lru_eviction() {
    let config = CheckpointConfig {
        max_total_checkpoints: 2,
        ..Default::default()
    };
    let manager = CheckpointManager::with_config(config);
    let inference_id = Uuid::new_v4();

    // Save 3 checkpoints (should trigger eviction of 1)
    for token in [0, 10, 20] {
        let tensor = create_test_tensor();
        manager
            .save_checkpoint(inference_id, 0, token, tensor)
            .await
            .unwrap();
    }

    // Should have only 2 after eviction
    assert_eq!(manager.get_stats().total_checkpoints, 2);
}

#[tokio::test]
async fn test_checkpoint_stats() {
    let manager = CheckpointManager::new();
    let inference_id = Uuid::new_v4();

    // Save multiple checkpoints
    for token in 0..5 {
        let tensor = create_test_tensor();
        manager
            .save_checkpoint(inference_id, 0, token * 10, tensor)
            .await
            .unwrap();
    }

    let stats = manager.get_stats();
    assert_eq!(stats.total_checkpoints, 5);
    assert_eq!(stats.inferences_tracked, 1);
    assert!(stats.total_storage_bytes > 0);
}

#[tokio::test]
async fn test_failover_metrics() {
    let metrics = FailoverMetrics::new();

    // Record some failovers
    metrics.record_failover(Duration::from_millis(100), 5);
    metrics.record_failover(Duration::from_millis(200), 3);
    metrics.record_failover(Duration::from_millis(150), 4);

    let stats = metrics.get_stats();
    assert_eq!(stats.total_failovers, 3);
    assert_eq!(stats.total_inferences_recovered, 12);
    assert_eq!(stats.avg_recovery_time_ms, 150); // (100+200+150)/3
}

#[tokio::test]
async fn test_failover_metrics_failure() {
    let metrics = FailoverMetrics::new();

    metrics.record_failover_failure();
    metrics.record_failover_failure();

    let stats = metrics.get_stats();
    assert_eq!(stats.total_failover_failures, 2);
    assert_eq!(stats.total_failovers, 0);
}

#[tokio::test]
async fn test_failover_config() {
    let config = FailoverConfig::default();
    assert_eq!(config.heartbeat_interval_ms, 500);
    assert_eq!(config.failure_threshold, 3);
    assert!(config.enable_auto_failover);
}

#[tokio::test]
async fn test_checkpoint_multiple_blocks() {
    let manager = CheckpointManager::new();
    let inference_id = Uuid::new_v4();

    // Save checkpoints for different blocks
    for block_id in 0..4 {
        let tensor = create_test_tensor();
        manager
            .save_checkpoint(inference_id, block_id, 10, tensor)
            .await
            .unwrap();
    }

    // Verify we can retrieve checkpoints for each block
    for block_id in 0..4 {
        let checkpoint = manager.get_latest_checkpoint(inference_id, block_id);
        assert!(checkpoint.is_some(), "Checkpoint should exist for block {}", block_id);
    }

    let stats = manager.get_stats();
    assert_eq!(stats.total_checkpoints, 4);
}

#[tokio::test]
async fn test_checkpoint_multiple_inferences() {
    let manager = CheckpointManager::new();

    // Create multiple inferences
    let inference_ids: Vec<_> = (0..3).map(|_| Uuid::new_v4()).collect();

    for (i, inference_id) in inference_ids.iter().enumerate() {
        // Each inference has some checkpoints
        for token in 0..=i as u32 {
            let tensor = create_test_tensor();
            manager
                .save_checkpoint(*inference_id, 0, token * 10, tensor)
                .await
                .unwrap();
        }
    }

    let stats = manager.get_stats();
    assert_eq!(stats.total_checkpoints, 6); // 1 + 2 + 3
    assert_eq!(stats.inferences_tracked, 3);

    // Cleanup one inference
    manager.cleanup_inference(inference_ids[0]).await;

    let stats_after = manager.get_stats();
    assert_eq!(stats_after.total_checkpoints, 5); // 2 + 3
    assert_eq!(stats_after.inferences_tracked, 2);
}

#[tokio::test]
async fn test_checkpoint_integrity() {
    use distributed_inference::pipeline_message::Checkpoint;

    let tensor = create_test_tensor();
    let inference_id = Uuid::new_v4();
    let checkpoint = Checkpoint::new(inference_id, 0, tensor);

    // Verify integrity passes
    assert!(checkpoint.verify_integrity());

    // Create a corrupted checkpoint
    let mut corrupted = checkpoint.clone();
    corrupted.checkpoint_hash[0] ^= 0xFF;

    // Integrity check should fail
    assert!(!corrupted.verify_integrity());
}
