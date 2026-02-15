// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Health Monitor for Replica Health Tracking
//!
//! Monitors replica health via heartbeat protocol with <1s detection:
//! - Sends heartbeat every 500ms
//! - Detects failures after 3 missed heartbeats (1.5s)
//! - Triggers failover for dead replicas

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use thiserror::Error;
use uuid::Uuid;

use crate::replica_manager::{ReplicaManager, HealthStatus};
use network::protocol::NetworkMessage;

/// Error type for health monitor operations
#[derive(Debug, Error)]
pub enum HealthMonitorError {
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("replica manager error: {0}")]
    ReplicaManagerError(String),
}

/// Monitors replica health via heartbeat protocol
pub struct HealthMonitor {
    /// Replica manager for health state
    replica_manager: Arc<ReplicaManager>,
    /// Heartbeat interval (default 500ms)
    heartbeat_interval: Duration,
    /// Number of missed heartbeats before marking dead (default 3)
    failure_threshold: u32,
    /// Network message sender
    network_tx: mpsc::Sender<NetworkMessage>,
    /// Node ID of this health monitor
    node_id: String,
}

impl HealthMonitor {
    /// Create a new HealthMonitor
    pub fn new(
        replica_manager: Arc<ReplicaManager>,
        heartbeat_interval_ms: u64,
        failure_threshold: u32,
        network_tx: mpsc::Sender<NetworkMessage>,
    ) -> Self {
        Self {
            replica_manager,
            heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
            failure_threshold,
            network_tx,
            node_id: "orchestrator".to_string(),
        }
    }

    /// Create with custom node ID
    pub fn with_node_id(
        mut self,
        node_id: String,
    ) -> Self {
        self.node_id = node_id;
        self
    }

    /// Main monitoring loop - detects failures
    pub async fn monitor_loop(&self) -> Result<(), HealthMonitorError> {
        let mut interval = tokio::time::interval(self.heartbeat_interval);
        
        loop {
            interval.tick().await;
            
            // Detect failures
            let failed_nodes = self.detect_failures().await;
            
            // Trigger failover for each failed node
            for node_id in failed_nodes {
                tracing::warn!("Node {} marked as dead after {} missed heartbeats", node_id, self.failure_threshold);
                
                // Notify network about failure
                let _ = self.network_tx.send(NetworkMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp: chrono::Utc::now().timestamp(),
                    active_tasks: Vec::new(),
                    load_score: -1.0, // Negative load score indicates failure
                }).await;
            }
        }
    }

    /// Detect nodes that missed 3+ heartbeats
    pub async fn detect_failures(&self) -> Vec<String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let threshold_ms = self.heartbeat_interval.as_millis() as i64 * self.failure_threshold as i64;
        
        let mut failed_nodes = Vec::new();
        
        // Check all known replicas
        for replica in self.replica_manager.get_all_replicas() {
            let last_heartbeat_ms = replica.last_heartbeat * 1000; // Convert seconds to milliseconds
            let elapsed_ms = now_ms - last_heartbeat_ms;
            
            if elapsed_ms > threshold_ms {
                // Record missed heartbeat
                if let Some(new_status) = self.replica_manager.record_missed_heartbeat(
                    &replica.node_id,
                    self.failure_threshold,
                ) {
                    if new_status == HealthStatus::Dead {
                        failed_nodes.push(replica.node_id.clone());
                    }
                }
            }
        }
        
        failed_nodes
    }

    /// Handle incoming heartbeat message
    pub fn handle_heartbeat(&self, msg: &NetworkMessage) -> Result<(), HealthMonitorError> {
        if let NetworkMessage::Heartbeat { node_id, timestamp, active_tasks, load_score } = msg {
            // Update replica health
            self.replica_manager.update_replica_health(
                node_id,
                *timestamp,
                active_tasks.clone(),
                *load_score,
            );
            
            tracing::trace!("Received heartbeat from {} at {} with load {}", node_id, timestamp, load_score);
        }
        
        Ok(())
    }

    /// Start heartbeat sender task
    pub fn start_heartbeat_sender(&self, node_id: String, load_provider: Arc<dyn LoadProvider>) {
        let interval = self.heartbeat_interval;
        let network_tx = self.network_tx.clone();
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            
            loop {
                ticker.tick().await;
                
                let load_score = load_provider.get_current_load();
                let active_tasks = load_provider.get_active_tasks();
                
                let heartbeat = NetworkMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp: chrono::Utc::now().timestamp(),
                    active_tasks,
                    load_score,
                };
                
                if let Err(e) = network_tx.send(heartbeat).await {
                    tracing::error!("Failed to send heartbeat: {}", e);
                }
            }
        });
    }

    /// Trigger failover for a failed node
    pub async fn trigger_failover(&self, failed_node_id: &str) -> Result<(), HealthMonitorError> {
        tracing::info!("Triggering failover for failed node: {}", failed_node_id);
        
        // The actual failover logic is handled by the OrchestratorNode
        // This method just broadcasts the failure notification
        let _ = self.network_tx.send(NetworkMessage::Heartbeat {
            node_id: failed_node_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            active_tasks: Vec::new(),
            load_score: -1.0, // Signal failure
        }).await;
        
        Ok(())
    }

    /// Get current health statistics
    pub fn get_health_stats(&self) -> HealthStats {
        let total_replicas = self.replica_manager.get_all_replicas().len();
        let healthy_replicas = self.replica_manager.healthy_replica_count();
        
        HealthStats {
            total_replicas,
            healthy_replicas,
            dead_replicas: total_replicas - healthy_replicas,
            failure_threshold: self.failure_threshold,
            heartbeat_interval_ms: self.heartbeat_interval.as_millis() as u64,
        }
    }

    /// Set failure threshold (number of missed heartbeats before marking dead)
    pub fn set_failure_threshold(&mut self, threshold: u32) {
        self.failure_threshold = threshold;
    }

    /// Set heartbeat interval
    pub fn set_heartbeat_interval(&mut self, interval_ms: u64) {
        self.heartbeat_interval = Duration::from_millis(interval_ms);
    }
}

