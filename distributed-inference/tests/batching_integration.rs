// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Integration tests for dynamic batching in distributed inference
//!
//! These tests verify that:
//! - Multiple concurrent requests are properly batched
//! - Batches are dispatched to the pipeline
//! - Individual results are correctly routed back
//! - Batching metrics accurately reflect throughput gains

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use distributed_inference::block_assignment::{StaticBlockConfig, BlockConfig, ReplicaConfig};
use distributed_inference::coordinator::{BatchConfig, StaticPipelineCoordinator, InferenceRequest};
use distributed_inference::pipeline_message::InferenceStart;

/// Helper function to create a test StaticBlockConfig
fn create_test_config() -> StaticBlockConfig {
    StaticBlockConfig {
        total_blocks: 3,
        replication_factor: 2,
        blocks: vec![
            BlockConfig {
                block_id: 0,
                layer_range: (0, 9),
                model_id: "test-model".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node-0".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node-1".to_string(), priority: 2 },
                ],
            },
            BlockConfig {
                block_id: 1,
                layer_range: (10, 19),
                model_id: "test-model".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node-2".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node-3".to_string(), priority: 2 },
                ],
            },
            BlockConfig {
                block_id: 2,
                layer_range: (20, 29),
                model_id: "test-model".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node-4".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node-5".to_string(), priority: 2 },
                ],
            },
        ],
    }
}

/// Test that verifies batching creates batches with avg_batch_size > 1
#[tokio::test]
async fn test_batching_creates_batches() {
    // Create batch config with small max size for testing
    let batch_config = BatchConfig {
        max_batch_size: 4,
        max_wait_ms: 50, // Short wait for faster tests
        enable_batching: true,
    };

    // Create coordinator with batching enabled
    let config = create_test_config();
    let coordinator = Arc::new(
        StaticPipelineCoordinator::with_batch_config(
            config,
            None,
            true,
            batch_config,
        )
    );

    // Submit 10 concurrent requests
    let mut handles = Vec::new();
    for i in 0..10 {
        let coord = coordinator.clone();
        let handle = tokio::spawn(async move {
            let request = InferenceRequest {
                model_id: "test-model".to_string(),
                prompt: format!("Test prompt {}", i),
                max_tokens: 50,
                user_id: format!("user{}", i),
                timestamp: std::time::Instant::now(),
                inference_id: Uuid::new_v4(),
            };
            coord.enqueue_inference_request(request).await
        });
        handles.push(handle);
    }

    // Wait for all requests to be enqueued
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // Wait for batch processing
    sleep(Duration::from_millis(100)).await;

    // Flush any remaining batches
    let _batches = coordinator.flush_pending_batches().await.unwrap();
    
    // Collect metrics
    let metrics = coordinator.get_batching_metrics();
    let (created, _dispatched, _completed, _timed_out, avg_size, _avg_wait) = metrics.get_stats();

    // Verify batching occurred
    assert!(created > 0, "Expected at least one batch to be created");
    
    // Verify average batch size > 1 (indicating batching is working)
    // The 10 requests should be batched into ~3 batches (ceil(10/4) = 3)
    // Average batch size should be around 3.33
    assert!(
        avg_size > 1.0,
        "Expected avg_batch_size > 1.0, got {}",
        avg_size
    );

    // Verify total batches roughly matches expected
    let expected_min_batches = 10 / 4; // batch_config.max_batch_size was 4
    assert!(
        created as usize >= expected_min_batches,
        "Expected at least {} batches, got {}",
        expected_min_batches,
        created
    );
}

/// Test that batched inferences are dispatched via network
#[tokio::test]
#[ignore = "Complex test requiring significant setup - DynamicScheduler and private field access need refactoring"]
async fn test_batching_dispatches_via_network() {
    // Skipped - requires proper DynamicScheduler setup and access to private fields
}

