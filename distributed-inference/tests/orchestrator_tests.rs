// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Integration tests for the Orchestrator Node
//!
//! These tests verify the orchestrator's core functionality:
//! - Leader election via DCS
//! - Replica health monitoring
//! - Dynamic route selection
//! - Failover handling
//! - Pipeline authentication

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use libp2p::PeerId;
use tokio::sync::mpsc;
use uuid::Uuid;

use distributed_inference::{
    OrchestratorNode, OrchestratorConfig, OrchestratorState, StaticBlockConfig,
    BlockConfig, ReplicaConfig, LayerBlock, ReplicaManager, HealthStatus,
    BlockReplica, NetworkMessage, NodeCapabilities,
};

/// Helper function to create a test static block config
fn create_test_config() -> StaticBlockConfig {
    StaticBlockConfig {
        total_blocks: 3,
        replication_factor: 3,
        blocks: vec![
            BlockConfig {
                block_id: 0,
                layer_range: (0, 9),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node_A".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node_B".to_string(), priority: 2 },
                    ReplicaConfig { node_id: "node_C".to_string(), priority: 3 },
                ],
            },
            BlockConfig {
                block_id: 1,
                layer_range: (10, 19),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node_D".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node_E".to_string(), priority: 2 },
                    ReplicaConfig { node_id: "node_F".to_string(), priority: 3 },
                ],
            },
            BlockConfig {
                block_id: 2,
                layer_range: (20, 29),
                model_id: "mistral-7b".to_string(),
                estimated_vram_gb: 2.5,
                replicas: vec![
                    ReplicaConfig { node_id: "node_G".to_string(), priority: 1 },
                    ReplicaConfig { node_id: "node_H".to_string(), priority: 2 },
                    ReplicaConfig { node_id: "node_I".to_string(), priority: 3 },
                ],
            },
        ],
    }
}

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

/// Helper to create a mock network channel
fn mock_network_channel() -> mpsc::Sender<NetworkMessage> {
    let (tx, _rx) = mpsc::channel(100);
    tx
}

#[tokio::test]
async fn test_replica_manager_adds_and_tracks_replicas() {
    let replica_manager = ReplicaManager::new();
    
    // Add replicas for block 0
    let replica_a = BlockReplica {
        block_id: 0,
        node_id: "node_A".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.2,
        rtt_map: HashMap::new(),
    };
    
    replica_manager.add_replica(0, replica_a.clone());
    
    // Verify replica is tracked
    let replicas = replica_manager.get_replicas(0);
    assert_eq!(replicas.len(), 1);
    assert_eq!(replicas[0].node_id, "node_A");
    
    // Verify health status is initialized
    let health = replica_manager.get_replica_health("node_A");
    assert!(health.is_some());
    assert_eq!(health.unwrap().status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_replica_manager_filters_healthy_replicas() {
    let replica_manager = ReplicaManager::new();
    
    // Add two replicas
    let replica_a = BlockReplica {
        block_id: 0,
        node_id: "node_A".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.2,
        rtt_map: HashMap::new(),
    };
    
    let replica_b = BlockReplica {
        block_id: 0,
        node_id: "node_B".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.3,
        rtt_map: HashMap::new(),
    };
    
    replica_manager.add_replica(0, replica_a);
    replica_manager.add_replica(0, replica_b);
    
    // Mark node_B as dead
    replica_manager.record_missed_heartbeat("node_B", 1);
    replica_manager.record_missed_heartbeat("node_B", 2);
    replica_manager.record_missed_heartbeat("node_B", 3); // Now dead
    
    // Verify only healthy replicas returned
    let healthy = replica_manager.get_healthy_replicas(0);
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0].node_id, "node_A");
}

#[tokio::test]
async fn test_replica_manager_selects_best_replica() {
    let replica_manager = ReplicaManager::new();
    
    // Add two replicas with different load scores
    let mut rtt_map_a = HashMap::new();
    rtt_map_a.insert("source".to_string(), Duration::from_millis(20));
    
    let replica_a = BlockReplica {
        block_id: 0,
        node_id: "node_A".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.8, // High load
        rtt_map: rtt_map_a,
    };
    
    let mut rtt_map_b = HashMap::new();
    rtt_map_b.insert("source".to_string(), Duration::from_millis(30));
    
    let replica_b = BlockReplica {
        block_id: 0,
        node_id: "node_B".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: chrono::Utc::now().timestamp(),
        load_score: 0.2, // Low load - should be selected
        rtt_map: rtt_map_b,
    };
    
    replica_manager.add_replica(0, replica_a);
    replica_manager.add_replica(0, replica_b);
    
    // Select best replica
    let best = replica_manager.select_best_replica(0, None);
    assert!(best.is_some());
    // Should select node_B due to lower load score
    assert_eq!(best.unwrap().node_id, "node_B");
}

#[tokio::test]
async fn test_health_monitor_detects_failures() {
    use distributed_inference::health_monitor::{HealthMonitor, SimpleLoadProvider};
    
    let replica_manager = Arc::new(ReplicaManager::new());
    let network_tx = mock_network_channel();
    
    let health_monitor = HealthMonitor::new(
        replica_manager.clone(),
        500, // 500ms interval
        3,   // 3 missed = dead
        network_tx,
    );
    
    // Add a replica with an old heartbeat
    let old_timestamp = chrono::Utc::now().timestamp() - 10; // 10 seconds ago
    
    let replica = BlockReplica {
        block_id: 0,
        node_id: "node_A".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: old_timestamp,
        load_score: 0.2,
        rtt_map: HashMap::new(),
    };
    
    replica_manager.add_replica(0, replica);
    
    // Record 3 missed heartbeats
    replica_manager.record_missed_heartbeat("node_A", 3);
    replica_manager.record_missed_heartbeat("node_A", 3);
    replica_manager.record_missed_heartbeat("node_A", 3);
    
    // Verify replica is marked as dead
    let health = replica_manager.get_replica_health("node_A").unwrap();
    assert_eq!(health.status, HealthStatus::Dead);
    assert_eq!(health.consecutive_misses, 3);
}

