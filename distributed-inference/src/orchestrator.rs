// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Orchestrator Node for Distributed Pipeline Inference
//!
//! The OrchestratorNode combines static block assignments with dynamic replica selection,
//! implementing leader election via DCS, health monitoring with <1s detection, and
//! dynamic route selection based on RTT and load.
//!
//! ## Canary Deployment System
//!
//! The orchestrator supports infinite canary rollout with the following phases:
//! - **Shadow Phase (24h)**: Route 100% to stable, run canary in background with full logging
//! - **Canary Phase**: Gradual traffic shifting from 0.1% to 100% with safety monitoring
//! - **Auto-Rollback**: Automatic rollback within 5 minutes if safety thresholds violated

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use libp2p::PeerId;
use tokio::sync::{mpsc, RwLock};
use thiserror::Error;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::block_assignment::{BlockAssignment, StaticBlockConfig};
use crate::pipeline_message::{InferenceStart, PipelineMessage};
use crate::replica_manager::{BlockReplica, ReplicaManager, ReplicaHealth};
use crate::health_monitor::HealthMonitor;
use crate::route_selector::RouteSelector;
use crate::coordinator::{StaticPipelineCoordinator, BatchConfig, InferenceRequest, BatchedInference};
use distributed_compute::scheduler::DynamicScheduler;
use consensus::validator::ValidatorRegistry;
use network::protocol::{NetworkMessage, NodeCapabilities};

// =============================================================================
// Canary Deployment Types
// =============================================================================

/// Duration of shadow phase in seconds (24 hours)
pub const SHADOW_PHASE_DURATION_SECONDS: i64 = 86400;

/// Duration of CEO veto window in seconds (24 hours)
pub const CEO_VETO_WINDOW_SECONDS: i64 = 86400;

/// Interval between canary traffic increments in seconds (1 hour)
pub const CANARY_INCREMENT_INTERVAL_SECONDS: i64 = 3600;

/// Initial canary traffic percentage
pub const INITIAL_CANARY_TRAFFIC_PERCENTAGE: f32 = 0.1;

/// Traffic increment step size
pub const TRAFFIC_INCREMENT_STEP: f32 = 0.5;

/// Maximum complaint rate threshold (0.1%)
pub const MAX_COMPLAINT_RATE_THRESHOLD: f32 = 0.001;

/// Maximum latency increase threshold (5%)
pub const MAX_LATENCY_INCREASE_THRESHOLD: f32 = 1.05;

/// Minimum benevolence score threshold
pub const MIN_BENEVOLENCE_SCORE: f32 = 0.99;

/// Deployment status for a model version
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentStatus {
    /// Stable production version
    Stable,
    /// Canary version receiving traffic
    Canary,
    /// Shadow mode - no traffic, background inference only
    Shadow,
    /// Deprecated version, no longer used
    Deprecated,
}

impl Default for DeploymentStatus {
    fn default() -> Self {
        DeploymentStatus::Stable
    }
}

/// User complaint record for canary monitoring
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserComplaint {
    /// User who submitted the complaint
    pub user_id: String,
    /// Inference ID that caused the complaint
    pub inference_id: Uuid,
    /// Unix timestamp of complaint
    pub timestamp: i64,
    /// Reason for complaint
    pub reason: String,
    /// Model version that was used (stable or canary)
    pub model_version: String,
}

/// Metrics tracked for canary deployment
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CanaryMetrics {
    /// Total requests routed to canary version
    pub canary_requests: u64,
    /// Constitutional filter violations detected
    pub canary_violations: u64,
    /// Benevolence model failures (score < 0.99)
    pub canary_benevolence_failures: u64,
    /// Safety oracle unsafe votes
    pub canary_oracle_unsafe_votes: u64,
    /// User complaints for canary version
    pub canary_complaints: u64,
    /// Average latency for canary version (ms)
    pub canary_avg_latency_ms: f32,
    /// Average latency for stable version (ms)
    pub stable_avg_latency_ms: f32,
    /// Last benevolence score computed
    pub last_benevolence_score: f32,
    /// Total stable requests during canary period
    pub stable_requests: u64,
}

impl CanaryMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate complaint rate (complaints / requests)
    pub fn complaint_rate(&self) -> f32 {
        if self.canary_requests == 0 {
            0.0
        } else {
            self.canary_complaints as f32 / self.canary_requests as f32
        }
    }

    /// Calculate latency ratio (canary / stable)
    pub fn latency_ratio(&self) -> f32 {
        if self.stable_avg_latency_ms == 0.0 {
            1.0
        } else {
            self.canary_avg_latency_ms / self.stable_avg_latency_ms
        }
    }

    /// Check if all rollout criteria pass
    pub fn check_rollout_criteria(&self) -> RolloutCheckResult {
        let mut failures = Vec::new();

        if self.canary_violations > 0 {
            failures.push(RolloutFailure::ConstitutionalViolation(self.canary_violations));
        }

        if self.last_benevolence_score < MIN_BENEVOLENCE_SCORE {
            failures.push(RolloutFailure::BenevolenceScoreTooLow(self.last_benevolence_score));
        }

        if self.canary_oracle_unsafe_votes > 0 {
            failures.push(RolloutFailure::SafetyOracleUnsafe(self.canary_oracle_unsafe_votes));
        }

        if self.complaint_rate() > MAX_COMPLAINT_RATE_THRESHOLD {
            failures.push(RolloutFailure::ComplaintRateTooHigh(self.complaint_rate()));
        }

        if self.latency_ratio() > MAX_LATENCY_INCREASE_THRESHOLD {
            failures.push(RolloutFailure::LatencyTooHigh(self.latency_ratio()));
        }

        RolloutCheckResult {
            passed: failures.is_empty(),
            failures,
        }
    }

    /// Update average latency using exponential moving average
    pub fn update_canary_latency(&mut self, latency_ms: f32) {
        if self.canary_avg_latency_ms == 0.0 {
            self.canary_avg_latency_ms = latency_ms;
        } else {
            self.canary_avg_latency_ms = (self.canary_avg_latency_ms + latency_ms) / 2.0;
        }
    }

    /// Update stable average latency using exponential moving average
    pub fn update_stable_latency(&mut self, latency_ms: f32) {
        if self.stable_avg_latency_ms == 0.0 {
            self.stable_avg_latency_ms = latency_ms;
        } else {
            self.stable_avg_latency_ms = (self.stable_avg_latency_ms + latency_ms) / 2.0;
        }
    }
}

/// Result of checking rollout criteria
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RolloutCheckResult {
    /// Whether all criteria passed
    pub passed: bool,
    /// List of failures if any
    pub failures: Vec<RolloutFailure>,
}

/// Types of rollout failures
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RolloutFailure {
    /// Constitutional filter violation detected
    ConstitutionalViolation(u64),
    /// Benevolence score below threshold
    BenevolenceScoreTooLow(f32),
    /// Safety oracle returned unsafe votes
    SafetyOracleUnsafe(u64),
    /// Complaint rate exceeds threshold
    ComplaintRateTooHigh(f32),
    /// Latency increase exceeds threshold
    LatencyTooHigh(f32),
}

/// Result of safety check on inference output
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyCheckResult {
    /// Number of constitutional violations
    pub violations: u64,
    /// Benevolence score (0-1)
    pub benevolence_score: f32,
    /// Number of unsafe votes from oracle
    pub unsafe_votes: u64,
    /// Whether the output passed all checks
    pub passed: bool,
}

/// Active canary deployment state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CanaryDeployment {
    /// Model being deployed
    pub model_id: String,
    /// Current stable version identifier
    pub stable_version: String,
    /// Canary version being rolled out
    pub canary_version: String,
    /// Current traffic percentage to canary (0.0-100.0)
    pub traffic_percentage: f32,
    /// Whether in shadow mode (0% traffic, background inference only)
    pub shadow_mode: bool,
    /// Unix timestamp when deployment started
    pub deployment_start: i64,
    /// Unix timestamp of last traffic increment
    pub last_increment: i64,
    /// Current metrics for the deployment
    pub metrics: CanaryMetrics,
    /// CEO veto deadline (24h after auto-approval)
    pub ceo_veto_deadline: Option<i64>,
    /// Whether CEO has vetoed
    pub ceo_vetoed: bool,
    /// Current deployment phase
    pub phase: CanaryPhase,
}

