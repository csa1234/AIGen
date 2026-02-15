// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Pipeline integration tests for distributed compute scheduler

use distributed_compute::state::{GlobalState, NodeState, NodeRole, FragmentLocation};
use distributed_compute::task::{ComputeTask, TaskStatus, TaskPlan, ActivationRef, InferenceTask};
use distributed_compute::routing::RouteSelector;
use network::protocol::NodeCapabilities;
use blockchain_core::types::Timestamp;
use uuid::Uuid;

fn create_test_node(node_id: &str, vram_gb: f32) -> NodeState {
    NodeState {
        node_id: node_id.to_string(),
        peer_id: None,
        role: NodeRole::Inference,
        vram_total_gb: vram_gb,
        vram_free_gb: vram_gb * 0.8,
        vram_allocated_gb: vram_gb * 0.2,
        region: Some("us-east".to_string()),
        last_heartbeat: Timestamp(0),
        load_score: 0.0,
        stake: blockchain_core::types::Amount::ZERO,
        capabilities: NodeCapabilities {
            has_gpu: true,
            gpu_model: Some("RTX4090".to_string()),
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 500,
        },
        rtt_map: std::collections::HashMap::new(),
        bid_price_per_task: None,
        accepts_bids: false,
    }
}

#[test]
fn test_task_plan_creation() {
    let inference_id = Uuid::new_v4();
    let task = ComputeTask {
        task_id: Uuid::new_v4(),
        inference_id,
        model_id: "test_model".to_string(),
        layer_range: (0, 10),
        required_fragments: vec!["frag_0".to_string()],
        input_activation_ref: None,
        assigned_node: String::new(),
        status: TaskStatus::Pending,
        tensor_shard_index: 0,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };

    let plan = TaskPlan {
        inference_id,
        tasks: vec![task.clone()],
        pipeline_order: vec![],
    };

    assert_eq!(plan.inference_id, inference_id);
    assert_eq!(plan.tasks.len(), 1);
    assert_eq!(plan.tasks[0].layer_range, (0, 10));
}

#[test]
fn test_activation_ref_creation() {
    let activation_ref = ActivationRef {
        location: "node_1/task_123".to_string(),
        size_bytes: 1024 * 1024 * 10, // 10MB
    };

    assert_eq!(activation_ref.location, "node_1/task_123");
    assert_eq!(activation_ref.size_bytes, 10485760);
}

#[test]
fn test_global_state_node_tracking() {
    let state = GlobalState::new(std::sync::Arc::new(blockchain_core::state::ChainState::new()));
    
    let node = create_test_node("node_1", 24.0);
    state.update_node_state(node.clone());
    
    assert!(state.nodes.contains_key("node_1"));
    
    if let Some(n) = state.nodes.get("node_1") {
        assert_eq!(n.node_id, "node_1");
        assert_eq!(n.vram_total_gb, 24.0);
    };

    // Ensure the Ref is dropped before state goes out of scope
}