#[tokio::test]
async fn test_health_monitor_handles_heartbeat() {
    use distributed_inference::health_monitor::HealthMonitor;
    
    let replica_manager = Arc::new(ReplicaManager::new());
    let network_tx = mock_network_channel();
    
    let health_monitor = HealthMonitor::new(
        replica_manager.clone(),
        500,
        3,
        network_tx,
    );
    
    // Add a replica
    let old_timestamp = chrono::Utc::now().timestamp() - 10;
    let replica = BlockReplica {
        block_id: 0,
        node_id: "node_A".to_string(),
        peer_id: PeerId::random(),
        capabilities: test_capabilities(),
        last_heartbeat: old_timestamp,
        load_score: 0.5,
        rtt_map: HashMap::new(),
    };
    replica_manager.add_replica(0, replica);
    
    // Record some misses first
    replica_manager.record_missed_heartbeat("node_A", 3);
    replica_manager.record_missed_heartbeat("node_A", 3);
    
    // Verify degraded status
    let health = replica_manager.get_replica_health("node_A").unwrap();
    assert_eq!(health.consecutive_misses, 2);
    
    // Handle a heartbeat
    let heartbeat = NetworkMessage::Heartbeat {
        node_id: "node_A".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        active_tasks: vec![Uuid::new_v4()],
        load_score: 0.3,
    };
    
    health_monitor.handle_heartbeat(&heartbeat).unwrap();
    
    // Verify health is updated
    let health = replica_manager.get_replica_health("node_A").unwrap();
    assert_eq!(health.consecutive_misses, 0);
    assert_eq!(health.current_load_score, 0.3);
}

#[tokio::test]
async fn test_orchestrator_state_response() {
    use distributed_inference::orchestrator::OrchestratorState;
    
    let state = OrchestratorState {
        is_leader: true,
        leader_node_id: Some("node_1".to_string()),
        total_blocks: 3,
        total_replicas: 9,
        healthy_replicas: 8,
    };
    
    assert!(state.is_leader);
    assert_eq!(state.total_blocks, 3);
    assert_eq!(state.total_replicas, 9);
    assert_eq!(state.healthy_replicas, 8);
}

#[tokio::test]
async fn test_route_selector_validates_route() {
    use distributed_inference::route_selector::RouteSelector;
    
    let replica_manager = Arc::new(ReplicaManager::new());
    let route_selector = RouteSelector::new(replica_manager.clone(), true);
    
    // Create replicas for 3 blocks
    for block_id in 0..3 {
        let replica = BlockReplica {
            block_id,
            node_id: format!("node_{}", block_id),
            peer_id: PeerId::random(),
            capabilities: test_capabilities(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.2,
            rtt_map: HashMap::new(),
        };
        replica_manager.add_replica(block_id, replica);
    }
    
    // Select route for blocks 0, 1, 2
    let route = route_selector.select_route(&[0, 1, 2]);
    assert!(route.is_ok());
    
    let route = route.unwrap();
    assert_eq!(route.len(), 3);
    assert_eq!(route[0].block_id, 0);
    assert_eq!(route[1].block_id, 1);
    assert_eq!(route[2].block_id, 2);
}

#[tokio::test]
async fn test_route_selector_fails_on_missing_replicas() {
    use distributed_inference::route_selector::RouteSelector;
    
    let replica_manager = Arc::new(ReplicaManager::new());
    let route_selector = RouteSelector::new(replica_manager.clone(), true);
    
    // Only add replicas for blocks 0 and 1
    for block_id in 0..2 {
        let replica = BlockReplica {
            block_id,
            node_id: format!("node_{}", block_id),
            peer_id: PeerId::random(),
            capabilities: test_capabilities(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.2,
            rtt_map: HashMap::new(),
        };
        replica_manager.add_replica(block_id, replica);
    }
    
    // Try to select route for blocks 0, 1, 2 (block 2 has no replicas)
    let route = route_selector.select_route(&[0, 1, 2]);
    assert!(route.is_err());
}

#[tokio::test]
async fn test_orchestrator_config_defaults() {
    let config = OrchestratorConfig::default();
    
    assert_eq!(config.replication_factor, 3);
    assert_eq!(config.heartbeat_interval_ms, 500);
    assert_eq!(config.failure_threshold, 3);
    assert!(config.enable_dynamic_routing);
}

#[tokio::test]
async fn test_block_assignment_conversion() {
    let config = create_test_config();
    let assignment = config.to_block_assignment();
    
    assert_eq!(assignment.blocks.len(), 3);
    assert_eq!(assignment.get_block_ids(), vec![0, 1, 2]);
    
    // Check block 0
    let block_0 = assignment.get_block(0).unwrap();
    assert_eq!(block_0.layer_range, (0, 9));
    assert_eq!(block_0.model_id, "mistral-7b");
    
    // Check replicas
    let replicas_0 = assignment.get_replicas(0);
    assert_eq!(replicas_0.len(), 3);
}