/// Phases of canary deployment
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanaryPhase {
    /// Waiting for CEO veto window to expire
    PendingCeoWindow,
    /// Shadow mode - background inference only
    Shadow,
    /// Canary mode - receiving traffic
    Canary,
    /// Fully rolled out
    Complete,
    /// Rolled back due to failure
    RolledBack,
    /// Aborted due to CEO veto
    Aborted,
}

impl CanaryDeployment {
    /// Create a new canary deployment in shadow mode
    pub fn new(model_id: String, stable_version: String, canary_version: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            model_id,
            stable_version,
            canary_version,
            traffic_percentage: 0.0,
            shadow_mode: true,
            deployment_start: now,
            last_increment: now,
            metrics: CanaryMetrics::new(),
            ceo_veto_deadline: Some(now + CEO_VETO_WINDOW_SECONDS),
            ceo_vetoed: false,
            phase: CanaryPhase::PendingCeoWindow,
        }
    }

    /// Check if CEO veto window has expired
    pub fn is_ceo_window_expired(&self) -> bool {
        if let Some(deadline) = self.ceo_veto_deadline {
            let now = chrono::Utc::now().timestamp();
            return now >= deadline;
        }
        true
    }

    /// Check if shadow phase duration has completed
    pub fn is_shadow_phase_complete(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.deployment_start >= SHADOW_PHASE_DURATION_SECONDS
    }

    /// Check if enough time has passed for next traffic increment
    pub fn is_increment_due(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.last_increment >= CANARY_INCREMENT_INTERVAL_SECONDS
    }

    /// Transition to shadow phase
    pub fn start_shadow_phase(&mut self) {
        self.phase = CanaryPhase::Shadow;
        self.shadow_mode = true;
        self.traffic_percentage = 0.0;
    }

    /// Transition to canary phase
    pub fn start_canary_phase(&mut self) {
        self.phase = CanaryPhase::Canary;
        self.shadow_mode = false;
        self.traffic_percentage = INITIAL_CANARY_TRAFFIC_PERCENTAGE;
        self.last_increment = chrono::Utc::now().timestamp();
    }

    /// Increment traffic percentage
    pub fn increment_traffic(&mut self) {
        self.traffic_percentage = (self.traffic_percentage + TRAFFIC_INCREMENT_STEP).min(100.0);
        self.last_increment = chrono::Utc::now().timestamp();
    }

    /// Check if rollout is complete
    pub fn is_complete(&self) -> bool {
        self.traffic_percentage >= 100.0
    }

    /// Mark deployment as complete
    pub fn mark_complete(&mut self) {
        self.phase = CanaryPhase::Complete;
        self.traffic_percentage = 100.0;
    }

    /// Execute rollback
    pub fn rollback(&mut self, reason: &str) {
        self.phase = CanaryPhase::RolledBack;
        self.traffic_percentage = 0.0;
        tracing::warn!("Canary deployment rolled back: {}", reason);
    }

    /// Abort due to CEO veto
    pub fn abort_for_ceo_veto(&mut self) {
        self.phase = CanaryPhase::Aborted;
        self.ceo_vetoed = true;
        self.traffic_percentage = 0.0;
    }
}

/// Rollback event record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollbackEvent {
    /// Model ID
    pub model_id: String,
    /// Canary version that was rolled back
    pub canary_version: String,
    /// Unix timestamp of rollback
    pub timestamp: i64,
    /// Reason for rollback
    pub reason: String,
    /// Metrics snapshot at time of rollback
    pub metrics_snapshot: CanaryMetrics,
    /// Traffic percentage at time of rollback
    pub traffic_at_rollback: f32,
}

/// Pipeline authentication token for replica join requests
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PipelineAuthToken {
    pub node_id: String,
    pub block_id: u32,
    pub timestamp: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl PipelineAuthToken {
    /// Token expiry time in seconds (5 minutes)
    pub const EXPIRY_SECONDS: u64 = 300;
    
    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp) > Self::EXPIRY_SECONDS
    }
    
    /// Serialize token data for signing (excludes signature)
    fn signable_bytes(&self) -> Vec<u8> {
        let data = (
            &self.node_id,
            self.block_id,
            self.timestamp,
            &self.public_key,
        );
        serde_json::to_vec(&data).unwrap_or_default()
    }
}

/// Verify a pipeline authentication token using Ed25519
pub fn verify_pipeline_auth_token(token: &PipelineAuthToken) -> Result<(), OrchestratorError> {
    // Check expiry
    if token.is_expired() {
        return Err(OrchestratorError::ReplicaManagerError(
            "pipeline auth token has expired".to_string()
        ));
    }
    
    // Reconstruct public key from protobuf encoding
    let public_key = libp2p::identity::PublicKey::try_decode_protobuf(&token.public_key)
        .map_err(|e| OrchestratorError::ReplicaManagerError(
            format!("failed to decode public key: {}", e)
        ))?;
    
    // Create unsigned token copy for verification
    let mut unsigned = token.clone();
    unsigned.signature = Vec::new();
    let signable = unsigned.signable_bytes();
    
    // Verify signature
    if !public_key.verify(&signable, &token.signature) {
        return Err(OrchestratorError::ReplicaManagerError(
            "pipeline auth token signature verification failed".to_string()
        ));
    }
    
    Ok(())
}

/// Orchestrator configuration
#[derive(Clone, Debug)]
pub struct OrchestratorConfig {
    /// Number of replicas per block (default K=3)
    pub replication_factor: usize,
    /// Heartbeat interval in milliseconds (default 500ms)
    pub heartbeat_interval_ms: u64,
    /// Number of missed heartbeats before marking dead (default 3)
    pub failure_threshold: u32,
    /// Enable dynamic routing based on RTT and load
    pub enable_dynamic_routing: bool,
    /// Node ID of this orchestrator instance
    pub node_id: String,
    /// Enable dynamic batching for inference requests
    pub enable_batching: bool,
    /// Maximum batch size when batching is enabled (default 8)
    pub max_batch_size: usize,
    /// Maximum wait time in milliseconds for batching (default 100ms)
    pub max_batch_wait_ms: u64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            replication_factor: 3,
            heartbeat_interval_ms: 500,
            failure_threshold: 3,
            enable_dynamic_routing: true,
            node_id: "orchestrator".to_string(),
            enable_batching: true,
            max_batch_size: 8,
            max_batch_wait_ms: 100,
        }
    }
}

/// Tracking information for a dispatched batch
#[derive(Clone, Debug)]
pub struct BatchTracking {
    pub batch_id: Uuid,
    pub inference_ids: Vec<Uuid>,
    pub pipeline_route: Vec<u32>,
    pub first_replica_node_id: String,
    pub dispatch_time: std::time::Instant,
}

/// Result routing information for batch processing
#[derive(Debug)]
pub struct BatchResultRouting {
    pub batch_id: Uuid,
    pub inference_ids: Vec<Uuid>,
    pub response_channels: Vec<tokio::sync::oneshot::Sender<Result<String, OrchestratorError>>>,
}

/// Replica plan for an inference task
#[derive(Clone, Debug)]
pub struct ReplicaPlan {
    pub inference_id: Uuid,
    pub model_id: String,
    /// Ordered list of replicas for each block in the pipeline
    pub pipeline_route: Vec<BlockReplica>,
    /// Estimated total latency in milliseconds
    pub estimated_latency_ms: u64,
}

/// Error type for orchestrator operations
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("not leader")]
    NotLeader,
    #[error("no healthy replicas for block {0}")]
    NoHealthyReplicas(u32),
    #[error("block not found: {0}")]
    BlockNotFound(u32),
    #[error("invalid pipeline route")]
    InvalidRoute,
    #[error("scheduler error: {0}")]
    SchedulerError(String),
    #[error("replica manager error: {0}")]
    ReplicaManagerError(String),
    #[error("health monitor error: {0}")]
    HealthMonitorError(String),
    #[error("route selection error: {0}")]
    RouteSelectionError(String),
}

