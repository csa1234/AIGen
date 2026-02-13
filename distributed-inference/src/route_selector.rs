// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Dynamic Route Selection Algorithm
//!
//! Selects optimal replica per block based on RTT and load:
//! - Calculates score: est_latency = avg_rtt + (load_score * 100ms)
//! - Selects replica with minimum estimated latency
//! - Builds ordered pipeline route across all blocks

use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;

use thiserror::Error;

use crate::replica_manager::{BlockReplica, ReplicaManager};

/// Error type for route selection
#[derive(Debug, Error)]
pub enum RouteSelectionError {
    #[error("no healthy replicas available")]
    NoHealthyReplicas,
    #[error("invalid block sequence")]
    InvalidBlockSequence,
    #[error("replica not found: {0}")]
    ReplicaNotFound(String),
}

/// Dynamic route selector for pipeline optimization
pub struct RouteSelector {
    /// Replica manager for health data
    replica_manager: Arc<ReplicaManager>,
    /// Enable dynamic routing (vs static assignment)
    enable_dynamic_routing: bool,
    /// Load penalty multiplier (ms per load unit)
    load_penalty_ms: f32,
}

impl RouteSelector {
    /// Create a new RouteSelector
    pub fn new(
        replica_manager: Arc<ReplicaManager>,
        enable_dynamic_routing: bool,
    ) -> Self {
        Self {
            replica_manager,
            enable_dynamic_routing,
            load_penalty_ms: 100.0, // 100ms per unit load
        }
    }

    /// Select optimal route for a pipeline inference
    ///
    /// Returns ordered list of replicas, one per block
    pub fn select_route(
        &self,
        block_ids: &[u32],
    ) -> Result<Vec<BlockReplica>, RouteSelectionError> {
        let mut route = Vec::new();
        let mut prev_replica: Option<&BlockReplica> = None;

        for block_id in block_ids {
            let replicas = self.replica_manager.get_healthy_replicas(*block_id);
            if replicas.is_empty() {
                return Err(RouteSelectionError::NoHealthyReplicas);
            }

            // Select best replica for this block
            let best_replica = self.select_best_replica(&replicas, prev_replica)
                .ok_or(RouteSelectionError::NoHealthyReplicas)?;

            route.push(best_replica);
            prev_replica = route.last();
        }

        // Validate the route
        if !self.validate_route(&route, block_ids) {
            return Err(RouteSelectionError::InvalidBlockSequence);
        }

        Ok(route)
    }