#[test]
fn test_global_state_fragment_tracking() {
    let state = GlobalState::new(std::sync::Arc::new(blockchain_core::state::ChainState::new()));
    
    let fragment = FragmentLocation {
        fragment_id: "frag_0".to_string(),
        model_id: "test_model".to_string(),
        fragment_index: 0,
        size_bytes: 200 * 1024 * 1024, // 200MB
        replicas: vec!["node_1".to_string(), "node_2".to_string()],
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    state.fragments.insert("frag_0".to_string(), fragment);
    
    assert!(state.fragments.contains_key("frag_0"));
}

#[test]
fn test_task_allocation() {
    let state = GlobalState::new(std::sync::Arc::new(blockchain_core::state::ChainState::new()));
    
    let node = create_test_node("node_1", 24.0);
    state.update_node_state(node);
    
    let task = ComputeTask {
        task_id: Uuid::new_v4(),
        inference_id: Uuid::new_v4(),
        model_id: "test_model".to_string(),
        layer_range: (0, 10),
        required_fragments: vec!["frag_0".to_string()],
        input_activation_ref: None,
        assigned_node: String::new(),
        status: TaskStatus::Pending,
        tensor_shard_index: 0,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    state.allocate_task(&task, "node_1");
    
    // Verify task is tracked
    assert!(state.active_tasks.contains_key(&task.task_id));
    
    // Verify node load score increased
    if let Some(n) = state.nodes.get("node_1") {
        assert!(n.load_score > 0.0);
    };

    // Ensure the Ref is dropped before state goes out of scope
}

#[test]
fn test_vram_tracking() {
    let state = GlobalState::new(std::sync::Arc::new(blockchain_core::state::ChainState::new()));
    
    let node = create_test_node("node_1", 24.0);
    state.update_node_state(node);
    
    // Simulate VRAM allocation
    if let Some(mut n) = state.nodes.get_mut("node_1") {
        n.vram_allocated_gb += 2.0;
        n.vram_free_gb -= 2.0;
    };
    
    if let Some(n) = state.nodes.get("node_1") {
        assert!(n.vram_allocated_gb > 22.0);
        assert!(n.vram_free_gb < 24.0);
    };

    // Ensure the Ref is dropped before state goes out of scope
}

#[test]
fn test_pipeline_activation_linking() {
    let state = GlobalState::new(std::sync::Arc::new(blockchain_core::state::ChainState::new()));
    let _router = RouteSelector::new(std::sync::Arc::new(state));
    
    // Create tasks for a pipeline
    let inference_id = Uuid::new_v4();
    let task = ComputeTask {
        task_id: Uuid::new_v4(),
        inference_id,
        model_id: "test_model".to_string(),
        layer_range: (0, 10),
        required_fragments: vec!["frag_0".to_string()],
        input_activation_ref: None,
        assigned_node: "node_1".to_string(),
        status: TaskStatus::Pending,
        tensor_shard_index: 0,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    let task2_id = Uuid::new_v4();
    let task2 = ComputeTask {
        task_id: task2_id,
        inference_id,
        model_id: "test_model".to_string(),
        layer_range: (11, 20),
        required_fragments: vec!["frag_1".to_string()],
        input_activation_ref: Some(ActivationRef {
            location: format!("node_1/{}", task.task_id),
            size_bytes: 1024 * 1024 * 10,
        }),
        assigned_node: "node_2".to_string(),
        status: TaskStatus::Pending,
        tensor_shard_index: 1,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    let plan = TaskPlan {
        inference_id,
        tasks: vec![task, task2],
        pipeline_order: vec!["node_1".to_string(), "node_2".to_string()],
    };
    
    // Verify pipeline order
    assert_eq!(plan.pipeline_order.len(), 2);
    
    // Verify task2 has activation reference from task1
    assert!(plan.tasks[1].input_activation_ref.is_some());
    let ref_loc = plan.tasks[1].input_activation_ref.as_ref().unwrap();
    assert!(ref_loc.location.contains(&plan.tasks[0].task_id.to_string()));
}

#[test]
fn test_task_status_transitions() {
    let task = ComputeTask {
        task_id: Uuid::new_v4(),
        inference_id: Uuid::new_v4(),
        model_id: "test_model".to_string(),
        layer_range: (0, 10),
        required_fragments: vec!["frag_0".to_string()],
        input_activation_ref: None,
        assigned_node: "node_1".to_string(),
        status: TaskStatus::Pending,
        tensor_shard_index: 0,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    assert_eq!(task.status, TaskStatus::Pending);
    
    // Clone and modify status
    let mut running_task = task.clone();
    running_task.status = TaskStatus::Running;
    assert_eq!(running_task.status, TaskStatus::Running);
    
    let mut completed_task = task.clone();
    completed_task.status = TaskStatus::Completed;
    assert_eq!(completed_task.status, TaskStatus::Completed);
    
    let mut failed_task = task.clone();
    failed_task.status = TaskStatus::Failed;
    assert_eq!(failed_task.status, TaskStatus::Failed);
}

#[test]
fn test_inference_task_creation() {
    let inference_id = Uuid::new_v4();
    let request = InferenceTask {
        inference_id,
        model_id: "gpt4".to_string(),
        prompt: "Hello world".to_string(),
        max_tokens: 100,
        user_id: "user_123".to_string(),
    };
    
    assert_eq!(request.model_id, "gpt4");
    assert_eq!(request.prompt, "Hello world");
    assert_eq!(request.max_tokens, 100);
    assert_eq!(request.user_id, "user_123");
}

#[test]
fn test_fragment_location_replicas() {
    let fragment = FragmentLocation {
        fragment_id: "frag_0".to_string(),
        model_id: "test_model".to_string(),
        fragment_index: 0,
        size_bytes: 200 * 1024 * 1024,
        replicas: vec!["node_1".to_string(), "node_2".to_string(), "node_3".to_string()],
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    assert_eq!(fragment.replicas.len(), 3);
    assert!(fragment.replicas.contains(&"node_1".to_string()));
    assert!(fragment.replicas.contains(&"node_2".to_string()));
    assert!(fragment.replicas.contains(&"node_3".to_string()));
}

#[test]
fn test_compute_task_clone() {
    let task = ComputeTask {
        task_id: Uuid::new_v4(),
        inference_id: Uuid::new_v4(),
        model_id: "test_model".to_string(),
        layer_range: (0, 10),
        required_fragments: vec!["frag_0".to_string()],
        input_activation_ref: None,
        assigned_node: "node_1".to_string(),
        status: TaskStatus::Pending,
        tensor_shard_index: 0,
        total_tensor_shards: 1,
        rejected_nodes: Vec::new(),
        target_fragment_size_bytes: distributed_compute::state::DEFAULT_FRAGMENT_SIZE_BYTES,
    };
    
    let cloned = task.clone();
    assert_eq!(cloned.task_id, task.task_id);
    assert_eq!(cloned.inference_id, task.inference_id);
    assert_eq!(cloned.model_id, task.model_id);
    assert_eq!(cloned.layer_range, task.layer_range);
}