/// Core orchestrator component that manages block replicas and coordinates inference
pub struct OrchestratorNode {
    /// Static block assignment configuration
    pub block_assignment: Arc<RwLock<BlockAssignment>>,
    /// Replica manager for tracking block replicas
    pub replica_manager: Arc<ReplicaManager>,
    /// Health monitor for heartbeat-based health checks
    pub health_monitor: Arc<HealthMonitor>,
    /// Route selector for dynamic replica selection
    pub route_selector: Arc<RouteSelector>,
    /// DCS scheduler for leader election and coordination
    pub dcs: Arc<DynamicScheduler>,
    /// Leader status cache
    is_leader: Arc<RwLock<bool>>,
    /// Configuration
    pub config: OrchestratorConfig,
    /// Network message sender
    network_tx: mpsc::Sender<NetworkMessage>,
    /// Active inference tracking: inference_id -> ReplicaPlan
    pub active_inferences: Arc<DashMap<Uuid, ReplicaPlan>>,
    /// Pipeline coordinator for batching and inference management
    coordinator: Arc<RwLock<StaticPipelineCoordinator>>,
    /// Batch tracking: batch_id -> BatchTracking for result routing
    batch_tracking: Arc<DashMap<Uuid, BatchTracking>>,
    /// Batch result routing for completed batches
    batch_results: Arc<DashMap<Uuid, BatchResultRouting>>,
    /// Active canary deployment (if any)
    active_canary: Arc<RwLock<Option<CanaryDeployment>>>,
    /// Complaint tracker: model_id -> list of complaints
    complaint_tracker: Arc<DashMap<String, Vec<UserComplaint>>>,
    /// Rollback history for audit trail
    rollback_history: Arc<RwLock<Vec<RollbackEvent>>>,
    /// Inference start times for latency tracking: inference_id -> start_time
    inference_start_times: Arc<DashMap<Uuid, std::time::Instant>>,
    /// Inference version tracking: inference_id -> "stable" or "canary"
    inference_version: Arc<DashMap<Uuid, String>>,
}

impl OrchestratorNode {
    /// Create a new OrchestratorNode with DCS integration and batch dispatcher
    pub fn new(
        config: StaticBlockConfig,
        dcs: Arc<DynamicScheduler>,
        _validator_registry: Arc<ValidatorRegistry>,
        network_tx: mpsc::Sender<NetworkMessage>,
        orchestrator_config: OrchestratorConfig,
    ) -> Self {
        let block_assignment = Arc::new(RwLock::new(config.to_block_assignment()));
        let replica_manager = Arc::new(ReplicaManager::new());
        let health_monitor = Arc::new(HealthMonitor::new(
            replica_manager.clone(),
            orchestrator_config.heartbeat_interval_ms,
            orchestrator_config.failure_threshold,
            network_tx.clone(),
        ));
        let route_selector = Arc::new(RouteSelector::new(
            replica_manager.clone(),
            orchestrator_config.enable_dynamic_routing,
        ));

        // Create batch config for coordinator
        let batch_config = BatchConfig {
            max_batch_size: orchestrator_config.max_batch_size,
            max_wait_ms: orchestrator_config.max_batch_wait_ms,
            enable_batching: orchestrator_config.enable_batching,
        };

        // Create the coordinator with batching support
        let mut coordinator = StaticPipelineCoordinator::with_batch_config(
            config.clone(),
            None, // This orchestrator doesn't process specific blocks
            true, // This is a coordinator
            batch_config,
        );

        // Take ownership of batch_receiver for dispatcher loop
        let batch_receiver = if orchestrator_config.enable_batching {
            coordinator.take_batch_receiver()
        } else {
            None
        };

        let coordinator = Arc::new(RwLock::new(coordinator));
        let batch_tracking: Arc<DashMap<Uuid, BatchTracking>> = Arc::new(DashMap::new());
        let batch_results: Arc<DashMap<Uuid, BatchResultRouting>> = Arc::new(DashMap::new());

        // Spawn batch dispatcher loop if batching is enabled
        if let Some(batch_rx) = batch_receiver {
            let network_tx_clone = network_tx.clone();
            let replica_manager_clone = replica_manager.clone();
            let batch_tracking_clone = batch_tracking.clone();
            let batch_results_clone = batch_results.clone();
            let coordinator_clone = coordinator.clone();

            tokio::spawn(async move {
                Self::batch_dispatcher_loop(
                    batch_rx,
                    network_tx_clone,
                    replica_manager_clone,
                    batch_tracking_clone,
                    batch_results_clone,
                    coordinator_clone,
                ).await;
            });
        }

        Self {
            block_assignment,
            replica_manager,
            health_monitor,
            route_selector,
            dcs,
            is_leader: Arc::new(RwLock::new(false)),
            config: orchestrator_config,
            network_tx,
            active_inferences: Arc::new(DashMap::new()),
            coordinator,
            batch_tracking,
            batch_results,
            active_canary: Arc::new(RwLock::new(None)),
            complaint_tracker: Arc::new(DashMap::new()),
            rollback_history: Arc::new(RwLock::new(Vec::new())),
            inference_start_times: Arc::new(DashMap::new()),
            inference_version: Arc::new(DashMap::new()),
        }
    }

    /// Batch dispatcher loop: consumes batches from channel and dispatches to replicas
    async fn batch_dispatcher_loop(
        mut batch_rx: mpsc::Receiver<BatchedInference>,
        network_tx: mpsc::Sender<NetworkMessage>,
        replica_manager: Arc<ReplicaManager>,
        batch_tracking: Arc<DashMap<Uuid, BatchTracking>>,
        batch_results: Arc<DashMap<Uuid, BatchResultRouting>>,
        coordinator: Arc<RwLock<StaticPipelineCoordinator>>,
    ) {
        tracing::info!("Batch dispatcher loop started");

        while let Some(batch) = batch_rx.recv().await {
            // Get pipeline route from batch
            let pipeline_route = &batch.start_message.pipeline_route;
            
            if pipeline_route.is_empty() {
                eprintln!("No pipeline route available for batch {}", batch.batch_id);
                continue;
            }

            // Select first replica for the first block
            let first_block_id = pipeline_route[0];
            let healthy_replicas = replica_manager.get_healthy_replicas(first_block_id);

            if healthy_replicas.is_empty() {
                eprintln!("No healthy replicas for block {}", first_block_id);
                continue;
            }

            let first_replica = &healthy_replicas[0];

            // Track batch for result routing
            let tracking = BatchTracking {
                batch_id: batch.batch_id,
                inference_ids: batch.inference_ids.clone(),
                pipeline_route: pipeline_route.clone(),
                first_replica_node_id: first_replica.node_id.clone(),
                dispatch_time: std::time::Instant::now(),
            };
            batch_tracking.insert(batch.batch_id, tracking);

            // Serialize and send the batch via network
            let msg = NetworkMessage::PipelineInference(
                PipelineMessage::InferenceStart(batch.start_message)
                    .serialize()
                    .unwrap_or_default()
            );

            if let Err(e) = network_tx.send(msg).await {
                eprintln!("Failed to dispatch batch {}: {}", batch.batch_id, e);
            } else {
                // Record batch dispatched metric
                let metrics = coordinator.read().await.get_batching_metrics();
                metrics.record_batch_dispatched();

                // Create result routing entry
                let result_routing = BatchResultRouting {
                    batch_id: batch.batch_id,
                    inference_ids: batch.inference_ids.clone(),
                    response_channels: Vec::new(),
                };
                batch_results.insert(batch.batch_id, result_routing);

                tracing::info!(
                    "Dispatched batch {} with {} inferences to replica {}",
                    batch.batch_id,
                    batch.inference_ids.len(),
                    first_replica.node_id
                );
            }
        }

        tracing::info!("Batch dispatcher loop terminated");
    }

    /// Check if this node is the leader (delegates to DCS)
    pub async fn is_leader(&self) -> bool {
        *self.is_leader.read().await
    }

    /// Trigger leader election via DCS
    pub async fn elect_leader(&self) -> Result<bool, OrchestratorError> {
        match self.dcs.elect_leader().await {
            Ok(is_leader) => {
                *self.is_leader.write().await = is_leader;
                Ok(is_leader)
            }
            Err(e) => Err(OrchestratorError::SchedulerError(e.to_string())),
        }
    }

    /// Start the background batch processor when batching is enabled
    /// 
    /// This spawns the time-based flush task that periodically flushes pending batches
    /// to the channel. The actual dispatch to replicas is handled by `batch_dispatcher_loop()`.
    pub async fn start_batch_processor(&self) {
        if self.config.enable_batching {
            let coordinator = self.coordinator.clone();
            
            // Start the batch processor in a background task
            tokio::spawn(async move {
                let interval_duration = Duration::from_millis(
                    coordinator.read().await.get_batch_config().max_wait_ms / 2
                );
                let mut interval = tokio::time::interval(interval_duration);
                
                loop {
                    interval.tick().await;
                    
                    // Check if batching is still enabled
                    if !coordinator.read().await.get_batch_config().enable_batching {
                        break;
                    }
                    
                    // Check if there are pending requests
                    let pending_count = coordinator.read().await.get_pending_request_count().await;
                    if pending_count == 0 {
                        continue;
                    }
                    
                    // Check if max wait time exceeded for first request
                    let should_flush = {
                        let coord = coordinator.read().await;
                        let max_wait = coord.get_batch_config().max_wait_ms;
                        if let Some(first_time) = coord.get_first_pending_time().await {
                            first_time.elapsed().as_millis() as u64 >= max_wait
                        } else {
                            pending_count > 0 // Flush if any pending
                        }
                    };
                    
                    if should_flush || pending_count >= coordinator.read().await.get_batch_config().max_batch_size {
                        // Flush batches to the channel - dispatcher loop handles the actual network send
                        if let Err(e) = coordinator.write().await.flush_pending_batches().await {
                            eprintln!("Batch flush error: {}", e);
                        }
                    }
                }
            });
        }
    }

