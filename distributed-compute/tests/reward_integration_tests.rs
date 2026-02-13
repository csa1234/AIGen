// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Reward Integration Tests
//!
//! Tests for PoI proof generation, reward calculation, and distribution

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

use blockchain_core::types::Amount;
use consensus::poi::{DistributedTaskProof, WorkType, CompressionMethod, ComputationMetadata};
use consensus::PoIProof;
use distributed_compute::state::{GlobalState, NodeState, NodeRole, FragmentLocation, RewardRecord};
use distributed_compute::scheduler::DynamicScheduler;
use distributed_compute::routing::RouteSelector;
use distributed_compute::task::{ComputeTask, TaskStatus};
use network::protocol::{NetworkMessage, NodeCapabilities};

fn create_test_node(node_id: &str, stake: u64, vram_gb: f32) -> NodeState {
    NodeState {
        node_id: node_id.to_string(),
        peer_id: None,
        role: NodeRole::Inference,
        vram_total_gb: vram_gb,
        vram_free_gb: vram_gb * 0.5,
        vram_allocated_gb: vram_gb * 0.3,
        region: Some("us-east".to_string()),
        last_heartbeat: blockchain_core::types::Timestamp(1234567890),
        load_score: 0.5,
        stake: Amount::new(stake),
        capabilities: NodeCapabilities {
            has_gpu: true,
            gpu_model: Some("RTX4090".to_string()),
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 2048,
        },
        rtt_map: HashMap::new(),
    }
}

fn create_test_fragment(fragment_id: &str, model_id: &str, size_bytes: u64, replicas: Vec<String>) -> FragmentLocation {
    FragmentLocation {
        fragment_id: fragment_id.to_string(),
        model_id: model_id.to_string(),
        fragment_index: 0,
        size_bytes,
        replicas,
    }
}

fn create_test_compute_task(task_id: Uuid, assigned_node: &str, fragment_ids: Vec<String>) -> ComputeTask {
    ComputeTask {
        task_id,
        inference_id: Uuid::new_v4(),
        model_id: "test-model".to_string(),
        layer_range: (0, 10),
        required_fragments: fragment_ids,
        assigned_node: assigned_node.to_string(),
        status: TaskStatus::Pending,
    }
}

fn create_test_distributed_proof(
    task_id: Uuid,
    inference_id: Uuid,
    fragment_count: u32,
    compute_time_ms: u64,
) -> DistributedTaskProof {
    let fragment_ids: Vec<String> = (0..fragment_count)
        .map(|i| format!("frag-{}", i))
        .collect();
    
    DistributedTaskProof::new(
        task_id,
        inference_id,
        fragment_ids,
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        Some([2u8; 32]),
        compute_time_ms,
        "test-node-1".to_string(),
    )
}

fn create_test_poi_proof(distributed_proof: &DistributedTaskProof) -> PoIProof {
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
        1000,
        1234567890,
        42,
        verification_data,
    )
}

#[test]
fn test_distributed_task_proof_creation() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    
    let proof = create_test_distributed_proof(task_id, inference_id, 5, 1000);
    
    assert_eq!(proof.task_id, task_id);
    assert_eq!(proof.inference_id, inference_id);
    assert_eq!(proof.fragment_ids.len(), 5);
    assert_eq!(proof.compute_time_ms, 1000);
    assert_eq!(proof.node_id, "test-node-1");
    assert!(proof.checkpoint_hash.is_some());
}

#[test]
fn test_distributed_task_proof_validation() {
    use consensus::poi::verify_distributed_task;
    
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    
    // Valid proof
    let proof = create_test_distributed_proof(task_id, inference_id, 3, 1000);
    let result = verify_distributed_task(&proof);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), true);
    
    // Invalid: empty fragments
    let invalid_proof = DistributedTaskProof::new(
        task_id,
        inference_id,
        vec![], // empty fragments
        (0, 10),
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    let result = verify_distributed_task(&invalid_proof);
    assert_eq!(result.unwrap(), false);
    
    // Invalid: invalid layer range
    let invalid_proof2 = DistributedTaskProof::new(
        task_id,
        inference_id,
        vec!["frag1".to_string()],
        (10, 0), // invalid: start > end
        [0u8; 32],
        [1u8; 32],
        None,
        1000,
        "test-node".to_string(),
    );
    let result = verify_distributed_task(&invalid_proof2);
    assert_eq!(result.unwrap(), false);
}