/// Health statistics
#[derive(Clone, Debug)]
pub struct HealthStats {
    pub total_replicas: usize,
    pub healthy_replicas: usize,
    pub dead_replicas: usize,
    pub failure_threshold: u32,
    pub heartbeat_interval_ms: u64,
}

/// Trait for providing load information
pub trait LoadProvider: Send + Sync {
    /// Get current load score (0.0 - 1.0)
    fn get_current_load(&self) -> f32;
    /// Get list of active task IDs
    fn get_active_tasks(&self) -> Vec<Uuid>;
}

/// Simple load provider implementation
pub struct SimpleLoadProvider {
    load_score: Arc<std::sync::atomic::AtomicU32>,
    active_tasks: Arc<std::sync::Mutex<Vec<Uuid>>>,
}

impl SimpleLoadProvider {
    pub fn new() -> Self {
        Self {
            load_score: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            active_tasks: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn update_load(&self, score: f32) {
        let bits = score.to_bits();
        self.load_score.store(bits, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_load(&self) -> f32 {
        let bits = self.load_score.load(std::sync::atomic::Ordering::Relaxed);
        f32::from_bits(bits)
    }

    pub fn add_task(&self, task_id: Uuid) {
        if let Ok(mut tasks) = self.active_tasks.lock() {
            tasks.push(task_id);
        }
    }

    pub fn remove_task(&self, task_id: Uuid) {
        if let Ok(mut tasks) = self.active_tasks.lock() {
            tasks.retain(|t| *t != task_id);
        }
    }
}

impl LoadProvider for SimpleLoadProvider {
    fn get_current_load(&self) -> f32 {
        self.get_load()
    }

    fn get_active_tasks(&self) -> Vec<Uuid> {
        self.active_tasks.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Default for SimpleLoadProvider {
    fn default() -> Self {
        Self::new()
    }
}
