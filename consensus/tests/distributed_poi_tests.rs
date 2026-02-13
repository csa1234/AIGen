// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Distributed PoI Verification Tests
//!
//! Tests for distributed task proof verification and reward calculation

use consensus::poi::{DistributedTaskProof, verify_distributed_task, calculate_poi_reward, WorkType, CompressionMethod, ComputationMetadata};
use consensus::PoIProof;
use sha3::{Digest, Sha3_256};
use uuid::Uuid;

fn create_test_distributed_proof(
    task_id: Uuid,
    fragment_count: u32,
    layer_start: u32,
    layer_end: u32,
    compute_time_ms: u64,
    has_checkpoint: bool,
) -> DistributedTaskProof {
    let fragment_ids: Vec<String> = (0..fragment_count)
        .map(|i| format!("frag-{}", i))
        .collect();
    
    let mut input_hash = [0u8; 32];
    let mut output_hash = [1u8; 32];
    let checkpoint_hash = if has_checkpoint { Some([2u8; 32]) } else { None };
    
    // Populate with some non-zero data
    for i in 0..32 {
        input_hash[i] = i as u8;
        output_hash[i] = (i + 100) as u8;
    }
    
    DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        fragment_ids,
        (layer_start, layer_end),
        input_hash,
        output_hash,
        checkpoint_hash,
        compute_time_ms,
        "test-node".to_string(),
    )
}

fn create_poi_proof(distributed_proof: &DistributedTaskProof, difficulty: u64) -> PoIProof {
    let verification_data = serde_json::json!({
        "distributed_task_proof": distributed_proof
    });
    
    PoIProof::new(
        distributed_proof.node_id.clone(),
        WorkType::DistributedInference,
        distributed_proof.input_activation_hash,
        distributed_proof.output_activation_hash.to_vec(),
        ComputationMetadata {
            rows: 0,
            cols: 0,
            inner: 0,
            iterations: 0,
            model_id: "test-model".to_string(),
            compression_method: CompressionMethod::None,
            original_size: 1000000,
        },
        difficulty,
        1234567890,
        42,
        verification_data,
    )
}

#[test]
fn test_verify_distributed_task_valid() {
    let task_id = Uuid::new_v4();
    let proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    let result = verify_distributed_task(&proof);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_verify_distributed_task_empty_fragments() {
    let task_id = Uuid::new_v4();
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec![], // Empty fragments
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    
    let result = verify_distributed_task(&proof);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), false);
}

#[test]
fn test_verify_distributed_task_invalid_layer_range() {
    let task_id = Uuid::new_v4();
    
    // Start == End
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (5, 5), // Invalid: start == end
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    
    let result = verify_distributed_task(&proof);
    assert_eq!(result.unwrap(), false);
    
    // Start > End
    let proof2 = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (10, 5), // Invalid: start > end
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    
    let result2 = verify_distributed_task(&proof2);
    assert_eq!(result2.unwrap(), false);
}

#[test]
fn test_verify_distributed_task_invalid_activation_hashes() {
    let task_id = Uuid::new_v4();
    
    // Input hash all zeros
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        [0u8; 32], // Invalid: all zeros
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    
    let result = verify_distributed_task(&proof);
    assert_eq!(result.unwrap(), false);
    
    // Output hash all zeros
    let proof2 = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        [1u8; 32],
        [0u8; 32], // Invalid: all zeros
        None,
        1000,
        "test-node".to_string(),
    );
    
    let result2 = verify_distributed_task(&proof2);
    assert_eq!(result2.unwrap(), false);
}

#[test]
fn test_verify_distributed_task_zero_compute_time() {
    let task_id = Uuid::new_v4();
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        0, // Invalid: zero compute time
        "test-node".to_string(),
    );
    
    let result = verify_distributed_task(&proof);
    assert_eq!(result.unwrap(), false);
}

#[test]
fn test_verify_distributed_task_empty_node_id() {
    let task_id = Uuid::new_v4();
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "".to_string(), // Invalid: empty node_id
    );
    
    let result = verify_distributed_task(&proof);
    assert_eq!(result.unwrap(), false);
}

