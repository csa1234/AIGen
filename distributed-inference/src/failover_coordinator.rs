// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Failover Coordinator for Automatic Recovery
//!
//! Orchestrates the recovery process by:
//! 1. Detecting failures via HealthMonitor
//! 2. Selecting replacement replicas
//! 3. Resuming from checkpoints
//! Target: <500ms failover latency

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::checkpoint_manager::CheckpointManager;
use crate::health_monitor::HealthMonitor;
use crate::orchestrator::{OrchestratorError, OrchestratorNode};
use crate::pipeline_message::{Failover, PipelineMessage};
use crate::replica_manager::BlockReplica;
use network::protocol::NetworkMessage;

/// Coordinator for automatic failover and recovery
pub struct FailoverCoordinator {
    orchestrator: Arc<OrchestratorNode>,
    checkpoint_manager: Arc<CheckpointManager>,
    health_monitor: Arc<HealthMonitor>,
    network_tx: mpsc::Sender<NetworkMessage>,
    metrics: Arc<FailoverMetrics>,
    /// Enable automatic failover (can be disabled for testing)
    enabled: std::sync::atomic::AtomicBool,
}

impl FailoverCoordinator {
    /// Create a new failover coordinator
    pub fn new(
        orchestrator: Arc<OrchestratorNode>,
        checkpoint_manager: Arc<CheckpointManager>,
        health_monitor: Arc<HealthMonitor>,
        network_tx: mpsc::Sender<NetworkMessage>,
    ) -> Self {
        Self {
            orchestrator,
            checkpoint_manager,
            health_monitor,
            network_tx,
            metrics: Arc::new(FailoverMetrics::default()),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create with explicit metrics
    pub fn with_metrics(
        orchestrator: Arc<OrchestratorNode>,
        checkpoint_manager: Arc<CheckpointManager>,
        health_monitor: Arc<HealthMonitor>,
        network_tx: mpsc::Sender<NetworkMessage>,
        metrics: Arc<FailoverMetrics>,
    ) -> Self {
        Self {
            orchestrator,
            checkpoint_manager,
            health_monitor,
            network_tx,
            metrics,
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Check if auto-failover is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Enable automatic failover
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
        tracing::info!("Automatic failover enabled");
    }

    /// Disable automatic failover
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        tracing::warn!("Automatic failover disabled");
    }

    /// Main recovery loop - monitors for failures and triggers recovery
    pub async fn recovery_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        tracing::info!("FailoverCoordinator recovery loop started");

        loop {
            interval.tick().await;

            // Skip if auto-failover is disabled
            if !self.is_enabled() {
                continue;
            }

            // Detect failures via HealthMonitor
            let failed_nodes = self.health_monitor.detect_failures().await;

            for node_id in failed_nodes {
                tracing::warn!("Node failure detected: {}", node_id);
                
                if let Err(e) = self.handle_node_failure(&node_id).await {
                    tracing::error!("Failover failed for node {}: {}", node_id, e);
                    self.metrics.record_failover_failure();
                }
            }
        }
    }

    /// Handle single node failure with checkpoint recovery
    async fn handle_node_failure(&self, failed_node_id: &str) -> Result<(), OrchestratorError> {
        let start_time = Instant::now();

        // Find which block this node was assigned to
        let block_id = self
            .orchestrator
            .replica_manager
            .get_block_for_node(failed_node_id)
            .ok_or_else(|| OrchestratorError::BlockNotFound(0))?;

        // Get affected inferences
        let affected_inferences = self.get_affected_inferences(block_id, failed_node_id);
        
        if affected_inferences.is_empty() {
            tracing::info!(
                "No active inferences affected by failure of node {} on block {}",
                failed_node_id, block_id
            );
            // Still need to handle replica failure for future assignments
            let _ = self.orchestrator
                .handle_replica_failure(block_id, failed_node_id)
                .await?;
            return Ok(());
        }

        tracing::info!(
            "Processing failover for node {} on block {} affecting {} inferences",
            failed_node_id,
            block_id,
            affected_inferences.len()
        );

        // Select replacement replica
        let replacement = self
            .orchestrator
            .handle_replica_failure(block_id, failed_node_id)
            .await?;

        // Recover each affected inference
        let mut recovered_count = 0;
        for inference_id in &affected_inferences {
            match self
                .recover_inference(*inference_id, block_id, failed_node_id, &replacement)
                .await
            {
                Ok(_) => {
                    recovered_count += 1;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to recover inference {}: {}",
                        inference_id, e
                    );
                }
            }
        }

        // Record metrics
        let recovery_time = start_time.elapsed();
        self.metrics
            .record_failover(recovery_time, recovered_count);

        tracing::info!(
            "Failover completed for node {} in {:?}: {}/{} inferences recovered",
            failed_node_id,
            recovery_time,
            recovered_count,
            affected_inferences.len()
        );

        Ok(())
    }

