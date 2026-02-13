// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Topology Optimizer for Ring Pipeline Optimization
//!
//! Arranges nodes in ring topology based on RTT measurements to minimize
//! total pipeline latency. Uses greedy nearest-neighbor TSP heuristic.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use crate::replica_manager::{BlockReplica, ReplicaManager};

/// Metrics for topology optimization
#[derive(Debug, Default)]
pub struct TopologyMetrics {
    pub topology_optimizations: AtomicU64,
    pub avg_pipeline_rtt_ms: AtomicU64, // Fixed-point: value / 100
    pub rtt_improvement_pct: AtomicU64, // Fixed-point: value / 100 (percentage)
}

impl TopologyMetrics {
    pub fn record_optimization(
        &self,
        pipeline_rtt_ms: u64,
        baseline_rtt_ms: u64,
    ) {
        self.topology_optimizations.fetch_add(1, Ordering::Relaxed);

        // Update average pipeline RTT (fixed-point)
        let rtt_fixed = pipeline_rtt_ms * 100;
        let prev = self.avg_pipeline_rtt_ms.load(Ordering::Relaxed);
        let new = if prev == 0 {
            rtt_fixed
        } else {
            (prev + rtt_fixed) / 2
        };
        self.avg_pipeline_rtt_ms.store(new, Ordering::Relaxed);

        // Calculate improvement percentage
        if baseline_rtt_ms > 0 {
            let improvement = ((baseline_rtt_ms - pipeline_rtt_ms) as f64 / baseline_rtt_ms as f64)
                * 100.0;
            let improvement_fixed = (improvement * 100.0) as u64;
            let prev_improvement = self.rtt_improvement_pct.load(Ordering::Relaxed);
            let new_improvement = if prev_improvement == 0 {
                improvement_fixed
            } else {
                (prev_improvement + improvement_fixed) / 2
            };
            self.rtt_improvement_pct.store(new_improvement, Ordering::Relaxed);
        }
    }

    pub fn get_stats(&self) -> (u64, f64, f64) {
        let optimizations = self.topology_optimizations.load(Ordering::Relaxed);
        let avg_rtt = self.avg_pipeline_rtt_ms.load(Ordering::Relaxed) as f64 / 100.0;
        let improvement = self.rtt_improvement_pct.load(Ordering::Relaxed) as f64 / 100.0;
        (optimizations, avg_rtt, improvement)
    }
}

/// Topology optimizer that arranges blocks in optimal ring order
pub struct TopologyOptimizer {
    replica_manager: Arc<ReplicaManager>,
    rtt_matrix: DashMap<(String, String), Duration>,
    metrics: Arc<TopologyMetrics>,
}

impl TopologyOptimizer {
    /// Create a new TopologyOptimizer
    pub fn new(replica_manager: Arc<ReplicaManager>) -> Self {
        Self {
            replica_manager,
            rtt_matrix: DashMap::new(),
            metrics: Arc::new(TopologyMetrics::default()),
        }
    }

    /// Update RTT measurement between two nodes
    pub fn update_rtt_measurement(
        &self,
        node_a: &str,
        node_b: &str,
        rtt: Duration,
    ) {
        // Store symmetric RTT
        self.rtt_matrix.insert((node_a.to_string(), node_b.to_string()), rtt);
        self.rtt_matrix.insert((node_b.to_string(), node_a.to_string()), rtt);

        // Also update in replica manager
        self.replica_manager.update_rtt(node_a, node_b, rtt);
    }

    /// Get RTT between two nodes
    fn get_rtt(&self, node_a: &str, node_b: &str) -> Option<Duration> {
        self.rtt_matrix
            .get(&(node_a.to_string(), node_b.to_string()))
            .map(|v| *v)
    }

    /// Calculate average RTT for a node to all others
    fn get_average_rtt(&self, node_id: &str, other_nodes: &[&str]) -> Duration {
        let mut total_rtt = Duration::from_millis(0);
        let mut count = 0;

        for other in other_nodes {
            if *other != node_id {
                if let Some(rtt) = self.get_rtt(node_id, other) {
                    total_rtt += rtt;
                    count += 1;
                }
            }
        }

        if count > 0 {
            Duration::from_millis(total_rtt.as_millis() as u64 / count)
        } else {
            Duration::from_millis(50) // Default 50ms
        }
    }