#[test]
fn test_calculate_reward_fragment_factor() {
    // Test 1 fragment, 10 layers
    let proof1 = create_test_distributed_proof(Uuid::new_v4(), 1, 0, 10, 1000, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // Test 5 fragments, 10 layers (5x fragment count)
    let proof2 = create_test_distributed_proof(Uuid::new_v4(), 5, 0, 10, 1000, false);
    let poi2 = create_poi_proof(&proof2, 1000);
    let reward2 = calculate_poi_reward(&poi2).unwrap();
    
    // Test 5 fragments, 50 layers (5x fragment count, 5x layer range)
    let proof3 = create_test_distributed_proof(Uuid::new_v4(), 5, 0, 50, 1000, false);
    let poi3 = create_poi_proof(&proof3, 1000);
    let reward3 = calculate_poi_reward(&poi3).unwrap();
    
    // Reward should scale with fragment count
    assert!(reward2.value() > reward1.value());
    
    // Reward should scale with layer range as well
    assert!(reward3.value() >= reward2.value());
}

#[test]
fn test_calculate_reward_time_factor() {
    let task_id = Uuid::new_v4();
    
    // Fast task (100ms)
    let proof1 = create_test_distributed_proof(task_id, 3, 0, 10, 100, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // Slow task (10000ms)
    let proof2 = create_test_distributed_proof(task_id, 3, 0, 10, 10000, false);
    let poi2 = create_poi_proof(&proof2, 1000);
    let reward2 = calculate_poi_reward(&poi2).unwrap();
    
    // Very slow task (50000ms)
    let proof3 = create_test_distributed_proof(task_id, 3, 0, 10, 50000, false);
    let poi3 = create_poi_proof(&proof3, 1000);
    let reward3 = calculate_poi_reward(&poi3).unwrap();
    
    // Longer tasks should have higher rewards (capped at 50% bonus)
    assert!(reward2.value() >= reward1.value());
    assert!(reward3.value() >= reward2.value());
}

#[test]
fn test_calculate_reward_checkpoint_bonus() {
    let task_id = Uuid::new_v4();
    
    // Without checkpoint
    let proof1 = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // With checkpoint (20% bonus)
    let proof2 = create_test_distributed_proof(task_id, 3, 0, 10, 1000, true);
    let poi2 = create_poi_proof(&proof2, 1000);
    let reward2 = calculate_poi_reward(&poi2).unwrap();
    
    // Checkpoint should give ~20% bonus
    assert!(reward2.value() > reward1.value());
    
    // Verify the bonus is roughly 20%
    let bonus_ratio = (reward2.value() as f64 - reward1.value() as f64) / reward1.value() as f64;
    assert!(bonus_ratio > 0.15 && bonus_ratio < 0.25, 
        "Checkpoint bonus should be ~20%, got {:.2}%", bonus_ratio * 100.0);
}

#[test]
fn test_calculate_reward_edge_cases() {
    // Test zero fragments
    let task_id = Uuid::new_v4();
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec![],
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    let poi = create_poi_proof(&proof, 1000);
    // Should not panic even with empty fragments
    let reward = calculate_poi_reward(&poi);
    assert!(reward.is_ok());
    
    // Test max fragments (100)
    let task_id = Uuid::new_v4();
    let mut fragment_ids = Vec::new();
    for i in 0..100 {
        fragment_ids.push(format!("frag-{:03}", i));
    }
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        fragment_ids,
        (0, 100), // 100 layers
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    let poi = create_poi_proof(&proof, 1000);
    let reward = calculate_poi_reward(&poi).unwrap();
    
    // High fragment count should result in high reward
    assert!(reward.value() > 100);
}

#[test]
fn test_calculate_reward_difficulty_factor() {
    let task_id = Uuid::new_v4();
    let proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    // Low difficulty
    let poi1 = create_poi_proof(&proof, 500);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // Medium difficulty
    let poi2 = create_poi_proof(&proof, 1000);
    let reward2 = calculate_poi_reward(&poi2).unwrap();
    
    // High difficulty
    let poi3 = create_poi_proof(&proof, 2000);
    let reward3 = calculate_poi_reward(&poi3).unwrap();
    
    // Higher difficulty should result in higher base reward
    assert!(reward2.value() >= reward1.value());
    assert!(reward3.value() >= reward2.value());
}

#[test]
fn test_distributed_proof_serialization() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let original = DistributedTaskProof::new(
        task_id,
        inference_id,
        vec!["frag1".to_string(), "frag2".to_string()],
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        Some([2u8; 32]),
        5000,
        "test-node".to_string(),
    );
    
    // Serialize
    let serialized = serde_json::to_string(&original).unwrap();
    
    // Deserialize
    let deserialized: DistributedTaskProof = serde_json::from_str(&serialized).unwrap();
    
    // Verify all fields match
    assert_eq!(deserialized.task_id, original.task_id);
    assert_eq!(deserialized.inference_id, original.inference_id);
    assert_eq!(deserialized.fragment_ids, original.fragment_ids);
    assert_eq!(deserialized.layer_range, original.layer_range);
    assert_eq!(deserialized.input_activation_hash, original.input_activation_hash);
    assert_eq!(deserialized.output_activation_hash, original.output_activation_hash);
    assert_eq!(deserialized.checkpoint_hash, original.checkpoint_hash);
    assert_eq!(deserialized.compute_time_ms, original.compute_time_ms);
    assert_eq!(deserialized.node_id, original.node_id);
}

#[test]
fn test_poi_proof_with_distributed_data() {
    let task_id = Uuid::new_v4();
    let distributed_proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    let poi = create_poi_proof(&distributed_proof, 1000);
    
    // Verify it's a DistributedInference work type
    assert_eq!(poi.work_type, WorkType::DistributedInference);
    
    // Verify verification_data contains distributed_task_proof
    let proof_data = poi.verification_data.get("distributed_task_proof");
    assert!(proof_data.is_some());
    
    // Verify we can deserialize it back
    let deserialized: DistributedTaskProof = serde_json::from_value(proof_data.unwrap().clone()).unwrap();
    assert_eq!(deserialized.task_id, task_id);
    assert_eq!(deserialized.fragment_ids.len(), 3);
}

#[test]
fn test_proof_verification_with_wrong_activation_hash() {
    let task_id = Uuid::new_v4();
    
    // Create proof with specific hashes
    let mut input_hash = [0u8; 32];
    let mut output_hash = [1u8; 32];
    for i in 0..32 {
        input_hash[i] = i as u8;
        output_hash[i] = (i + 100) as u8;
    }
    
    let proof = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        input_hash,
        output_hash,
        None,
        1000,
        "test-node".to_string(),
    );
    
    // Verify the proof with correct hashes
    let result = verify_distributed_task(&proof);
    assert!(result.unwrap());
    
    // Create proof with wrong output hash (should still pass basic verification)
    let wrong_output_hash = [99u8; 32];
    let proof2 = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        input_hash,
        wrong_output_hash,
        None,
        1000,
        "test-node".to_string(),
    );
    
    // Basic verification doesn't check hash correctness - just non-zero
    let result2 = verify_distributed_task(&proof2);
    assert!(result2.unwrap());
}

#[test]
fn test_reward_formula_components() {
    let task_id = Uuid::new_v4();
    
    // Base case: 1 fragment, 1 layer, 0ms, no checkpoint
    let proof1 = create_test_distributed_proof(task_id, 1, 0, 1, 0, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // Base + difficulty: 100 * 1 = 100 (from difficulty factor)
    assert!(reward1.value() >= 100);
    
    // Test fragment_factor = 0 is handled
    let proof2 = create_test_distributed_proof(task_id, 1, 0, 0, 1000, false);
    let poi2 = create_poi_proof(&proof2, 1000);
    // This would have layer_range of 0, which is invalid but we test it doesn't panic
    let reward2 = calculate_poi_reward(&poi2);
    assert!(reward2.is_ok());
}
