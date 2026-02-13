// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Replica Manager for Block Replica Tracking
//!
//! Tracks 3 replicas per block with health status, supporting concurrent access
//! via DashMap for high-performance operations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};

use network::protocol::NodeCapabilities;

/// Health status of a replica
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    /// Replica is healthy and available
    Healthy,
    /// Replica is experiencing issues but still functional
    Degraded,
    /// Replica is unresponsive and should not be used
    Dead,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Dead => write!(f, "dead"),
        }
    }
}

/// Replica information for a specific block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockReplica {
    pub block_id: u32,
    pub node_id: String,
    #[serde(with = "serde_bytes")]
    pub peer_id: PeerId,
    pub capabilities: NodeCapabilities,
    pub last_heartbeat: i64,
    pub load_score: f32,
    pub rtt_map: HashMap<String, Duration>,
}

/// Health information for a replica
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplicaHealth {
    pub node_id: String,
    pub status: HealthStatus,
    pub consecutive_misses: u32,
    pub last_seen: i64,
    pub current_load_score: f32,
    pub active_task_count: usize,
}

/// Manages block replicas with concurrent access
pub struct ReplicaManager {
    /// Block ID -> Vec<BlockReplica> mapping
    block_replicas: DashMap<u32, Vec<BlockReplica>>,
    /// Node ID -> ReplicaHealth mapping
    replica_health: DashMap<String, ReplicaHealth>,
    /// Node ID -> Block ID reverse mapping for quick lookup
    node_to_block: DashMap<String, u32>,
}

impl ReplicaManager {
    /// Create a new ReplicaManager
    pub fn new() -> Self {
        Self {
            block_replicas: DashMap::new(),
            replica_health: DashMap::new(),
            node_to_block: DashMap::new(),
        }
    }

    /// Add a replica for a specific block
    pub fn add_replica(&self, block_id: u32, replica: BlockReplica) {
        let node_id = replica.node_id.clone();
        
        // Insert into block replicas
        self.block_replicas
            .entry(block_id)
            .or_insert_with(Vec::new)
            .push(replica);

        // Initialize health record
        let health = ReplicaHealth {
            node_id: node_id.clone(),
            status: HealthStatus::Healthy,
            consecutive_misses: 0,
            last_seen: chrono::Utc::now().timestamp(),
            current_load_score: 0.0,
            active_task_count: 0,
        };
        
        self.replica_health.insert(node_id.clone(), health);
        self.node_to_block.insert(node_id, block_id);
    }

    /// Remove a replica for a specific block
    pub fn remove_replica(&self, block_id: u32, node_id: &str) {
        // Remove from block replicas
        if let Some(mut replicas) = self.block_replicas.get_mut(&block_id) {
            replicas.retain(|r| r.node_id != node_id);
        }

        // Update health status to Dead
        if let Some(mut health) = self.replica_health.get_mut(node_id) {
            health.status = HealthStatus::Dead;
        }

        // Remove reverse mapping
        self.node_to_block.remove(node_id);
    }

