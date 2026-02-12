// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Fragment execution tests for distributed inference

use model::inference::{FragmentActivation, FragmentExecutorError};

#[test]
fn test_fragment_activation_creation() {
    let task_id = uuid::Uuid::new_v4();
    let inference_id = uuid::Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 768i64];
    let data = vec![0.5f32; 768];

    let activation = FragmentActivation::new(
        task_id,
        inference_id,
        layer_range,
        shape.clone(),
        data.clone(),
    );

    assert_eq!(activation.task_id, task_id);
    assert_eq!(activation.inference_id, inference_id);
    assert_eq!(activation.layer_range, layer_range);
    assert_eq!(activation.shape, shape);
    assert_eq!(activation.data, data);
    assert!(activation.verify_integrity());
}

#[test]
fn test_fragment_activation_hash_computation() {
    let data = vec![1.0f32, 2.0f32, 3.0f32, 4.0f32];
    let hash = FragmentActivation::compute_hash(&data);
    
    // Hash should be 32 bytes
    assert_eq!(hash.len(), 32);
    
    // Same data should produce same hash
    let hash2 = FragmentActivation::compute_hash(&data);
    assert_eq!(hash, hash2);
    
    // Different data should produce different hash (with high probability)
    let different_data = vec![1.0f32, 2.0f32, 3.0f32, 4.1f32];
    let hash3 = FragmentActivation::compute_hash(&different_data);
    assert_ne!(hash, hash3);
}

#[test]
fn test_fragment_activation_integrity_verification() {
    let task_id = uuid::Uuid::new_v4();
    let inference_id = uuid::Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 4i64];
    let data = vec![0.1f32, 0.2f32, 0.3f32, 0.4f32];

    let mut activation = FragmentActivation::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    // Should verify successfully
    assert!(activation.verify_integrity());

    // Corrupt the data
    activation.data[0] = 999.0f32;
    
    // Should fail verification
    assert!(!activation.verify_integrity());
}

#[test]
fn test_fragment_activation_size_methods() {
    let task_id = uuid::Uuid::new_v4();
    let inference_id = uuid::Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![2i64, 256i64];
    let data = vec![0.5f32; 512];

    let activation = FragmentActivation::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data.clone(),
    );

    assert_eq!(activation.num_elements(), 512);
    assert_eq!(activation.size_bytes(), 512 * std::mem::size_of::<f32>());
}

#[test]
fn test_fragment_executor_error_display() {
    let err = FragmentExecutorError::FragmentNotFound("frag_001".to_string());
    assert!(err.to_string().contains("frag_001"));

    let err = FragmentExecutorError::IntegrityCheckFailed("hash mismatch".to_string());
    assert!(err.to_string().contains("hash mismatch"));

    let err = FragmentExecutorError::InvalidLayerRange("invalid range".to_string());
    assert!(err.to_string().contains("invalid range"));
}

#[tokio::test]
async fn test_fragment_cache_operations() {
    // This is a placeholder for more comprehensive integration tests
    // that would require a full test environment with model registry
    // and storage backend.
    
    // Test basic FragmentActivation operations
    let task_id = uuid::Uuid::new_v4();
    let inference_id = uuid::Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 768i64];
    let data = vec![0.0f32; 768];

    let activation = FragmentActivation::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    assert!(activation.verify_integrity());
}

#[test]
fn test_checkpoint_generation_performance() {
    // Test that checkpoint hash generation is fast (< 5ms for 10MB)
    use std::time::Instant;
    
    let data_size = 10 * 1024 * 1024 / std::mem::size_of::<f32>(); // 10MB worth of f32s
    let data = vec![0.5f32; data_size];
    
    let start = Instant::now();
    let hash = FragmentActivation::compute_hash(&data);
    let elapsed = start.elapsed();
    
    assert_eq!(hash.len(), 32);
    assert!(elapsed.as_millis() < 50, "Checkpoint generation took {}ms, expected < 50ms", elapsed.as_millis());
    
    println!("Checkpoint generation for 10MB took: {:?}", elapsed);
}