    /// Recover single inference from checkpoint
    async fn recover_inference(
        &self,
        inference_id: Uuid,
        block_id: u32,
        failed_node_id: &str,
        replacement: &BlockReplica,
    ) -> Result<(), OrchestratorError> {
        // Get latest checkpoint
        let checkpoint = self
            .checkpoint_manager
            .get_latest_checkpoint(inference_id, block_id)
            .ok_or_else(|| {
                OrchestratorError::SchedulerError(format!(
                    "No checkpoint found for inference {} on block {}",
                    inference_id, block_id
                ))
            })?;

        // Verify checkpoint integrity before sending
        if !checkpoint.verify_integrity() {
            return Err(OrchestratorError::SchedulerError(
                "Checkpoint integrity verification failed".to_string()
            ));
        }

        // Create failover message
        let failover = Failover::new(
            inference_id,
            block_id,
            failed_node_id.to_string(),
            replacement.node_id.clone(),
            checkpoint,
        );

        // Serialize and send to replacement node
        let pipeline_msg = PipelineMessage::Failover(failover);
        let serialized = pipeline_msg
            .serialize()
            .map_err(|e| OrchestratorError::SchedulerError(e.to_string()))?;

        let msg = NetworkMessage::PipelineInference(serialized);

        self.network_tx
            .send(msg)
            .await
            .map_err(|e| OrchestratorError::SchedulerError(e.to_string()))?;

        tracing::debug!(
            "Sent failover for inference {} to replacement node {}",
            inference_id,
            replacement.node_id
        );

        Ok(())
    }

    /// Get list of inferences affected by node failure
    fn get_affected_inferences(&self, block_id: u32, node_id: &str) -> Vec<Uuid> {
        self.orchestrator
            .active_inferences
            .iter()
            .filter(|entry| {
                entry
                    .value()
                    .pipeline_route
                    .iter()
                    .any(|r| r.block_id == block_id && r.node_id == node_id)
            })
            .map(|entry| *entry.key())
            .collect()
    }

    /// Get current failover statistics
    pub fn get_stats(&self) -> FailoverStats {
        self.metrics.get_stats()
    }
}

/// Metrics for failover operations
#[derive(Debug, Default)]
pub struct FailoverMetrics {
    total_failovers: AtomicU64,
    total_recovery_time_ms: AtomicU64,
    total_inferences_recovered: AtomicU64,
    total_failover_failures: AtomicU64,
    min_recovery_time_ms: AtomicU64,
    max_recovery_time_ms: AtomicU64,
}

impl FailoverMetrics {
    /// Create new metrics with proper initialization
    pub fn new() -> Self {
        Self {
            total_failovers: AtomicU64::new(0),
            total_recovery_time_ms: AtomicU64::new(0),
            total_inferences_recovered: AtomicU64::new(0),
            total_failover_failures: AtomicU64::new(0),
            min_recovery_time_ms: AtomicU64::new(u64::MAX),
            max_recovery_time_ms: AtomicU64::new(0),
        }
    }