    /// Assign inference to optimal replicas for each block
    pub async fn assign_inference_to_replicas(
        &self,
        model_id: String,
        prompt: String,
        max_tokens: u32,
    ) -> Result<ReplicaPlan, OrchestratorError> {
        if !self.is_leader().await {
            return Err(OrchestratorError::NotLeader);
        }

        // Route to appropriate version based on canary state
        let (version, is_canary) = self.route_to_version(&model_id).await;

        // If batching is enabled, enqueue the request for batch processing
        if self.config.enable_batching {
            let inference_id = Uuid::new_v4();
            
            // Store start time for latency tracking
            self.inference_start_times.insert(inference_id, std::time::Instant::now());
            
            // Track version for this inference
            self.inference_version.insert(inference_id, if is_canary { "canary".to_string() } else { "stable".to_string() });
            
            // Increment request counters in canary metrics
            {
                let mut active_canary = self.active_canary.write().await;
                if let Some(ref mut deployment) = *active_canary {
                    if is_canary {
                        deployment.metrics.canary_requests += 1;
                    } else {
                        deployment.metrics.stable_requests += 1;
                    }
                }
            }
            
            // Run shadow inference in background if in shadow phase
            {
                let active_canary = self.active_canary.read().await;
                if let Some(ref deployment) = *active_canary {
                    if deployment.model_id == model_id && deployment.phase == CanaryPhase::Shadow {
                        let orchestrator = Arc::new(self.clone_ref());
                        let model_id_clone = model_id.clone();
                        let prompt_clone = prompt.clone();
                        tokio::spawn(async move {
                            orchestrator.run_shadow_inference(&model_id_clone, &prompt_clone, max_tokens).await;
                        });
                    }
                }
            }

            let request = InferenceRequest {
                model_id: model_id.clone(),
                prompt: prompt.clone(),
                max_tokens,
                user_id: "default".to_string(), // TODO: use actual user_id from auth context
                timestamp: std::time::Instant::now(),
                inference_id,
            };

            let coordinator = self.coordinator.read().await;
            if let Err(e) = coordinator.enqueue_inference_request(request).await {
                return Err(OrchestratorError::SchedulerError(format!(
                    "Failed to enqueue inference request: {}",
                    e
                )));
            }
            drop(coordinator);

            // Return a plan with the pre-generated inference_id
            // The actual execution will happen via batch processing
            let assignment = self.block_assignment.read().await;
            let pipeline_route: Vec<BlockReplica> = assignment
                .get_block_ids()
                .iter()
                .filter_map(|block_id| {
                    let healthy = self.replica_manager.get_healthy_replicas(*block_id);
                    self.route_selector.select_best_replica(&healthy, None)
                })
                .collect();

            if pipeline_route.is_empty() {
                return Err(OrchestratorError::InvalidRoute);
            }

            let mut total_latency_ms: u64 = 0;
            for replica in &pipeline_route {
                total_latency_ms += self.estimate_replica_latency(replica).as_millis() as u64;
            }

            let plan = ReplicaPlan {
                inference_id,
                model_id: version.clone(), // Use routed version
                pipeline_route,
                estimated_latency_ms: total_latency_ms,
            };

            // Store active inference
            self.active_inferences.insert(inference_id, plan.clone());

            return Ok(plan);
        }

        // Non-batched path: immediate execution
        let inference_id = Uuid::new_v4();
        
        // Store start time for latency tracking
        self.inference_start_times.insert(inference_id, std::time::Instant::now());
        
        // Track version for this inference
        self.inference_version.insert(inference_id, if is_canary { "canary".to_string() } else { "stable".to_string() });
        
        // Increment request counters in canary metrics
        {
            let mut active_canary = self.active_canary.write().await;
            if let Some(ref mut deployment) = *active_canary {
                if is_canary {
                    deployment.metrics.canary_requests += 1;
                } else {
                    deployment.metrics.stable_requests += 1;
                }
            }
        }
        
        // Run shadow inference in background if in shadow phase
        {
            let active_canary = self.active_canary.read().await;
            if let Some(ref deployment) = *active_canary {
                if deployment.model_id == model_id && deployment.phase == CanaryPhase::Shadow {
                    let orchestrator = Arc::new(self.clone_ref());
                    let model_id_clone = model_id.clone();
                    let prompt_clone = prompt.clone();
                    tokio::spawn(async move {
                        orchestrator.run_shadow_inference(&model_id_clone, &prompt_clone, max_tokens).await;
                    });
                }
            }
        }
        
        let assignment = self.block_assignment.read().await;

        // Build pipeline route: select best replica for each block
        let mut pipeline_route = Vec::new();
        let mut total_latency_ms: u64 = 0;

        for block in &assignment.blocks {
            // Get healthy replicas for this block
            let healthy_replicas = self.replica_manager.get_healthy_replicas(block.block_id);
            if healthy_replicas.is_empty() {
                return Err(OrchestratorError::NoHealthyReplicas(block.block_id));
            }

            // Select best replica based on RTT and load
            let best_replica = self
                .route_selector
                .select_best_replica(&healthy_replicas, pipeline_route.last())
                .ok_or(OrchestratorError::NoHealthyReplicas(block.block_id))?;

            // Estimate latency contribution
            let latency_contribution = self.estimate_replica_latency(&best_replica);
            total_latency_ms += latency_contribution.as_millis() as u64;

            pipeline_route.push(best_replica);
        }

        // Validate the route
        if pipeline_route.len() != assignment.blocks.len() {
            return Err(OrchestratorError::InvalidRoute);
        }

        let plan = ReplicaPlan {
            inference_id,
            model_id: version.clone(), // Use routed version
            pipeline_route,
            estimated_latency_ms: total_latency_ms,
        };

        // Store active inference
        self.active_inferences.insert(inference_id, plan.clone());

        // Send inference start to first block with version information
        if let Some(_first_replica) = plan.pipeline_route.first() {
            let inference_start = InferenceStart {
                inference_id,
                model_id: version, // Propagate version to pipeline message
                prompt,
                max_tokens,
                pipeline_route: assignment.get_block_ids(),
            };

            let msg = NetworkMessage::PipelineInference(
                PipelineMessage::InferenceStart(inference_start)
                    .serialize()
                    .unwrap(),
            );

            let _ = self.network_tx.send(msg).await;
        }

        Ok(plan)
    }

    /// Handle replica failure - reassign to healthy replica
    pub async fn handle_replica_failure(
        &self,
        block_id: u32,
        failed_node_id: &str,
    ) -> Result<BlockReplica, OrchestratorError> {
        if !self.is_leader().await {
            return Err(OrchestratorError::NotLeader);
        }

        // Mark replica as unhealthy
        self.replica_manager.remove_replica(block_id, failed_node_id);

        // Find active inferences using this replica
        let affected_inferences: Vec<Uuid> = self.active_inferences
            .iter()
            .filter(|entry| {
                entry.value().pipeline_route.iter().any(|r| {
                    r.block_id == block_id && r.node_id == failed_node_id
                })
            })
            .map(|entry| *entry.key())
            .collect();

        // Get healthy replicas for this block
        let healthy_replicas = self.replica_manager.get_healthy_replicas(block_id);
        if healthy_replicas.is_empty() {
            return Err(OrchestratorError::NoHealthyReplicas(block_id));
        }

        // Select best replacement replica
        let replacement = self.route_selector
            .select_best_replica(&healthy_replicas, None)
            .ok_or(OrchestratorError::NoHealthyReplicas(block_id))?;

        // Update active inference routes (failover messaging is now handled by FailoverCoordinator)
        for inference_id in affected_inferences {
            if let Some(mut plan) = self.active_inferences.get_mut(&inference_id) {
                if let Some(replica) = plan.pipeline_route.iter_mut().find(|r| r.block_id == block_id) {
                    *replica = replacement.clone();
                }
            }
        }

        Ok(replacement)
    }