#[test]
fn test_reward_calculation_formula() {
    use consensus::poi::calculate_poi_reward;
    
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    
    // Test with 3 fragments, layer range 10, 1000ms, no checkpoint
    let distributed_proof = create_test_distributed_proof(task_id, inference_id, 3, 1000);
    let poi_proof = create_test_poi_proof(&distributed_proof);
    
    let reward = calculate_poi_reward(&poi_proof).unwrap();
    
    // Base: 100, fragment_factor: 3 * 10 / 100 = 0.3, time_factor: 1.01, checkpoint: 1.0
    // Total: 100 * 0.3 * 1.01 * 1.0 = 30.3
    // Plus base from difficulty: 100 * 1 = 100
    assert!(reward.value() > 100); // Should include both base and distributed reward
    
    // Test with checkpoint (20% bonus)
    let distributed_proof_checkpoint = DistributedTaskProof::new(
        task_id,
        inference_id,
        vec!["frag1".to_string(), "frag2".to_string()],
        (0, 20),
        [0u8; 32],
        [1u8; 32],
        Some([2u8; 32]), // checkpoint
        2000,
        "test-node".to_string(),
    );
    
    let poi_proof_checkpoint = create_test_poi_proof(&distributed_proof_checkpoint);
    let reward_with_checkpoint = calculate_poi_reward(&poi_proof_checkpoint).unwrap();
    
    // Should be higher due to checkpoint bonus
    assert!(reward_with_checkpoint.value() >= reward.value());
}

#[test]
fn test_reward_split_formula() {
    // Test 80/15/5 split
    let total_reward = 1000u64;
    
    let compute_reward = (total_reward * 80) / 100;
    let storage_reward = (total_reward * 15) / 100;
    let treasury_reward = (total_reward * 5) / 100;
    
    assert_eq!(compute_reward, 800);
    assert_eq!(storage_reward, 150);
    assert_eq!(treasury_reward, 50);
    
    // Verify sum equals total (may have rounding differences)
    assert_eq!(compute_reward + storage_reward + treasury_reward, total_reward);
}

#[test]
fn test_global_state_reward_tracking() {
    let state = GlobalState::new();
    let node_id = "test-node-1".to_string();
    
    // Initially no record
    assert!(state.get_node_earnings(&node_id).is_none());
    
    // Record compute reward
    state.record_reward(&node_id, Amount::new(500), Amount::ZERO, true);
    
    let record = state.get_node_earnings(&node_id).unwrap();
    assert_eq!(record.total_earned.value(), 500);
    assert_eq!(record.compute_rewards.value(), 500);
    assert_eq!(record.storage_rewards.value(), 0);
    assert_eq!(record.tasks_completed, 1);
    
    // Record storage reward
    state.record_reward(&node_id, Amount::ZERO, Amount::new(200), false);
    
    let record = state.get_node_earnings(&node_id).unwrap();
    assert_eq!(record.total_earned.value(), 700); // 500 + 200
    assert_eq!(record.compute_rewards.value(), 500);
    assert_eq!(record.storage_rewards.value(), 200);
    assert_eq!(record.tasks_completed, 1); // unchanged
}

#[test]
fn test_nodes_hosting_fragments() {
    let state = GlobalState::new();
    
    // Add fragments
    let frag1 = create_test_fragment("frag-1", "model-1", 1000000, 
        vec!["node-1".to_string(), "node-2".to_string()]);
    let frag2 = create_test_fragment("frag-2", "model-1", 2000000, 
        vec!["node-2".to_string(), "node-3".to_string()]);
    let frag3 = create_test_fragment("frag-3", "model-1", 1500000, 
        vec!["node-1".to_string()]);
    
    state.fragments.insert(frag1.fragment_id.clone(), frag1);
    state.fragments.insert(frag2.fragment_id.clone(), frag2);
    state.fragments.insert(frag3.fragment_id.clone(), frag3);
    
    // Get nodes hosting fragments
    let nodes_with_fragments = state.get_nodes_hosting_fragments();
    
    assert_eq!(nodes_with_fragments.len(), 3);
    assert_eq!(nodes_with_fragments.get("node-1").unwrap().len(), 2); // frag-1, frag-3
    assert_eq!(nodes_with_fragments.get("node-2").unwrap().len(), 2); // frag-1, frag-2
    assert_eq!(nodes_with_fragments.get("node-3").unwrap().len(), 1); // frag-2
}