    /// Select best replica from a list based on RTT and load
    ///
    /// Uses scoring algorithm: score = avg_rtt + (load_score * load_penalty_ms)
    pub fn select_best_replica(
        &self,
        replicas: &[BlockReplica],
        prev_replica: Option<&BlockReplica>,
    ) -> Option<BlockReplica> {
        if replicas.is_empty() {
            return None;
        }

        if !self.enable_dynamic_routing {
            // Static: return first available
            return replicas.first().cloned();
        }

        replicas
            .iter()
            .min_by(|a, b| {
                let score_a = self.calculate_replica_score(a, prev_replica);
                let score_b = self.calculate_replica_score(b, prev_replica);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Calculate score for a replica (lower is better)
    ///
    /// Formula: est_latency = avg_rtt + (load_score * load_penalty_ms)
    fn calculate_replica_score(
        &self,
        replica: &BlockReplica,
        prev_replica: Option<&BlockReplica>,
    ) -> f32 {
        // Get average RTT
        let avg_rtt_ms = if replica.rtt_map.is_empty() {
            50.0 // Default 50ms if no RTT data
        } else {
            let sum: u64 = replica.rtt_map.values()
                .map(|d| d.as_millis() as u64)
                .sum();
            (sum as f32) / (replica.rtt_map.len() as f32)
        };

        // If we have a previous replica, add RTT between them
        let inter_replica_rtt = if let Some(prev) = prev_replica {
            prev.rtt_map
                .get(&replica.node_id)
                .map(|d| d.as_millis() as f32)
                .unwrap_or(10.0) // Default 10ms between replicas
        } else {
            0.0
        };

        // Load penalty (queue time estimate)
        let load_penalty = replica.load_score * self.load_penalty_ms;

        // Capability bonus (prefer GPU nodes)
        let capability_bonus = if replica.capabilities.has_gpu {
            -20.0 // 20ms bonus for GPU nodes
        } else {
            0.0
        };

        avg_rtt_ms + inter_replica_rtt + load_penalty + capability_bonus
    }

    /// Optimize pipeline order to minimize total RTT
    ///
    /// Uses greedy TSP-like optimization
    pub fn optimize_pipeline_order(&self, route: &mut [BlockReplica]) {
        if route.len() <= 2 {
            return; // No optimization needed
        }

        // Greedy optimization: try to minimize RTT between consecutive replicas
        for i in 0..route.len() - 2 {
            let current = &route[i];
            
            // Find best next replica from remaining
            let best_next_idx = (i + 1..route.len())
                .min_by(|&a, &b| {
                    let rtt_a = current.rtt_map
                        .get(&route[a].node_id)
                        .map(|d| d.as_millis() as f32)
                        .unwrap_or(f32::MAX);
                    let rtt_b = current.rtt_map
                        .get(&route[b].node_id)
                        .map(|d| d.as_millis() as f32)
                        .unwrap_or(f32::MAX);
                    rtt_a.partial_cmp(&rtt_b).unwrap_or(std::cmp::Ordering::Equal)
                });

            // Swap if better option found
            if let Some(best_idx) = best_next_idx {
                if best_idx != i + 1 {
                    route.swap(i + 1, best_idx);
                }
            }
        }
    }

    /// Validate that a route covers all required blocks
    fn validate_route(&self, route: &[BlockReplica], expected_block_ids: &[u32]) -> bool {
        if route.len() != expected_block_ids.len() {
            return false;
        }

        // Check all blocks are covered in order
        for (i, (replica, expected_id)) in route.iter().zip(expected_block_ids.iter()).enumerate() {
            if replica.block_id != *expected_id {
                tracing::warn!(
                    "Route validation failed: position {} expected block {} but got {}",
                    i, expected_id, replica.block_id
                );
                return false;
            }
        }

        // Check for duplicate replicas
        let mut seen_nodes = HashMap::new();
        for (i, replica) in route.iter().enumerate() {
            if let Some(prev_idx) = seen_nodes.insert(replica.node_id.clone(), i) {
                tracing::warn!(
                    "Route validation failed: node {} appears at positions {} and {}",
                    replica.node_id, prev_idx, i
                );
                return false;
            }
        }

        true
    }

    /// Estimate total latency for a route
    pub fn estimate_route_latency(&self, route: &[BlockReplica]) -> Duration {
        let mut total_ms: u64 = 0;
        let mut prev_replica: Option<&BlockReplica> = None;

        for replica in route {
            // Add processing time estimate based on load
            total_ms += (replica.load_score * 50.0) as u64; // 0-50ms processing time

            // Add RTT from previous replica
            if let Some(prev) = prev_replica {
                let rtt = prev.rtt_map
                    .get(&replica.node_id)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(10);
                total_ms += rtt;
            }

            prev_replica = Some(replica);
        }

        Duration::from_millis(total_ms)
    }

    /// Update load penalty multiplier
    pub fn set_load_penalty(&mut self, penalty_ms: f32) {
        self.load_penalty_ms = penalty_ms;
    }

    /// Enable or disable dynamic routing
    pub fn set_dynamic_routing(&mut self, enabled: bool) {
        self.enable_dynamic_routing = enabled;
    }

    /// Get routing statistics
    pub fn get_stats(&self) -> RouteStats {
        let total_replicas = self.replica_manager.get_all_replicas().len();
        let healthy_replicas = self.replica_manager.healthy_replica_count();
        
        // Calculate average RTT across all replicas
        let mut total_rtt_ms: f64 = 0.0;
        let mut rtt_count: usize = 0;
        
        for replica in self.replica_manager.get_all_replicas() {
            for rtt in replica.rtt_map.values() {
                total_rtt_ms += rtt.as_millis() as f64;
                rtt_count += 1;
            }
        }
        
        let avg_rtt_ms = if rtt_count > 0 {
            total_rtt_ms / rtt_count as f64
        } else {
            0.0
        };

        RouteStats {
            total_replicas,
            healthy_replicas,
            dead_replicas: total_replicas - healthy_replicas,
            avg_rtt_ms,
            dynamic_routing_enabled: self.enable_dynamic_routing,
            load_penalty_ms: self.load_penalty_ms,
        }
    }
}

/// Route selection statistics
#[derive(Clone, Debug)]
pub struct RouteStats {
    pub total_replicas: usize,
    pub healthy_replicas: usize,
    pub dead_replicas: usize,
    pub avg_rtt_ms: f64,
    pub dynamic_routing_enabled: bool,
    pub load_penalty_ms: f32,
}

/// Extended route selector that integrates with distributed-compute routing
///
/// This wraps the base RouteSelector and adds DCS integration
pub struct DcsIntegratedRouteSelector {
    base_selector: RouteSelector,
    dcs_state: Option<Arc<distributed_compute::state::GlobalState>>,
}

impl DcsIntegratedRouteSelector {
    /// Create with DCS global state integration
    pub fn new(
        replica_manager: Arc<ReplicaManager>,
        dcs_state: Option<Arc<distributed_compute::state::GlobalState>>,
        enable_dynamic_routing: bool,
    ) -> Self {
        Self {
            base_selector: RouteSelector::new(replica_manager, enable_dynamic_routing),
            dcs_state,
        }
    }

    /// Select route with DCS integration
    pub fn select_route_with_dcs(
        &self,
        block_ids: &[u32],
    ) -> Result<Vec<BlockReplica>, RouteSelectionError> {
        // Update RTT data from DCS state if available
        if let Some(dcs) = &self.dcs_state {
            self.update_rtt_from_dcs(dcs);
        }

        // Use base selector
        self.base_selector.select_route(block_ids)
    }

    /// Update RTT map from DCS global state
    fn update_rtt_from_dcs(&self, dcs_state: &distributed_compute::state::GlobalState) {
        for entry in dcs_state.nodes.iter() {
            let node = entry.value();
            let node_id = entry.key().clone();

            // Update RTT for this node
            for (peer_id, rtt) in &node.rtt_map {
                self.base_selector.replica_manager.update_rtt(&node_id, peer_id, *rtt);
            }
        }
    }

    /// Delegate to base selector
    pub fn select_best_replica(
        &self,
        replicas: &[BlockReplica],
        prev_replica: Option<&BlockReplica>,
    ) -> Option<BlockReplica> {
        self.base_selector.select_best_replica(replicas, prev_replica)
    }
}
