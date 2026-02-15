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

use consensus::poi::{DistributedTaskProof, verify_distributed_task, verify_poi_proof, calculate_poi_reward, WorkType, CompressionMethod, ComputationMetadata, poi_proof_work_payload_bytes, validate_work_difficulty, ConsensusError};
use consensus::PoIProof;
use blockchain_core::hash_data;
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
        0, // nonce
    )
}

fn create_poi_proof(distributed_proof: &DistributedTaskProof, difficulty: u64) -> PoIProof {
    let miner_address = distributed_proof.node_id.clone();
    let work_type = WorkType::DistributedInference;
    let input_hash = distributed_proof.input_activation_hash;
    let output_data = distributed_proof.output_activation_hash.to_vec();
    let metadata = ComputationMetadata {
        rows: 0,
        cols: 0,
        inner: 0,
        iterations: 0,
        model_id: "test-model".to_string(),
        compression_method: CompressionMethod::None,
        original_size: 1000000,
    };
    // Use current timestamp to pass verify_poi_proof time window validation
    let timestamp = chrono::Utc::now().timestamp();
    
    // Mining loop to find a valid nonce
    let mut nonce: u64 = 0;
    const MAX_NONCE: u64 = 100_000;
    
    while nonce < MAX_NONCE {
        let payload = poi_proof_work_payload_bytes(
            &miner_address,
            work_type,
            input_hash,
            &output_data,
            &metadata,
            difficulty,
            timestamp,
            nonce,
        );
        let work_hash_vec = hash_data(&payload);
        let work_hash: [u8; 32] = work_hash_vec.try_into().expect("Hash should be 32 bytes");
        
        if validate_work_difficulty(&work_hash, difficulty).is_ok() {
            // Clone the distributed proof and update its nonce to match the PoI nonce
            let mut distributed_proof_with_nonce = distributed_proof.clone();
            distributed_proof_with_nonce.nonce = nonce;
            
            let verification_data = serde_json::json!({
                "distributed_task_proof": distributed_proof_with_nonce
            });
            
            let poi = PoIProof::new(
                work_hash,
                miner_address,
                timestamp,
                verification_data,
                work_type,
                input_hash,
                output_data,
                metadata,
                difficulty,
                nonce,
            );
            
            // Verify the generated proof passes full validation
            assert!(
                verify_poi_proof(&poi).expect("Proof verification should not fail"),
                "Test helper create_poi_proof generated an invalid proof - verify_poi_proof returned false"
            );
            
            return poi;
        }
        
        nonce += 1;
    }
    
    panic!("Mining timeout: Could not find a valid nonce within {} iterations. Check difficulty configuration.", MAX_NONCE);
}

#[test]
fn test_verify_distributed_task_valid() {
    let task_id = Uuid::new_v4();
    let proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    let result2 = verify_distributed_task(&proof2, None, None, None);
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
        0, // nonce
    );
    
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    let result2 = verify_distributed_task(&proof2, None, None, None);
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
        0, // nonce
    );
    
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    let result = verify_distributed_task(&proof, None, None, None);
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
    let base_reward = 100u64; // difficulty=1000 → factor=1 → base=100
    
    // Without checkpoint
    let proof1 = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // With checkpoint (20% bonus)
    let proof2 = create_test_distributed_proof(task_id, 3, 0, 10, 1000, true);
    let poi2 = create_poi_proof(&proof2, 1000);
    let reward2 = calculate_poi_reward(&poi2).unwrap();
    
    // Checkpoint should give ~20% bonus on incremental rewards only
    assert!(reward2.value() > reward1.value());
    
    // Calculate incremental rewards (excluding base difficulty reward)
    let incremental1 = reward1.value() as f64 - base_reward as f64;
    let incremental2 = reward2.value() as f64 - base_reward as f64;
    
    // Verify the bonus is roughly 20% on incremental portion
    let bonus_ratio = (incremental2 / incremental1) - 1.0;
    assert!(bonus_ratio > 0.15 && bonus_ratio < 0.25, 
        "Checkpoint bonus on incremental reward should be ~20%, got {:.2}%", bonus_ratio * 100.0);
}

#[test]
fn test_calculate_reward_edge_cases() {
    // Test zero fragments - use valid proof for mining, fragments don't affect PoI validation
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
        0, // nonce
    );
    // Edge case: zero fragments should result in minimal reward but not panic
    // We can't use create_poi_proof with this invalid proof, so we verify the edge case directly
    let result = verify_distributed_task(&proof, None, None, None);
    assert_eq!(result.unwrap(), false); // Zero fragments returns false
    
    // Test max fragments (100) - use valid distributed proof for mining
    let task_id = Uuid::new_v4();
    let mut fragment_ids = Vec::new();
    for i in 0..100 {
        fragment_ids.push(format!("frag-{:03}", i));
    }
    let valid_proof = create_test_distributed_proof(task_id, 100, 0, 100, 1000, false);
    let poi = create_poi_proof(&valid_proof, 1000);
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
        0, // nonce
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
        0, // nonce
    );
    
    // Verify the proof with correct hashes
    let result = verify_distributed_task(&proof, None, None, None);
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
        0, // nonce
    );
    
    // Basic verification doesn't check hash correctness - just non-zero
    let result2 = verify_distributed_task(&proof2, None, None, None);
    assert!(result2.unwrap());
}