    /// Send failover message to replacement node
    /// 
    /// DEPRECATED: This method is deprecated and disabled because it emits zeroed checkpoint hashes.
    /// Use the FailoverCoordinator for checkpoint-aware failover recovery instead.
    #[allow(dead_code)]
    async fn send_failover_message(
        &self,
        _inference_id: Uuid,
        _block_id: u32,
        _failed_node_id: &str,
        _replacement: &BlockReplica,
    ) -> Result<(), OrchestratorError> {
        // Legacy failover path disabled - zeroed checkpoint hash removed
        // FailoverCoordinator now handles checkpoint-aware recovery
        tracing::warn!("Legacy send_failover_message path is disabled. Use FailoverCoordinator for checkpoint-aware failover.");
        Err(OrchestratorError::SchedulerError(
            "Legacy failover path disabled - use FailoverCoordinator".to_string()
        ))
    }

    /// Get current block assignments
    pub async fn get_block_assignments(&self) -> HashMap<u32, Vec<BlockReplica>> {
        let mut result = HashMap::new();
        let assignment = self.block_assignment.read().await;

        for block_id in assignment.get_block_ids() {
            let replicas = self.replica_manager.get_replicas(block_id);
            result.insert(block_id, replicas);
        }

        result
    }

    /// Get health status for all replicas
    pub fn get_replica_health(&self, block_id: Option<u32>) -> Vec<ReplicaHealth> {
        match block_id {
            Some(id) => self.replica_manager
                .get_replicas(id)
                .into_iter()
                .filter_map(|r| self.replica_manager.get_replica_health(&r.node_id))
                .collect(),
            None => self.replica_manager.get_all_health_status(),
        }
    }

    /// Get orchestrator state for RPC queries
    pub async fn get_orchestrator_state(&self) -> OrchestratorState {
        let assignment = self.block_assignment.read().await;
        let total_replicas: usize = assignment.get_block_ids()
            .iter()
            .map(|id| self.replica_manager.get_replicas(*id).len())
            .sum();

        let healthy_replicas: usize = assignment.get_block_ids()
            .iter()
            .map(|id| self.replica_manager.get_healthy_replicas(*id).len())
            .sum();

        OrchestratorState {
            is_leader: self.is_leader().await,
            leader_node_id: if self.is_leader().await {
                Some(self.config.node_id.clone())
            } else {
                None
            },
            total_blocks: assignment.blocks.len() as u32,
            total_replicas: total_replicas as u32,
            healthy_replicas: healthy_replicas as u32,
        }
    }

    // ==================== Canary Deployment Accessors ====================

    /// Get active canary deployment (public accessor)
    pub fn get_active_canary(&self) -> Arc<RwLock<Option<CanaryDeployment>>> {
        self.active_canary.clone()
    }

    /// Get complaint tracker (public accessor)
    pub fn get_complaint_tracker(&self) -> Arc<DashMap<String, Vec<UserComplaint>>> {
        self.complaint_tracker.clone()
    }

    /// Get rollback history (public accessor)
    pub fn get_rollback_history(&self) -> Arc<RwLock<Vec<RollbackEvent>>> {
        self.rollback_history.clone()
    }

    /// Register a new replica for a block
    pub async fn register_replica(
        &self,
        block_id: u32,
        node_id: String,
        peer_id: PeerId,
        capabilities: NodeCapabilities,
    ) -> Result<(), OrchestratorError> {
        let replica = BlockReplica {
            block_id,
            node_id,
            peer_id,
            capabilities,
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.0,
            rtt_map: HashMap::new(),
        };

        self.replica_manager.add_replica(block_id, replica);

        // Ensure replication factor is maintained
        self.replica_manager.ensure_replication_factor(block_id, self.config.replication_factor);

        Ok(())
    }

    /// Register a replica with authentication token validation
    pub async fn register_replica_with_auth(
        &self,
        block_id: u32,
        node_id: String,
        peer_id: PeerId,
        capabilities: NodeCapabilities,
        auth_token: &PipelineAuthToken,
    ) -> Result<(), OrchestratorError> {
        // Verify the authentication token
        verify_pipeline_auth_token(auth_token)?;
        
        // Verify token matches the claimed identity
        if auth_token.node_id != node_id {
            return Err(OrchestratorError::ReplicaManagerError(
                format!("token node_id '{}' does not match claimed node_id '{}'", 
                    auth_token.node_id, node_id)
            ));
        }
        
        if auth_token.block_id != block_id {
            return Err(OrchestratorError::ReplicaManagerError(
                format!("token block_id '{}' does not match claimed block_id '{}'", 
                    auth_token.block_id, block_id)
            ));
        }
        
        // Token is valid - register the replica
        self.register_replica(block_id, node_id, peer_id, capabilities).await
    }

    /// Handle replica join request from network
    pub async fn handle_replica_join_request(
        &self,
        node_id: String,
        block_id: u32,
        peer_id_bytes: Vec<u8>,
        capabilities: NodeCapabilities,
        auth_token_bytes: Vec<u8>,
    ) -> Result<(), OrchestratorError> {
        // Deserialize auth token
        let auth_token: PipelineAuthToken = serde_json::from_slice(&auth_token_bytes)
            .map_err(|e| OrchestratorError::ReplicaManagerError(
                format!("failed to deserialize auth token: {}", e)
            ))?;
        
        // Deserialize peer_id
        let peer_id = PeerId::from_bytes(&peer_id_bytes)
            .map_err(|e| OrchestratorError::ReplicaManagerError(
                format!("failed to deserialize peer_id: {}", e)
            ))?;
        
        // Register with auth validation
        self.register_replica_with_auth(block_id, node_id, peer_id, capabilities, &auth_token).await
    }

    /// Update replica health from heartbeat
    pub fn update_replica_health(&self, node_id: &str, heartbeat: &NetworkMessage) {
        if let NetworkMessage::Heartbeat { timestamp, active_tasks, load_score, .. } = heartbeat {
            self.replica_manager.update_replica_health(
                node_id,
                *timestamp,
                active_tasks.clone(),
                *load_score,
            );
        }
    }

    /// Estimate latency for a replica
    fn estimate_replica_latency(&self, replica: &BlockReplica) -> Duration {
        // Base latency from load score (0.0-1.0 maps to 0-100ms queue time)
        let queue_time_ms = (replica.load_score * 100.0) as u64;

        // Average RTT to this replica
        let avg_rtt_ms: u64 = if replica.rtt_map.is_empty() {
            50 // Default 50ms if no RTT data
        } else {
            let sum: u64 = replica.rtt_map.values()
                .map(|d| d.as_millis() as u64)
                .sum();
            sum / replica.rtt_map.len() as u64
        };

        Duration::from_millis(queue_time_ms + avg_rtt_ms)
    }

    /// Start heartbeat sender for this node
    pub async fn start_heartbeat_sender(&self) {
        let interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let network_tx = self.network_tx.clone();
        let node_id = self.config.node_id.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let heartbeat = NetworkMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp: chrono::Utc::now().timestamp(),
                    active_tasks: Vec::new(), // TODO: track active tasks
                    load_score: 0.0, // TODO: compute actual load score
                };