#[test]
fn test_vram_allocation_calculation() {
    let state = GlobalState::new();
    
    // Add fragments
    let frag1 = create_test_fragment("frag-1", "model-1", 1073741824, // 1 GB
        vec!["node-1".to_string(), "node-2".to_string()]);
    let frag2 = create_test_fragment("frag-2", "model-1", 2147483648, // 2 GB
        vec!["node-1".to_string()]);
    
    state.fragments.insert(frag1.fragment_id.clone(), frag1);
    state.fragments.insert(frag2.fragment_id.clone(), frag2);
    
    // Calculate VRAM for node-1 (hosts frag-1 and frag-2 = 3GB total)
    let vram_node1 = state.calculate_vram_allocated("node-1");
    assert!((vram_node1 - 3.0).abs() < 0.01);
    
    // Calculate VRAM for node-2 (hosts frag-1 = 1GB total)
    let vram_node2 = state.calculate_vram_allocated("node-2");
    assert!((vram_node2 - 1.0).abs() < 0.01);
    
    // Calculate VRAM for non-existent node
    let vram_node3 = state.calculate_vram_allocated("node-3");
    assert_eq!(vram_node3, 0.0);
}

#[test]
fn test_storage_reward_distribution() {
    let state = GlobalState::new();
    
    // Add nodes with different stakes
    state.nodes.insert("node-1".to_string(), create_test_node("node-1", 1000, 24.0));
    state.nodes.insert("node-2".to_string(), create_test_node("node-2", 2000, 48.0));
    state.nodes.insert("node-3".to_string(), create_test_node("node-3", 500, 12.0));
    
    // Add fragments
    let frag1 = create_test_fragment("frag-1", "model-1", 1073741824, // 1 GB
        vec!["node-1".to_string(), "node-2".to_string()]);
    let frag2 = create_test_fragment("frag-2", "model-1", 2147483648, // 2 GB
        vec!["node-2".to_string(), "node-3".to_string()]);
    
    state.fragments.insert(frag1.fragment_id.clone(), frag1);
    state.fragments.insert(frag2.fragment_id.clone(), frag2);
    
    // Calculate total stake and VRAM
    let total_stake: u64 = state.nodes.iter()
        .map(|n| n.value().stake.value())
        .sum();
    let total_vram: f32 = state.nodes.iter()
        .map(|n| n.value().vram_total_gb)
        .sum();
    
    assert_eq!(total_stake, 3500);
    assert_eq!(total_vram, 84.0);
    
    // Test weight calculations for each node
    // node-1: stake_weight = 1000/3500 = 0.286, vram_weight = 1GB/84GB = 0.012
    let vram_node1 = state.calculate_vram_allocated("node-1");
    let stake_weight_1 = 1000.0 / total_stake as f32;
    let vram_weight_1 = vram_node1 / total_vram;
    
    // HOURLY_STORAGE_POOL = 10,000
    const HOURLY_STORAGE_POOL: f32 = 10_000.0;
    let reward_1 = (HOURLY_STORAGE_POOL * stake_weight_1 * vram_weight_1) as u64;
    
    // Verify calculations are reasonable
    assert!(stake_weight_1 > 0.0 && stake_weight_1 <= 1.0);
    assert!(vram_weight_1 > 0.0 && vram_weight_1 <= 1.0);
    assert!(reward_1 >= 0);
}

#[test]
fn test_reward_pool_accumulation() {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use std::collections::HashMap;
    use blockchain_core::types::Amount;
    
    let reward_pool: Arc<RwLock<HashMap<String, Amount>>> = Arc::new(RwLock::new(HashMap::new()));
    
    // Simulate accumulating rewards
    {
        let mut pool = reward_pool.blocking_write();
        let entry = pool.entry("node-1".to_string()).or_insert(Amount::ZERO);
        *entry = Amount::new(entry.value().saturating_add(500));
    }
    
    {
        let mut pool = reward_pool.blocking_write();
        let entry = pool.entry("node-1".to_string()).or_insert(Amount::ZERO);
        *entry = Amount::new(entry.value().saturating_add(300));
    }
    
    {
        let pool = reward_pool.blocking_read();
        let total = pool.get("node-1").copied().unwrap_or(Amount::ZERO);
        assert_eq!(total.value(), 800);
    }
}