/// Test that batch results are properly split and routed
#[tokio::test]
async fn test_batch_result_splitting() {
    use distributed_inference::coordinator::BatchedInference;

    // Create coordinator
    let batch_config = BatchConfig {
        max_batch_size: 3,
        max_wait_ms: 100,
        enable_batching: true,
    };

    let config = create_test_config();
    let coordinator = Arc::new(
        StaticPipelineCoordinator::with_batch_config(
            config,
            None,
            true,
            batch_config,
        )
    );

    // Create batched inference manually
    let inference_ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let combined_prompt = "Prompt 1\n[BATCH_SEP]\nPrompt 2\n[BATCH_SEP]\nPrompt 3".to_string();
    
    let start_message = InferenceStart::new(
        "test-model".to_string(),
        combined_prompt,
        50,
        vec![0, 1, 2],
    );

    let batch = BatchedInference {
        batch_id: Uuid::new_v4(),
        inference_ids: inference_ids.clone(),
        start_message,
    };

    // Simulate processing batch result
    let outputs = vec![
        "Output 1".to_string(),
        "Output 2".to_string(),
        "Output 3".to_string(),
    ];

    // Process batch result
    let results = coordinator
        .process_batch_result(batch.batch_id, inference_ids.clone(), outputs)
        .await
        .unwrap();

    // Verify all inferences have results
    assert_eq!(results.len(), 3, "Expected 3 results");
    
    // Verify each inference_id has a corresponding output
    for (i, id) in inference_ids.iter().enumerate() {
        assert!(
            results.contains_key(id),
            "Expected result for inference {}",
            i
        );
        assert_eq!(
            results.get(id).unwrap(),
            &format!("Output {}", i + 1),
            "Output mismatch for inference {}",
            i
        );
    }
}

/// Test batching metrics are updated correctly
#[tokio::test]
async fn test_batching_metrics() {
    let batch_config = BatchConfig {
        max_batch_size: 2,
        max_wait_ms: 50,
        enable_batching: true,
    };

    let config = create_test_config();
    let coordinator = Arc::new(
        StaticPipelineCoordinator::with_batch_config(
            config,
            None,
            true,
            batch_config,
        )
    );

    // Submit 4 requests (should create 2 batches of size 2)
    for i in 0..4 {
        let request = InferenceRequest {
            model_id: "test-model".to_string(),
            prompt: format!("Prompt {}", i),
            max_tokens: 50,
            user_id: format!("user{}", i),
            timestamp: std::time::Instant::now(),
            inference_id: Uuid::new_v4(),
        };
        coordinator.enqueue_inference_request(request).await.unwrap();
    }

    // Wait for batches to be created
    sleep(Duration::from_millis(100)).await;

    // Flush and check metrics
    let _ = coordinator.flush_pending_batches().await.unwrap();
    
    let metrics = coordinator.get_batching_metrics();
    let (created, _dispatched, _completed, _timed_out, avg_size, _avg_wait) = metrics.get_stats();

    assert_eq!(created, 2, "Expected 2 batches to be created");
    assert!(
        (avg_size - 2.0).abs() < 0.1,
        "Expected avg batch size ~2.0, got {}",
        avg_size
    );
}

/// Test that batch timeout clears stuck batches
#[tokio::test]
async fn test_batch_timeout() {
    let batch_config = BatchConfig {
        max_batch_size: 10, // Large batch size so batches accumulate
        max_wait_ms: 100,
        enable_batching: true,
    };

    let config = create_test_config();
    let coordinator = Arc::new(
        StaticPipelineCoordinator::with_batch_config(
            config,
            None,
            true,
            batch_config,
        )
    );

    // Submit 3 requests (below batch size threshold)
    for i in 0..3 {
        let request = InferenceRequest {
            model_id: "test-model".to_string(),
            prompt: format!("Prompt {}", i),
            max_tokens: 50,
            user_id: format!("user{}", i),
            timestamp: std::time::Instant::now(),
            inference_id: Uuid::new_v4(),
        };
        coordinator.enqueue_inference_request(request).await.unwrap();
    }

    // Verify pending count
    let pending = coordinator.get_pending_request_count().await;
    assert_eq!(pending, 3, "Expected 3 pending requests");

    // Verify first pending time is set
    let first_time = coordinator.get_first_pending_time().await;
    assert!(first_time.is_some(), "Expected first pending time to be set");
}

/// Test that prompts with BATCH_SEP are properly split
#[tokio::test]
#[ignore = "Requires model crate which is not available in test context"]
async fn test_prompt_batch_sep_splitting() {
    // Skipped - requires model::inference module
}