    /// Record a successful failover
    pub fn record_failover(&self, recovery_time: Duration, inferences_count: usize) {
        self.total_failovers.fetch_add(1, Ordering::Relaxed);
        
        let time_ms = recovery_time.as_millis() as u64;
        self.total_recovery_time_ms.fetch_add(time_ms, Ordering::Relaxed);
        
        self.total_inferences_recovered.fetch_add(
            inferences_count as u64,
            Ordering::Relaxed,
        );

        // Update min
        let mut current_min = self.min_recovery_time_ms.load(Ordering::Relaxed);
        while time_ms < current_min {
            match self.min_recovery_time_ms.compare_exchange(
                current_min,
                time_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }

        // Update max
        let mut current_max = self.max_recovery_time_ms.load(Ordering::Relaxed);
        while time_ms > current_max {
            match self.max_recovery_time_ms.compare_exchange(
                current_max,
                time_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    /// Record a failover failure
    pub fn record_failover_failure(&self) {
        self.total_failover_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current statistics
    pub fn get_stats(&self) -> FailoverStats {
        let total = self.total_failovers.load(Ordering::Relaxed);
        let total_time = self.total_recovery_time_ms.load(Ordering::Relaxed);
        let min_time = self.min_recovery_time_ms.load(Ordering::Relaxed);
        let max_time = self.max_recovery_time_ms.load(Ordering::Relaxed);

        FailoverStats {
            total_failovers: total,
            avg_recovery_time_ms: if total > 0 { total_time / total } else { 0 },
            min_recovery_time_ms: if min_time == u64::MAX { 0 } else { min_time },
            max_recovery_time_ms: max_time,
            total_inferences_recovered: self.total_inferences_recovered.load(Ordering::Relaxed),
            total_failover_failures: self.total_failover_failures.load(Ordering::Relaxed),
        }
    }
}

impl Default for FailoverMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for failover operations
#[derive(Clone, Debug, Default)]
pub struct FailoverStats {
    pub total_failovers: u64,
    pub avg_recovery_time_ms: u64,
    pub min_recovery_time_ms: u64,
    pub max_recovery_time_ms: u64,
    pub total_inferences_recovered: u64,
    pub total_failover_failures: u64,
}

/// Configuration for failover behavior
#[derive(Clone, Debug)]
pub struct FailoverConfig {
    /// Heartbeat interval in milliseconds (default: 500)
    pub heartbeat_interval_ms: u64,
    /// Number of missed heartbeats before marking dead (default: 3)
    pub failure_threshold: u32,
    /// Enable automatic failover (default: true)
    pub enable_auto_failover: bool,
    /// Maximum recovery time target in milliseconds (default: 500)
    pub target_recovery_time_ms: u64,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 500,
            failure_threshold: 3,
            enable_auto_failover: true,
            target_recovery_time_ms: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_metrics() {
        let metrics = FailoverMetrics::new();

        // Record some failovers
        metrics.record_failover(Duration::from_millis(100), 5);
        metrics.record_failover(Duration::from_millis(200), 3);
        metrics.record_failover(Duration::from_millis(150), 4);

        let stats = metrics.get_stats();
        assert_eq!(stats.total_failovers, 3);
        assert_eq!(stats.total_inferences_recovered, 12);
        assert_eq!(stats.avg_recovery_time_ms, 150); // (100+200+150)/3
        assert_eq!(stats.min_recovery_time_ms, 100);
        assert_eq!(stats.max_recovery_time_ms, 200);
    }

    #[test]
    fn test_failover_metrics_failure() {
        let metrics = FailoverMetrics::new();

        metrics.record_failover_failure();
        metrics.record_failover_failure();

        let stats = metrics.get_stats();
        assert_eq!(stats.total_failover_failures, 2);
        assert_eq!(stats.total_failovers, 0);
    }

    #[test]
    fn test_default_failover_config() {
        let config = FailoverConfig::default();
        assert_eq!(config.heartbeat_interval_ms, 500);
        assert_eq!(config.failure_threshold, 3);
        assert!(config.enable_auto_failover);
        assert_eq!(config.target_recovery_time_ms, 500);
    }
}