#[test]
fn test_reward_formula_components() {
    let task_id = Uuid::new_v4();
    
    // Base case: 1 fragment, 1 layer, valid compute time, no checkpoint
    let proof1 = create_test_distributed_proof(task_id, 1, 0, 1, 1000, false);
    let poi1 = create_poi_proof(&proof1, 1000);
    let reward1 = calculate_poi_reward(&poi1).unwrap();
    
    // Base + difficulty: 100 * 1 = 100 (from difficulty factor)
    assert!(reward1.value() >= 100);
    
    // Test compute_time_ms = 0 edge case - verify_distributed_task handles it
    let proof2 = DistributedTaskProof::new(
        task_id,
        Uuid::new_v4(),
        vec!["frag1".to_string()],
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        0, // Invalid: zero compute time
        "test-node".to_string(),
        0,
    );
    let result = verify_distributed_task(&proof2, None, None, None);
    assert_eq!(result.unwrap(), false); // Zero compute time returns false
}

#[test]
fn test_calculate_reward_rejects_invalid_work_hash() {
    // Create a valid distributed proof
    let task_id = uuid::Uuid::new_v4();
    let distributed_proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    // Create a valid PoI proof
    let mut poi = create_poi_proof(&distributed_proof, 1000);
    
    // Tamper with the work_hash by flipping some bytes
    poi.work_hash[0] ^= 0xFF;
    poi.work_hash[1] ^= 0xFF;
    
    // Verify that calculate_poi_reward rejects the tampered proof
    let result = calculate_poi_reward(&poi);
    assert!(
        result.is_err(),
        "calculate_poi_reward should reject a proof with an invalid work_hash"
    );
    assert!(
        matches!(result.unwrap_err(), ConsensusError::InvalidProof),
        "Expected InvalidProof error for tampered work_hash, but got a different error"
    );
}

#[test]
fn test_calculate_reward_rejects_invalid_difficulty() {
    // Create a valid distributed proof
    let task_id = uuid::Uuid::new_v4();
    let distributed_proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    let miner_address = distributed_proof.node_id.clone();
    let work_type = WorkType::DistributedInference;
    let input_hash = distributed_proof.input_activation_hash;
    let output_data = distributed_proof.output_activation_hash.to_vec();
    let metadata = ComputationMetadata {
        rows: 0,
        cols: 0,
        inner: 0,
        iterations: 0,
        model_id: "test-model".to_string(),
        compression_method: CompressionMethod::None,
        original_size: 1000000,
    };
    let timestamp = chrono::Utc::now().timestamp();
    
    // Use a fixed nonce of 0 that won't meet the difficulty requirement
    let nonce: u64 = 0;
    let difficulty: u64 = 10000; // High difficulty that nonce=0 won't satisfy
    
    // Compute work_hash with the invalid nonce
    let payload = poi_proof_work_payload_bytes(
        &miner_address,
        work_type,
        input_hash,
        &output_data,
        &metadata,
        difficulty,
        timestamp,
        nonce,
    );
    let work_hash_vec = blockchain_core::hash_data(&payload);
    let work_hash: [u8; 32] = work_hash_vec.try_into().expect("Hash should be 32 bytes");
    
    // Create verification data with the distributed proof
    let verification_data = serde_json::json!({
        "distributed_task_proof": distributed_proof
    });
    
    // Create a PoI proof with nonce=0 and high difficulty
    let poi = PoIProof::new(
        work_hash,
        miner_address,
        timestamp,
        verification_data,
        work_type,
        input_hash,
        output_data,
        metadata,
        difficulty,
        nonce,
    );
    
    // Verify that calculate_poi_reward rejects the proof due to difficulty validation failure
    let result = calculate_poi_reward(&poi);
    assert!(
        result.is_err(),
        "calculate_poi_reward should reject a proof where nonce doesn't meet difficulty requirement"
    );
    assert!(
        matches!(result.unwrap_err(), ConsensusError::InvalidProof),
        "Expected InvalidProof error for difficulty validation failure, but got a different error"
    );
}

#[test]
fn test_calculate_reward_rejects_mismatched_nonce() {
    // Create a valid distributed proof
    let task_id = uuid::Uuid::new_v4();
    let distributed_proof = create_test_distributed_proof(task_id, 3, 0, 10, 1000, false);
    
    // Create a valid PoI proof with a specific nonce (will be found by mining)
    let mut poi = create_poi_proof(&distributed_proof, 1000);
    let original_nonce = poi.nonce;
    
    // Extract the embedded DistributedTaskProof
    let proof_data = poi.verification_data.get("distributed_task_proof").unwrap();
    let mut distributed_proof: DistributedTaskProof = serde_json::from_value(proof_data.clone()).unwrap();
    
    // Modify the distributed proof's nonce to a different value
    distributed_proof.nonce = original_nonce + 99; // Use a different nonce
    
    // Re-embed the tampered distributed proof back into verification_data
    poi.verification_data["distributed_task_proof"] = serde_json::to_value(&distributed_proof).unwrap();
    
    // Verify that calculate_poi_reward rejects the proof due to nonce mismatch
    let result = calculate_poi_reward(&poi);
    assert!(
        result.is_err(),
        "calculate_poi_reward should reject a proof with mismatched nonce in distributed_task_proof"
    );
    assert!(
        matches!(result.unwrap_err(), ConsensusError::InvalidProof),
        "Expected InvalidProof error for nonce binding violation, but got a different error"
    );
}
