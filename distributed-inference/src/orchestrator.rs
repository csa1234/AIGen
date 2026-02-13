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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use libp2p::PeerId;
use tokio::sync::{mpsc, RwLock};
use thiserror::Error;
use uuid::Uuid;

use crate::block_assignment::{BlockAssignment, LayerBlock, StaticBlockConfig};
use crate::pipeline_message::{InferenceStart, PipelineMessage};
use crate::replica_manager::{BlockReplica, ReplicaManager, ReplicaHealth, HealthStatus};
use crate::health_monitor::HealthMonitor;
use crate::route_selector::RouteSelector;
use crate::coordinator::{StaticPipelineCoordinator, BatchConfig, InferenceRequest, BatchedInference};
use distributed_compute::scheduler::DynamicScheduler;
use distributed_compute::state::GlobalState;
use consensus::validator::ValidatorRegistry;
use network::protocol::{NetworkMessage, NodeCapabilities};

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
    let public_key = libp2p::identity::PublicKey::from_protobuf_encoding(&token.public_key)
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
#[derive(Clone, Debug)]
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

        // If batching is enabled, enqueue the request for batch processing
        if self.config.enable_batching {
            let inference_id = Uuid::new_v4();
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
                model_id: model_id.clone(),
                pipeline_route,
                estimated_latency_ms: total_latency_ms,
            };

            // Store active inference
            self.active_inferences.insert(inference_id, plan.clone());

            return Ok(plan);
        }

        // Non-batched path: immediate execution
        let inference_id = Uuid::new_v4();
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
            model_id: model_id.clone(),
            pipeline_route,
            estimated_latency_ms: total_latency_ms,
        };

        // Store active inference
        self.active_inferences.insert(inference_id, plan.clone());

        // Send inference start to first block
        if let Some(first_replica) = plan.pipeline_route.first() {
            let inference_start = InferenceStart {
                inference_id,
                model_id,
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
            let batch_id_bytes = chunk.tensor.metadata.layer_range.1.to_le_bytes();
            // This is a simplified extraction - in production, batch_id would be properly serialized
            None // Placeholder - requires proper encoding in block executor
        } else {
            None
        }
    }
    
    /// Split batched output by [BATCH_SEP] delimiter
    fn split_batched_output(chunk: &crate::pipeline_message::ActivationChunk) -> Vec<String> {
        // Decompress tensor data and split by separator
        // This is a placeholder - actual implementation depends on output encoding
        vec![]
    }
    
    /// Extract single output from chunk
    fn extract_output_from_chunk(chunk: &crate::pipeline_message::ActivationChunk) -> String {
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
        // Remove from active inferences
        if let Some((_, plan)) = self.active_inferences.remove(&inference_id) {
            tracing::info!(
                "Completed inference {} with output length {}",
                inference_id,
                output.len()
            );
        }
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