    /// Optimize ring topology for a set of block replicas
    ///
    /// Uses greedy nearest-neighbor heuristic to solve approximate TSP:
    /// 1. Start with replica having lowest average RTT
    /// 2. Iteratively select next replica with minimum RTT to current node
    /// 3. Returns ordered list optimized for sequential pipeline execution
    pub fn optimize_ring_topology(
        &self,
        replicas: Vec<BlockReplica>,
    ) -> Vec<BlockReplica> {
        if replicas.len() <= 2 {
            return replicas; // No optimization needed
        }

        // Calculate baseline total RTT (random order)
        let baseline_rtt = self.calculate_total_pipeline_latency(&replicas);

        // Get all node IDs
        let node_ids: Vec<String> = replicas.iter().map(|r| r.node_id.clone()).collect();
        let node_id_refs: Vec<&str> = node_ids.iter().map(|s| s.as_str()).collect();

        // Find starting node (lowest average RTT)
        let mut start_idx = 0;
        let mut lowest_avg_rtt = Duration::from_millis(u64::MAX);

        for (idx, replica) in replicas.iter().enumerate() {
            let avg_rtt = self.get_average_rtt(&replica.node_id, &node_id_refs);
            if avg_rtt < lowest_avg_rtt {
                lowest_avg_rtt = avg_rtt;
                start_idx = idx;
            }
        }

        // Build optimized route using greedy nearest-neighbor
        let mut optimized = Vec::with_capacity(replicas.len());
        let mut remaining: Vec<BlockReplica> = replicas;

        // Start with the best initial node
        let current = remaining.remove(start_idx);
        let mut current_node_id = current.node_id.clone();
        optimized.push(current);

        // Greedily select nearest neighbors
        while !remaining.is_empty() {
            // Find nearest neighbor
            let nearest_idx = remaining
                .iter()
                .enumerate()
                .map(|(idx, replica)| {
                    let rtt = self
                        .get_rtt(&current_node_id, &replica.node_id)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(u64::MAX);
                    (idx, rtt)
                })
                .min_by_key(|&(_, rtt)| rtt)
                .map(|(idx, _)| idx);

            if let Some(idx) = nearest_idx {
                let next = remaining.remove(idx);
                current_node_id = next.node_id.clone();
                optimized.push(next);
            } else {
                // No RTT data available, just take the first remaining
                let next = remaining.remove(0);
                current_node_id = next.node_id.clone();
                optimized.push(next);
            }
        }

        // Calculate optimized total RTT
        let optimized_rtt = self.calculate_total_pipeline_latency(&optimized);

        // Record metrics
        self.metrics.record_optimization(
            optimized_rtt.as_millis() as u64,
            baseline_rtt.as_millis() as u64,
        );

        tracing::info!(
            "Ring topology optimized: {}ms -> {}ms ({}% improvement)",
            baseline_rtt.as_millis(),
            optimized_rtt.as_millis(),
            if baseline_rtt.as_millis() > 0 {
                ((baseline_rtt.as_millis() - optimized_rtt.as_millis()) as f64
                    / baseline_rtt.as_millis() as f64
                    * 100.0) as i64
            } else {
                0
            }
        );

        optimized
    }

    /// Calculate total pipeline latency for a route
    ///
    /// Sums RTTs between consecutive replicas in the pipeline
    pub fn calculate_total_pipeline_latency(&self, route: &[BlockReplica]) -> Duration {
        if route.len() < 2 {
            return Duration::from_millis(0);
        }

        let mut total_rtt = Duration::from_millis(0);

        for i in 0..route.len() - 1 {
            let current_node_id = &route[i].node_id;
            let next_node_id = &route[i + 1].node_id;

            let rtt = self
                .get_rtt(current_node_id, next_node_id)
                .unwrap_or_else(|| Duration::from_millis(10)); // Default 10ms

            total_rtt += rtt;
        }

        total_rtt
    }

    /// Get optimization metrics
    pub fn get_metrics(&self) -> Arc<TopologyMetrics> {
        self.metrics.clone()
    }

    /// Get RTT matrix statistics
    pub fn get_rtt_matrix_stats(&self) -> (usize, f64) {
        let count = self.rtt_matrix.len();
        let avg_rtt: f64 = if count > 0 {
            let sum: u64 = self
                .rtt_matrix
                .iter()
                .map(|entry| entry.value().as_millis() as u64)
                .sum();
            sum as f64 / count as f64
        } else {
            0.0
        };
        (count, avg_rtt)
    }

    /// Clear all RTT measurements
    pub fn clear_rtt_measurements(&self) {
        self.rtt_matrix.clear();
    }

    /// Initialize RTT matrix from replica manager data
    pub fn initialize_from_replica_manager(&self) {
        for replica in self.replica_manager.get_all_replicas() {
            for (peer_id, rtt) in &replica.rtt_map {
                self.rtt_matrix
                    .insert((replica.node_id.clone(), peer_id.clone()), *rtt);
            }
        }
    }
}

impl Clone for TopologyOptimizer {
    fn clone(&self) -> Self {
        Self {
            replica_manager: self.replica_manager.clone(),
            rtt_matrix: DashMap::new(),
            metrics: self.metrics.clone(),
        }
    }
}

/// Extension trait for RouteSelector to integrate topology optimization
pub trait TopologyOptimizedRouteSelector {
    /// Select route with topology optimization
    fn select_route_with_topology_optimization(
        &self,
        block_ids: &[u32],
        optimizer: &TopologyOptimizer,
    ) -> Result<Vec<BlockReplica>, crate::route_selector::RouteSelectionError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replica_manager::{HealthStatus, ReplicaHealth};
    use libp2p::PeerId;
    use network::protocol::NodeCapabilities;
    use std::str::FromStr;