    /// Get all replicas for a block
    pub fn get_replicas(&self, block_id: u32) -> Vec<BlockReplica> {
        self.block_replicas
            .get(&block_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Get healthy replicas for a block (filters out Dead replicas)
    pub fn get_healthy_replicas(&self, block_id: u32) -> Vec<BlockReplica> {
        self.block_replicas
            .get(&block_id)
            .map(|replicas| {
                replicas
                    .iter()
                    .filter(|r| {
                        self.replica_health
                            .get(&r.node_id)
                            .map(|h| h.status == HealthStatus::Healthy)
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all replicas across all blocks
    pub fn get_all_replicas(&self) -> Vec<BlockReplica> {
        let mut all_replicas = Vec::new();
        for entry in self.block_replicas.iter() {
            all_replicas.extend(entry.value().iter().cloned());
        }
        all_replicas
    }

    /// Get health status for a specific replica
    pub fn get_replica_health(&self, node_id: &str) -> Option<ReplicaHealth> {
        self.replica_health.get(node_id).map(|h| h.clone())
    }

    /// Get all health statuses
    pub fn get_all_health_status(&self) -> Vec<ReplicaHealth> {
        self.replica_health
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Update replica health from heartbeat data
    pub fn update_replica_health(
        &self,
        node_id: &str,
        timestamp: i64,
        _active_tasks: Vec<Uuid>,
        load_score: f32,
    ) {
        if let Some(mut health) = self.replica_health.get_mut(node_id) {
            health.last_seen = timestamp;
            health.consecutive_misses = 0;
            health.current_load_score = load_score;
            
            // Update status based on load score
            if load_score > 0.9 {
                health.status = HealthStatus::Degraded;
            } else {
                health.status = HealthStatus::Healthy;
            }
        }

        // Also update the replica's last_heartbeat and load_score
        if let Some(block_id) = self.node_to_block.get(node_id) {
            if let Some(mut replicas) = self.block_replicas.get_mut(&*block_id) {
                if let Some(replica) = replicas.iter_mut().find(|r| r.node_id == node_id) {
                    replica.last_heartbeat = timestamp;
                    replica.load_score = load_score;
                }
            }
        }
    }

    /// Record a missed heartbeat for a node
    pub fn record_missed_heartbeat(&self, node_id: &str, failure_threshold: u32) -> Option<HealthStatus> {
        if let Some(mut health) = self.replica_health.get_mut(node_id) {
            health.consecutive_misses += 1;
            
            if health.consecutive_misses >= failure_threshold {
                health.status = HealthStatus::Dead;
                return Some(HealthStatus::Dead);
            } else if health.consecutive_misses >= failure_threshold / 2 {
                health.status = HealthStatus::Degraded;
                return Some(HealthStatus::Degraded);
            }
        }
        None
    }

    /// Select best replica based on load score and RTT
    pub fn select_best_replica(
        &self,
        block_id: u32,
        criteria: Option<ReplicaSelectionCriteria>,
    ) -> Option<BlockReplica> {
        let healthy = self.get_healthy_replicas(block_id);
        if healthy.is_empty() {
            return None;
        }

        let criteria = criteria.unwrap_or_default();

        healthy
            .into_iter()
            .min_by(|a, b| {
                let score_a = self.calculate_replica_score(a, &criteria);
                let score_b = self.calculate_replica_score(b, &criteria);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Calculate score for a replica (lower is better)
    fn calculate_replica_score(
        &self,
        replica: &BlockReplica,
        criteria: &ReplicaSelectionCriteria,
    ) -> f32 {
        // Base score from load
        let load_score = replica.load_score * criteria.load_weight;

        // RTT component
        let avg_rtt_ms: f32 = if replica.rtt_map.is_empty() {
            50.0 // Default 50ms
        } else {
            let sum: u64 = replica.rtt_map.values()
                .map(|d| d.as_millis() as u64)
                .sum();
            (sum as f32) / (replica.rtt_map.len() as f32)
        };
        let rtt_score = (avg_rtt_ms / 100.0) * criteria.rtt_weight;

        // Capability score (prefer GPU nodes for inference)
        let capability_score = if replica.capabilities.has_gpu {
            0.0
        } else {
            0.5 * criteria.capability_weight
        };

        load_score + rtt_score + capability_score
    }

    /// Ensure replication factor is maintained for a block
    pub fn ensure_replication_factor(&self, block_id: u32, target_k: usize) {
        let current_count = self.get_healthy_replicas(block_id).len();
        
        if current_count < target_k {
            tracing::warn!(
                "Block {} has {} healthy replicas, below target K={}",
                block_id,
                current_count,
                target_k
            );
        }
    }

    /// Get block ID for a node
    pub fn get_block_for_node(&self, node_id: &str) -> Option<u32> {
        self.node_to_block.get(node_id).map(|b| *b)
    }

    /// Update RTT for a node pair
    pub fn update_rtt(&self, node_id: &str, peer_node_id: &str, rtt: Duration) {
        if let Some(block_id) = self.node_to_block.get(node_id) {
            if let Some(mut replicas) = self.block_replicas.get_mut(&*block_id) {
                if let Some(replica) = replicas.iter_mut().find(|r| r.node_id == node_id) {
                    replica.rtt_map.insert(peer_node_id.to_string(), rtt);
                }
            }
        }
    }

    /// Get count of healthy replicas
    pub fn healthy_replica_count(&self) -> usize {
        self.replica_health
            .iter()
            .filter(|entry| entry.value().status == HealthStatus::Healthy)
            .count()
    }

    /// Get count of replicas by block
    pub fn replica_count_by_block(&self) -> HashMap<u32, usize> {
        let mut counts = HashMap::new();
        for entry in self.block_replicas.iter() {
            counts.insert(*entry.key(), entry.value().len());
        }
        counts
    }
}

impl Default for ReplicaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Criteria for replica selection
#[derive(Clone, Debug)]
pub struct ReplicaSelectionCriteria {
    pub load_weight: f32,
    pub rtt_weight: f32,
    pub capability_weight: f32,
    pub prefer_gpu: bool,
}

impl Default for ReplicaSelectionCriteria {
    fn default() -> Self {
        Self {
            load_weight: 0.4,
            rtt_weight: 0.4,
            capability_weight: 0.2,
            prefer_gpu: true,
        }
    }
}

use uuid::Uuid;