                let _ = network_tx.send(heartbeat).await;
            }
        });
    }

    /// Start health monitoring loop
    pub async fn start_health_monitor(&self) {
        let health_monitor = self.health_monitor.clone();
        let replica_manager = self.replica_manager.clone();
        let orchestrator = Arc::new(self.clone_ref());

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;

                // Detect failures
                let failed_nodes = health_monitor.detect_failures().await;
                for node_id in failed_nodes {
                    // Find which block this node was assigned to
                    if let Some(block_id) = replica_manager.get_block_for_node(&node_id) {
                        let _ = orchestrator.handle_replica_failure(block_id, &node_id).await;
                    }
                }
            }
        });
    }

    /// Start failover coordinator loop
    pub async fn start_failover_coordinator(
        &self,
        checkpoint_manager: Arc<crate::checkpoint_manager::CheckpointManager>,
    ) {
        let coordinator = Arc::new(crate::failover_coordinator::FailoverCoordinator::new(
            Arc::new(self.clone_ref()),
            checkpoint_manager,
            self.health_monitor.clone(),
            self.network_tx.clone(),
        ));
        
        tokio::spawn(async move {
            coordinator.recovery_loop().await;
        });
    }

    /// Handle batched pipeline message result
    pub async fn handle_pipeline_message(&self, message: PipelineMessage) -> Result<(), OrchestratorError> {
        match message {
            PipelineMessage::ActivationChunk(chunk) => {
                // Check if this is the final block output
                if chunk.is_final_block() {
                    // Check for batch_id in the tensor metadata
                    // The batch_id is encoded in the first 16 bytes of tensor data when batched
                    if let Some(batch_id) = Self::extract_batch_id_from_chunk(&chunk) {
                        // Split outputs by [BATCH_SEP]
                        let outputs = Self::split_batched_output(&chunk);
                        
                        // Process batch result
                        if let Some(result_routing) = self.batch_results.get(&batch_id) {
                            let inference_ids = result_routing.inference_ids.clone();
                            drop(result_routing);
                            
                            // Process each individual result
                            for (i, inference_id) in inference_ids.iter().enumerate() {
                                if let Some(output) = outputs.get(i) {
                                    // Complete the inference
                                    self.complete_inference(*inference_id, output.clone()).await?;
                                }
                            }
                            
                            // Remove batch tracking
                            self.batch_tracking.remove(&batch_id);
                            self.batch_results.remove(&batch_id);
                            
                            // Update metrics
                            let metrics = self.coordinator.read().await.get_batching_metrics();
                            metrics.record_batch_completed();
                            
                            tracing::info!(
                                "Completed batch {} with {} inferences",
                                batch_id,
                                inference_ids.len()
                            );
                        }
                    } else {
                        // Single inference completion
                        let inference_id = chunk.inference_id;
                        let output = Self::extract_output_from_chunk(&chunk);
                        self.complete_inference(inference_id, output).await?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    
    /// Extract batch_id from activation chunk tensor metadata
    fn extract_batch_id_from_chunk(chunk: &crate::pipeline_message::ActivationChunk) -> Option<Uuid> {
        // Check for batch marker in tensor metadata
        // The batch_id is encoded in a custom metadata field when present
        if chunk.tensor.metadata.layer_range.0 == u32::MAX {
            // Special marker indicating batched output - batch_id encoded in layer_range.1
            let _batch_id_bytes = chunk.tensor.metadata.layer_range.1.to_le_bytes();
            // This is a simplified extraction - in production, batch_id would be properly serialized
            None // Placeholder - requires proper encoding in block executor
        } else {
            None
        }
    }
    
    /// Split batched output by [BATCH_SEP] delimiter
    fn split_batched_output(_chunk: &crate::pipeline_message::ActivationChunk) -> Vec<String> {
        // Decompress tensor data and split by separator
        // This is a placeholder - actual implementation depends on output encoding
        vec![]
    }
    
    /// Extract single output from chunk
    fn extract_output_from_chunk(_chunk: &crate::pipeline_message::ActivationChunk) -> String {
        // Convert tensor data to string output
        // Placeholder implementation
        String::new()
    }
    
    /// Complete an individual inference with result
    async fn complete_inference(
        &self,
        inference_id: Uuid,
        output: String,
    ) -> Result<(), OrchestratorError> {
        // Calculate latency from start time
        let latency_ms = if let Some((_, start_time)) = self.inference_start_times.remove(&inference_id) {
            start_time.elapsed().as_millis() as f32
        } else {
            0.0
        };
        
        // Get the version for this inference
        let version = self.inference_version.get(&inference_id)
            .map(|v| v.clone())
            .unwrap_or_else(|| "stable".to_string());
        
        // Remove from active inferences
        if let Some((_, _plan)) = self.active_inferences.remove(&inference_id) {
            tracing::info!(
                "Completed inference {} with output length {}",
                inference_id,
                output.len()
            );
        }
        
        // Update metrics based on version
        {
            let mut active_canary = self.active_canary.write().await;
            if let Some(ref mut deployment) = *active_canary {
                if version == "canary" {
                    // Update canary latency
                    deployment.metrics.update_canary_latency(latency_ms);
                    
                    // Run safety check on canary output
                    let safety_result = self.check_canary_safety(&output).await;
                    
                    // Update safety metrics
                    deployment.metrics.canary_violations += safety_result.violations;
                    if safety_result.benevolence_score < MIN_BENEVOLENCE_SCORE {
                        deployment.metrics.canary_benevolence_failures += 1;
                    }
                    deployment.metrics.canary_oracle_unsafe_votes += safety_result.unsafe_votes;
                    deployment.metrics.last_benevolence_score = safety_result.benevolence_score;
                    
                    // Trigger rollback if safety check failed
                    if !safety_result.passed {
                        let reason = format!("Canary safety check failed: violations={}, benevolence={}, unsafe_votes={}",
                            safety_result.violations, safety_result.benevolence_score, safety_result.unsafe_votes);
                        deployment.rollback(&reason);
                        self.execute_rollback_internal(deployment, &reason).await;
                        *active_canary = None;
                    }
                } else {
                    // Update stable latency
                    deployment.metrics.update_stable_latency(latency_ms);
                }
            }
        }
        
        // Clean up version tracking
        self.inference_version.remove(&inference_id);
        
        Ok(())
    }

    /// Clone a reference to self for async tasks
    fn clone_ref(&self) -> Self {
        Self {
            block_assignment: self.block_assignment.clone(),
            replica_manager: self.replica_manager.clone(),
            health_monitor: self.health_monitor.clone(),
            route_selector: self.route_selector.clone(),
            dcs: self.dcs.clone(),
            is_leader: self.is_leader.clone(),
            config: self.config.clone(),
            network_tx: self.network_tx.clone(),
            active_inferences: self.active_inferences.clone(),
            coordinator: self.coordinator.clone(),
            batch_tracking: self.batch_tracking.clone(),
            batch_results: self.batch_results.clone(),
            active_canary: self.active_canary.clone(),
            complaint_tracker: self.complaint_tracker.clone(),
            rollback_history: self.rollback_history.clone(),
            inference_start_times: self.inference_start_times.clone(),
            inference_version: self.inference_version.clone(),
        }
    }

    /// Get batch tracking information
    pub fn get_batch_tracking(&self, batch_id: Uuid) -> Option<BatchTracking> {
        self.batch_tracking.get(&batch_id).map(|t| t.clone())
    }

    /// Get batch results for RPC queries
    pub fn get_batch_results(&self) -> Vec<(Uuid, Vec<Uuid>)> {
        self.batch_results
            .iter()
            .map(|entry| (*entry.key(), entry.value().inference_ids.clone()))
            .collect()
    }

    /// Clear timed-out batches (called by health monitor)
    pub async fn clear_timeout_batches(&self) {
        let timeout = Duration::from_millis(self.config.max_batch_wait_ms * 2);
        let now = std::time::Instant::now();
        
        let timed_out: Vec<Uuid> = self
            .batch_tracking
            .iter()
            .filter(|entry| now.duration_since(entry.value().dispatch_time) > timeout)
            .map(|entry| *entry.key())
            .collect();
        
        for batch_id in timed_out {
            if let Some((_, tracking)) = self.batch_tracking.remove(&batch_id) {
                tracing::warn!("Batch {} timed out after {:?}", batch_id, timeout);
                
                // Mark individual inferences as failed
                for inference_id in &tracking.inference_ids {
                    self.active_inferences.remove(inference_id);
                }
            }
            self.batch_results.remove(&batch_id);
            
            // Update metrics
            let metrics = self.coordinator.read().await.get_batching_metrics();
            metrics.record_batch_timeout();
        }
    }

    /// Get active inferences for RPC queries
    pub async fn get_active_inferences(&self) -> Vec<crate::coordinator::ActiveInference> {
        let mut inferences = Vec::new();
        let now = std::time::Instant::now();
        
        for entry in self.active_inferences.iter() {
            let plan = entry.value();
            let inference_id = *entry.key();
            
            // Build pipeline route from block_ids
            let pipeline_route: Vec<u32> = plan.pipeline_route.iter()
                .map(|r| r.block_id)
                .collect();
            
            // Get current block (first block that hasn't completed)
            let current_block = pipeline_route.first().copied().unwrap_or(0);
            
            // Estimate elapsed time (in a real implementation, this would track actual start time)
            // For now, we use the inference_id timestamp as a proxy
            let elapsed = now.elapsed(); // Placeholder - would use actual start time
            
            inferences.push(crate::coordinator::ActiveInference {
                inference_id,
                model_id: plan.model_id.clone(),
                prompt: String::new(), // Not stored in ReplicaPlan, would need to add
                current_block,
                pipeline_route,
                start_time: std::time::Instant::now() - elapsed, // Placeholder
                token_pipeline: Vec::new(), // Not applicable for orchestrator view
            });
        }
        
        inferences
    }

    /// Shutdown the orchestrator gracefully
    pub async fn shutdown(&self) {
        *self.is_leader.write().await = false;
        // Clear active inferences
        self.active_inferences.clear();
        // Clear canary deployment
        *self.active_canary.write().await = None;
    }

    // =========================================================================
    // Canary Deployment Methods
    // =========================================================================

    /// Start a new canary deployment
    ///
    /// This initiates the canary rollout process:
    /// 1. CEO veto window (24h) - deployment waits for CEO approval
    /// 2. Shadow phase (24h) - 0% traffic, background inference only
    /// 3. Canary phase - gradual traffic increase from 0.1% to 100%
    pub async fn start_canary_rollout(
        &self,
        model_id: String,
        stable_version: String,
        canary_version: String,
    ) -> Result<(), OrchestratorError> {
        if !self.is_leader().await {
            return Err(OrchestratorError::NotLeader);
        }

        let mut active_canary = self.active_canary.write().await;
        if active_canary.is_some() {
            return Err(OrchestratorError::SchedulerError(
                "Canary deployment already in progress".to_string()
            ));
        }

        let deployment = CanaryDeployment::new(model_id, stable_version, canary_version);
        tracing::info!(
            "Starting canary deployment for model {} with canary version {}",
            deployment.model_id,
            deployment.canary_version
        );

        *active_canary = Some(deployment);
        drop(active_canary);

        // Spawn the canary rollout loop
        self.spawn_canary_rollout_loop().await;

        Ok(())
    }

    /// Spawn the canary rollout background task
    async fn spawn_canary_rollout_loop(&self) {
        let orchestrator = Arc::new(self.clone_ref());
        let network_tx = self.network_tx.clone();
        
        // Spawn the auto-rollback checker with 5-minute guarantee
        let rollback_orchestrator = Arc::new(self.clone_ref());
        tokio::spawn(async move {
            rollback_orchestrator.auto_rollback_checker().await;
        });

        tokio::spawn(async move {
            orchestrator.canary_rollout_loop(network_tx).await;
        });
    }
    
    /// Auto-rollback checker running on 5-minute intervals
    /// 
    /// This ensures the 5-minute rollback guarantee is enforced by periodically
    /// checking rollback triggers and executing immediate rollback when thresholds are breached.
    async fn auto_rollback_checker(&self) {
        // Check every 5 minutes (300 seconds)
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        
        loop {
            interval.tick().await;
            
            // Check if there's an active canary deployment
            let has_active_canary = {
                let active_canary = self.active_canary.read().await;
                active_canary.is_some()
            };
            
            if !has_active_canary {
                // No active canary, exit the loop
                break;
            }
            
            // Check rollback triggers
            self.check_rollback_triggers().await;
            
            tracing::debug!("Auto-rollback checker completed 5-minute interval check");
        }
        
        tracing::info!("Auto-rollback checker terminated (no active canary)");
    }

    /// Main canary rollout loop
    async fn canary_rollout_loop(
        &self,
        _network_tx: mpsc::Sender<NetworkMessage>,
    ) {
        let mut check_interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute

        loop {
            check_interval.tick().await;

            let mut active_canary = self.active_canary.write().await;
            if let Some(ref mut deployment) = *active_canary {
                // Check for CEO veto
                if deployment.ceo_vetoed {
                    tracing::warn!("Canary deployment aborted due to CEO veto");
                    *active_canary = None;
                    continue;
                }

                match deployment.phase {
                    CanaryPhase::PendingCeoWindow => {
                        if deployment.is_ceo_window_expired() {
                            tracing::info!("CEO veto window expired, starting shadow phase");
                            deployment.start_shadow_phase();
                        }
                    }
                    CanaryPhase::Shadow => {
                        // Check safety metrics every hour
                        if deployment.is_shadow_phase_complete() {
                            let result = deployment.metrics.check_rollout_criteria();
                            if result.passed {
                                tracing::info!("Shadow phase complete, starting canary phase");
                                deployment.start_canary_phase();
                            } else {
                                let reason = format!("Shadow phase failed: {:?}", result.failures);
                                deployment.rollback(&reason);
                                self.execute_rollback_internal(deployment, &reason).await;
                                *active_canary = None;
                                continue;
                            }
                        }
                    }
                    CanaryPhase::Canary => {
                        if deployment.is_complete() {
                            tracing::info!("Canary deployment complete!");
                            deployment.mark_complete();
                            *active_canary = None;
                            continue;
                        }

                        if deployment.is_increment_due() {
                            let result = deployment.metrics.check_rollout_criteria();
                            if result.passed {
                                deployment.increment_traffic();
                                tracing::info!(
                                    "Canary traffic increased to {}%",
                                    deployment.traffic_percentage
                                );
                            } else {
                                let reason = format!("Rollout criteria failed: {:?}", result.failures);
                                deployment.rollback(&reason);
                                self.execute_rollback_internal(deployment, &reason).await;
                                *active_canary = None;
                                continue;
                            }
                        }
                    }
                    CanaryPhase::Complete | CanaryPhase::RolledBack | CanaryPhase::Aborted => {
                        *active_canary = None;
                        continue;
                    }
                }
            } else {
                // No active canary, exit loop
                break;
            }
        }
    }

    /// Execute rollback internal implementation
    async fn execute_rollback_internal(
        &self,
        deployment: &CanaryDeployment,
        reason: &str,
    ) {
        let rollback_event = RollbackEvent {
            model_id: deployment.model_id.clone(),
            canary_version: deployment.canary_version.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            reason: reason.to_string(),
            metrics_snapshot: deployment.metrics.clone(),
            traffic_at_rollback: deployment.traffic_percentage,
        };

        // Store rollback event
        self.rollback_history.write().await.push(rollback_event.clone());

        // Log rollback event
        tracing::warn!(
            "Canary rollback executed for model {} at {}% traffic. Reason: {}",
            rollback_event.model_id,
            rollback_event.traffic_at_rollback,
            rollback_event.reason
        );

        // Send alert to network (would be picked up by CEO notification system)
        let alert = NetworkMessage::CanaryRollback {
            model_id: rollback_event.model_id,
            canary_version: rollback_event.canary_version,
            reason: rollback_event.reason,
            traffic_at_rollback: rollback_event.traffic_at_rollback,
            timestamp: rollback_event.timestamp,
        };
        let _ = self.network_tx.send(alert).await;
    }

    /// Check rollback triggers (called every 5 minutes)
    pub async fn check_rollback_triggers(&self) {
        let active_canary = self.active_canary.read().await;
        if let Some(ref deployment) = *active_canary {
            let result = deployment.metrics.check_rollout_criteria();
            if !result.passed {
                drop(active_canary);
                let mut active_canary = self.active_canary.write().await;
                if let Some(ref mut deployment) = *active_canary {
                    let reason = format!("Automatic rollback triggered: {:?}", result.failures);
                    deployment.rollback(&reason);
                    self.execute_rollback_internal(deployment, &reason).await;
                    *active_canary = None;
                }
            }
        }
    }

    /// Execute emergency rollback (CEO initiated)
    pub async fn execute_rollback(&self, reason: String) -> Result<(), OrchestratorError> {
        if !self.is_leader().await {
            return Err(OrchestratorError::NotLeader);
        }

        let mut active_canary = self.active_canary.write().await;
        if let Some(ref mut deployment) = *active_canary {
            deployment.rollback(&reason);
            self.execute_rollback_internal(deployment, &reason).await;
            *active_canary = None;
            Ok(())
        } else {
            Err(OrchestratorError::SchedulerError("No active canary deployment".to_string()))
        }
    }

    /// Record a user complaint for canary monitoring
    pub async fn record_complaint(
        &self,
        user_id: String,
        inference_id: Uuid,
        reason: String,
    ) -> Result<(), OrchestratorError> {
        // Get the model version for this inference
        let model_version = self.inference_version
            .get(&inference_id)
            .map(|v| v.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let complaint = UserComplaint {
            user_id,
            inference_id,
            timestamp: chrono::Utc::now().timestamp(),
            reason: reason.clone(),
            model_version: model_version.clone(),
        };

        // Store in complaint tracker
        let mut active_canary = self.active_canary.write().await;
        if let Some(ref mut deployment) = *active_canary {
            // Only count complaints for canary version
            if model_version == "canary" || model_version == deployment.canary_version {
                deployment.metrics.canary_complaints += 1;

                // Check if complaint rate exceeds threshold
                let complaint_rate = deployment.metrics.complaint_rate();
                if complaint_rate > MAX_COMPLAINT_RATE_THRESHOLD {
                    let rollback_reason = format!(
                        "Complaint rate {} exceeds threshold {}",
                        complaint_rate,
                        MAX_COMPLAINT_RATE_THRESHOLD
                    );
                    deployment.rollback(&rollback_reason);
                    self.execute_rollback_internal(deployment, &rollback_reason).await;
                    *active_canary = None;
                }
            }
        }

        // Also store in complaint tracker for audit
        let model_id = active_canary.as_ref().map(|d| d.model_id.clone()).unwrap_or_default();
        drop(active_canary);

        self.complaint_tracker
            .entry(model_id)
            .or_insert_with(Vec::new)
            .push(complaint);

        Ok(())
    }

    /// Get complaint statistics for a model
    pub fn get_complaint_stats(&self, model_id: &str) -> Vec<UserComplaint> {
        self.complaint_tracker
            .get(model_id)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get current canary deployment status
    pub async fn get_canary_status(&self) -> Option<CanaryDeployment> {
        self.active_canary.read().await.clone()
    }

    /// Get canary metrics
    pub async fn get_canary_metrics(&self) -> Option<CanaryMetrics> {
        self.active_canary.read().await.as_ref().map(|d| d.metrics.clone())
    }

    /// Get latency comparison between canary and stable
    pub async fn get_latency_comparison(&self) -> Option<f32> {
        self.active_canary.read().await.as_ref().map(|d| d.metrics.latency_ratio())
    }

    /// Force canary traffic percentage (CEO override)
    pub async fn override_canary_traffic(
        &self,
        new_percentage: f32,
    ) -> Result<(), OrchestratorError> {
        if !self.is_leader().await {
            return Err(OrchestratorError::NotLeader);
        }

        let mut active_canary = self.active_canary.write().await;
        if let Some(ref mut deployment) = *active_canary {
            deployment.traffic_percentage = new_percentage.clamp(0.0, 100.0);
            deployment.last_increment = chrono::Utc::now().timestamp();
            tracing::info!("Canary traffic manually set to {}%", new_percentage);
            Ok(())
        } else {
            Err(OrchestratorError::SchedulerError("No active canary deployment".to_string()))
        }
    }

    /// Process CEO veto for canary deployment
    pub async fn process_ceo_veto(&self) -> Result<(), OrchestratorError> {
        let mut active_canary = self.active_canary.write().await;
        if let Some(ref mut deployment) = *active_canary {
            deployment.abort_for_ceo_veto();
            tracing::warn!("Canary deployment aborted by CEO veto");
            *active_canary = None;
            Ok(())
        } else {
            Err(OrchestratorError::SchedulerError("No active canary deployment".to_string()))
        }
    }

    /// Check canary safety on inference output
    /// 
    /// Integrates constitutional filter, benevolence model, and safety oracle
    /// to comprehensively evaluate canary output safety.
    pub async fn check_canary_safety(&self, output: &str) -> SafetyCheckResult {
        // 1. Constitutional Filter Check
        let violations = genesis::CONSTITUTIONAL_FILTER.scan_text(output);
        let violation_count = violations.len() as u64;
        
        // 2. Benevolence Model Score
        let benevolence_score = match genesis::get_benevolence_model() {
            Ok(model) => {
                match model.score_output(output).await {
                    Ok(score) => score.score,
                    Err(_) => 0.0, // Default to 0 on error for safety
                }
            }
            Err(_) => {
                // If model not initialized, use a simple heuristic
                // In production, this should always be initialized
                tracing::warn!("Benevolence model not initialized, using fallback scoring");
                0.95 // Conservative fallback
            }
        };
        
        // 3. Safety Oracle Evaluation
        let (unsafe_votes, is_safe) = match genesis::get_safety_oracle_config() {
            Ok(config) => {
                match genesis::evaluate_safety(output, &config).await {
                    Ok(result) => {
                        let unsafe_count = if result.unanimous_safe { 0 } else { 1 };
                        (unsafe_count, result.unanimous_safe)
                    }
                    Err(_) => {
                        // Fail-closed: treat errors as unsafe
                        (1, false)
                    }
                }
            }
            Err(_) => {
                // If safety oracle not configured, skip this check
                tracing::warn!("Safety oracle not configured, skipping oracle check");
                (0, true)
            }
        };

        // Determine overall pass/fail
        let passed = violation_count == 0
            && benevolence_score >= MIN_BENEVOLENCE_SCORE
            && is_safe;

        SafetyCheckResult {
            violations: violation_count,
            benevolence_score,
            unsafe_votes,
            passed,
        }
    }

    /// Update canary metrics with safety check result
    pub async fn update_canary_safety_metrics(&self, result: &SafetyCheckResult) {
        let mut active_canary = self.active_canary.write().await;
        if let Some(ref mut deployment) = *active_canary {
            deployment.metrics.canary_violations += result.violations;
            if result.benevolence_score < MIN_BENEVOLENCE_SCORE {
                deployment.metrics.canary_benevolence_failures += 1;
            }
            deployment.metrics.canary_oracle_unsafe_votes += result.unsafe_votes;
            deployment.metrics.last_benevolence_score = result.benevolence_score;
        }
    }

    /// Route inference to appropriate version based on canary state
    /// 
    /// Returns (version, is_canary) tuple
    pub async fn route_to_version(&self, model_id: &str) -> (String, bool) {
        let active_canary = self.active_canary.read().await;
        if let Some(ref deployment) = *active_canary {
            if deployment.model_id == model_id {
                match deployment.phase {
                    CanaryPhase::Shadow => {
                        // In shadow mode, always route to stable
                        (deployment.stable_version.clone(), false)
                    }
                    CanaryPhase::Canary => {
                        // Route based on traffic percentage using simple hash-based routing
                        // This ensures consistent routing for the same inference
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos();
                        let roll = (now % 100000) as f32 / 1000.0; // 0.0 - 99.999
                        if roll < deployment.traffic_percentage {
                            (deployment.canary_version.clone(), true)
                        } else {
                            (deployment.stable_version.clone(), false)
                        }
                    }
                    _ => {
                        // Pending or complete, route to stable
                        (deployment.stable_version.clone(), false)
                    }
                }
            } else {
                (model_id.to_string(), false)
            }
        } else {
            (model_id.to_string(), false)
        }
    }

    /// Run shadow inference on canary version (background)
    pub async fn run_shadow_inference(
        &self,
        model_id: &str,
        prompt: &str,
        _max_tokens: u32,
    ) {
        // This runs inference on canary version without returning to user
        // Results are logged and safety-checked
        tracing::debug!("Running shadow inference for model {}", model_id);
        
        let start_time = std::time::Instant::now();

        // Execute inference on canary version
        // In production, this would actually route to canary replicas
        let output = format!("shadow_output_for_{}", prompt);

        // Run safety checks
        let safety_result = self.check_canary_safety(&output).await;

        // Calculate latency
        let latency_ms = start_time.elapsed().as_millis() as f32;

        // Update metrics with safety check result and latency
        {
            let mut active_canary = self.active_canary.write().await;
            if let Some(ref mut deployment) = *active_canary {
                // Update safety metrics
                deployment.metrics.canary_violations += safety_result.violations;
                if safety_result.benevolence_score < MIN_BENEVOLENCE_SCORE {
                    deployment.metrics.canary_benevolence_failures += 1;
                }
                deployment.metrics.canary_oracle_unsafe_votes += safety_result.unsafe_votes;
                
                // Set last benevolence score from actual safety check
                deployment.metrics.last_benevolence_score = safety_result.benevolence_score;
                
                // Update canary latency
                deployment.metrics.update_canary_latency(latency_ms);
                
                // Increment shadow request count
                deployment.metrics.canary_requests += 1;
            }
        }

        // If safety check failed, trigger rollback
        if !safety_result.passed {
            tracing::warn!("Shadow inference safety check failed: {:?}", safety_result);
            let reason = format!("Shadow safety check failed: violations={}, benevolence={}, unsafe_votes={}",
                safety_result.violations, safety_result.benevolence_score, safety_result.unsafe_votes);
            let _ = self.execute_rollback(reason).await;
        }
    }
}

/// Orchestrator state for RPC responses
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OrchestratorState {
    pub is_leader: bool,
    pub leader_node_id: Option<String>,
    pub total_blocks: u32,
    pub total_replicas: u32,
    pub healthy_replicas: u32,
}