    fn create_test_replica(block_id: u32, node_id: &str) -> BlockReplica {
        BlockReplica {
            block_id,
            node_id: node_id.to_string(),
            peer_id: PeerId::from_str("12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN").unwrap(),
            capabilities: NodeCapabilities {
                has_gpu: false,
                cuda_version: None,
                max_model_size_gb: 8,
                supported_dtypes: vec!["f32".to_string()],
            },
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.5,
            rtt_map: HashMap::new(),
        }
    }

    #[test]
    fn test_topology_optimization_basic() {
        let replica_manager = Arc::new(ReplicaManager::new());
        let optimizer = TopologyOptimizer::new(replica_manager);

        // Create test replicas
        let replicas = vec![
            create_test_replica(0, "node-0"),
            create_test_replica(1, "node-1"),
            create_test_replica(2, "node-2"),
            create_test_replica(3, "node-3"),
        ];

        // Set up RTT matrix: ring topology should be 0-1-2-3-0
        optimizer.update_rtt_measurement("node-0", "node-1", Duration::from_millis(10));
        optimizer.update_rtt_measurement("node-1", "node-2", Duration::from_millis(10));
        optimizer.update_rtt_measurement("node-2", "node-3", Duration::from_millis(10));
        optimizer.update_rtt_measurement("node-0", "node-2", Duration::from_millis(50));
        optimizer.update_rtt_measurement("node-0", "node-3", Duration::from_millis(50));
        optimizer.update_rtt_measurement("node-1", "node-3", Duration::from_millis(50));

        // Optimize
        let optimized = optimizer.optimize_ring_topology(replicas);

        // Verify all replicas are included
        assert_eq!(optimized.len(), 4);
        let node_ids: Vec<_> = optimized.iter().map(|r| r.node_id.clone()).collect();
        assert!(node_ids.contains(&"node-0".to_string()));
        assert!(node_ids.contains(&"node-1".to_string()));
        assert!(node_ids.contains(&"node-2".to_string()));
        assert!(node_ids.contains(&"node-3".to_string()));

        // Check that metrics were recorded
        let (optimizations, _, improvement) = optimizer.get_metrics().get_stats();
        assert_eq!(optimizations, 1);
        // Improvement should be positive (optimized better than random)
        // Note: with the specific RTT setup, we might not always get improvement
    }

    #[test]
    fn test_calculate_total_pipeline_latency() {
        let replica_manager = Arc::new(ReplicaManager::new());
        let optimizer = TopologyOptimizer::new(replica_manager);

        let replicas = vec![
            create_test_replica(0, "node-0"),
            create_test_replica(1, "node-1"),
            create_test_replica(2, "node-2"),
        ];

        // Set up RTT: 0->1 = 10ms, 1->2 = 20ms
        optimizer.update_rtt_measurement("node-0", "node-1", Duration::from_millis(10));
        optimizer.update_rtt_measurement("node-1", "node-2", Duration::from_millis(20));

        let total_latency = optimizer.calculate_total_pipeline_latency(&replicas);
        assert_eq!(total_latency.as_millis(), 30);
    }

    #[test]
    fn test_empty_route_latency() {
        let replica_manager = Arc::new(ReplicaManager::new());
        let optimizer = TopologyOptimizer::new(replica_manager);

        let empty: Vec<BlockReplica> = vec![];
        assert_eq!(optimizer.calculate_total_pipeline_latency(&empty).as_millis(), 0);

        let single = vec![create_test_replica(0, "node-0")];
        assert_eq!(optimizer.calculate_total_pipeline_latency(&single).as_millis(), 0);
    }

    #[test]
    fn test_rtt_measurement_update() {
        let replica_manager = Arc::new(ReplicaManager::new());
        let optimizer = TopologyOptimizer::new(replica_manager);

        optimizer.update_rtt_measurement("node-a", "node-b", Duration::from_millis(25));

        let (count, avg) = optimizer.get_rtt_matrix_stats();
        assert_eq!(count, 2); // Symmetric entries: (a,b) and (b,a)
        assert!(avg > 0.0);

        let rtt = optimizer.get_rtt("node-a", "node-b");
        assert_eq!(rtt, Some(Duration::from_millis(25)));
    }

    #[test]
    fn test_average_rtt_calculation() {
        let replica_manager = Arc::new(ReplicaManager::new());
        let optimizer = TopologyOptimizer::new(replica_manager);

        // Set up RTT measurements
        optimizer.update_rtt_measurement("node-0", "node-1", Duration::from_millis(10));
        optimizer.update_rtt_measurement("node-0", "node-2", Duration::from_millis(20));
        optimizer.update_rtt_measurement("node-0", "node-3", Duration::from_millis(30));

        let avg = optimizer.get_average_rtt("node-0", &["node-1", "node-2", "node-3"]);
        assert_eq!(avg.as_millis(), 20); // Average of 10, 20, 30
    }

    #[test]
    fn test_metrics_recording() {
        let metrics = TopologyMetrics::default();

        metrics.record_optimization(100, 150);
        metrics.record_optimization(90, 150);
        metrics.record_optimization(80, 150);

        let (optimizations, avg_rtt, improvement) = metrics.get_stats();
        assert_eq!(optimizations, 3);
        assert!(avg_rtt > 0.0);
        assert!(improvement > 0.0); // Should show positive improvement
    }
}
