// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::sync::atomic::Ordering;
use std::sync::Arc;

use blockchain_core::types::{validate_address, BlockHash};
use blockchain_core::Blockchain;
use genesis;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use tokio::sync::Mutex;
use tracing::info;

use crate::rpc::types::{
    BalanceResponse, BlockResponse, ChainInfoResponse, HealthResponse, NodeInfoResponse, RpcError,
    SubmitTransactionResponse, TransactionResponse, DcsStateResponse, SubmitInferenceRequest, TaskPlanResponse, TaskStatusResponse,
    NodeEarningsResponse, TaskRewardResponse, PendingRewardsResponse, StorageRewardEstimateResponse, RewardHistoryResponse,
    PendingReward, RewardHistoryEntry,
    // NEW: VRAM Pool types
    VramPoolStatsResponse, NodeMetricsResponse, RewardLeaderboardEntry, RewardLeaderboardResponse,
    // NEW: Orchestrator types
    OrchestratorStateResponse, BlockAssignmentsResponse, BlockAssignmentInfo, ReplicaInfo,
    ReplicaHealthResponse, PipelineInferenceRequest, PipelineInferenceResponse,
    // NEW: Distributed Inference types
    DistributedInferenceStatsResponse, ActivePipelineResponse, BlockLatencyMetrics, CompressionStatsResponse,
};
use distributed_compute::scheduler::DynamicScheduler;
use distributed_compute::state::GlobalState;
use distributed_compute::task::InferenceTask;
use uuid::Uuid;

#[rpc(server)]
pub trait PublicRpc {
    #[method(name = "getBlock")]
    async fn get_block(
        &self,
        block_height: Option<u64>,
        block_hash: Option<String>,
    ) -> Result<Option<BlockResponse>, RpcError>;

    #[method(name = "getBalance")]
    async fn get_balance(&self, address: String) -> Result<BalanceResponse, RpcError>;

    #[method(name = "submitTransaction")]
    async fn submit_transaction(
        &self,
        transaction: TransactionResponse,
    ) -> Result<SubmitTransactionResponse, RpcError>;

    #[method(name = "getChainInfo")]
    async fn get_chain_info(&self) -> Result<ChainInfoResponse, RpcError>;

    #[method(name = "getNodeInfo")]
    async fn get_node_info(&self) -> Result<NodeInfoResponse, RpcError>;

    #[method(name = "health")]
    async fn health(&self) -> Result<HealthResponse, RpcError>;

    #[method(name = "getPendingTransactions")]
    async fn get_pending_transactions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionResponse>, RpcError>;

    #[method(name = "getDcsState")]
    async fn get_dcs_state(&self) -> Result<DcsStateResponse, RpcError>;

    #[method(name = "submitInferenceTask")]
    async fn submit_inference_task(&self, request: SubmitInferenceRequest) -> Result<TaskPlanResponse, RpcError>;

    #[method(name = "getTaskStatus")]
    async fn get_task_status(&self, task_id: String) -> Result<TaskStatusResponse, RpcError>;

    // Orchestrator Methods
    #[method(name = "getOrchestratorState")]
    async fn get_orchestrator_state(&self) -> Result<OrchestratorStateResponse, RpcError>;

    #[method(name = "getBlockAssignments")]
    async fn get_block_assignments(&self, model_id: Option<String>) -> Result<BlockAssignmentsResponse, RpcError>;

    #[method(name = "getReplicaHealth")]
    async fn get_replica_health(&self, block_id: Option<u32>) -> Result<Vec<ReplicaHealthResponse>, RpcError>;

    #[method(name = "submitPipelineInference")]
    async fn submit_pipeline_inference(&self, request: PipelineInferenceRequest) -> Result<PipelineInferenceResponse, RpcError>;

    // NEW: Distributed Inference Metrics Methods
    #[method(name = "getDistributedInferenceStats")]
    async fn get_distributed_inference_stats(&self) -> Result<DistributedInferenceStatsResponse, RpcError>;

    #[method(name = "getActivePipelines")]
    async fn get_active_pipelines(&self, limit: Option<u32>) -> Result<Vec<ActivePipelineResponse>, RpcError>;

    #[method(name = "getBlockLatencyMetrics")]
    async fn get_block_latency_metrics(&self, block_id: Option<u32>) -> Result<Vec<BlockLatencyMetrics>, RpcError>;

    #[method(name = "getCompressionStats")]
    async fn get_compression_stats(&self) -> Result<CompressionStatsResponse, RpcError>;
}

#[derive(Clone)]
pub struct RpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
    pub network_metrics: network::metrics::SharedNetworkMetrics,
    pub start_time: std::sync::Arc<std::time::Instant>,
    pub dcs: Option<Arc<DynamicScheduler>>,
    pub dcs_state: Option<Arc<GlobalState>>,
    pub orchestrator: Option<Arc<distributed_inference::OrchestratorNode>>,
}

impl RpcMethods {
    pub fn new(
        blockchain: Arc<Mutex<Blockchain>>,
        tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
        network_metrics: network::metrics::SharedNetworkMetrics,
        dcs: Option<Arc<DynamicScheduler>>,
        dcs_state: Option<Arc<GlobalState>>,
        orchestrator: Option<Arc<distributed_inference::OrchestratorNode>>,
    ) -> Self {
        Self {
            blockchain,
            tx_broadcast,
            network_metrics,
            start_time: std::sync::Arc::new(std::time::Instant::now()),
            dcs,
            dcs_state,
            orchestrator,
        }
    }
}

#[async_trait]
impl PublicRpcServer for RpcMethods {
    async fn get_block(
        &self,
        block_height: Option<u64>,
        block_hash: Option<String>,
    ) -> Result<Option<BlockResponse>, RpcError> {
        let bc = self.blockchain.lock().await;

        let block = if let Some(h) = block_height {
            bc.blocks.get(h as usize)
        } else if let Some(hs) = block_hash {
            let parsed: BlockHash = hs
                .parse()
                .map_err(|e| RpcError::InvalidParams(format!("invalid block_hash: {e}")))?;
            bc.blocks.iter().find(|b| b.block_hash == parsed)
        } else {
            return Err(RpcError::InvalidParams(
                "must provide block_height or block_hash".to_string(),
            ));
        };

        let Some(block) = block else {
            return Err(RpcError::BlockNotFound);
        };

        Ok(Some(BlockResponse::from_block(block)))
    }

    async fn get_balance(&self, address: String) -> Result<BalanceResponse, RpcError> {
        validate_address(&address)
            .map_err(|e| RpcError::InvalidParams(format!("invalid address: {e}")))?;

        let bc = self.blockchain.lock().await;
        let bal = bc.get_balance(&address);
        let account = bc.get_account_state(&address);

        Ok(BalanceResponse {
            address,
            balance: bal,
            nonce: account.nonce.value(),
        })
    }

    async fn submit_transaction(
        &self,
        transaction: TransactionResponse,
    ) -> Result<SubmitTransactionResponse, RpcError> {
        use blockchain_core::crypto::PublicKey;
        use ed25519_dalek::VerifyingKey;

        let pk_hex = transaction
            .sender_public_key
            .clone()
            .ok_or_else(|| RpcError::InvalidParams("sender_public_key is required".to_string()))?;
        let pk_bytes = crate::rpc::types::parse_hex_bytes(&pk_hex)?;
        if pk_bytes.len() != 32 {
            return Err(RpcError::InvalidParams(
                "sender_public_key must be 32 bytes".to_string(),
            ));
        }
        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(&pk_bytes);
        let public_key = VerifyingKey::from_bytes(&pk_arr)
            .map_err(|_| RpcError::InvalidParams("invalid sender_public_key".to_string()))?;
        let public_key: PublicKey = public_key;

        let tx = transaction.into_tx()?;

        let derived_addr = blockchain_core::derive_address_from_pubkey(&public_key);
        if !derived_addr.eq_ignore_ascii_case(&tx.sender) {
            return Err(RpcError::InvalidParams(
                "sender does not match sender_public_key".to_string(),
            ));
        }

        tx.verify_with_pubkey(&public_key)
            .map_err(|_| RpcError::InvalidParams("invalid transaction signature".to_string()))?;
        let tx_hash = tx.tx_hash;

        let mut bc = self.blockchain.lock().await;
        bc.add_transaction_to_pool(tx.clone())
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        let _ = self.tx_broadcast.send(tx);

        info!(tx_hash = %tx_hash, "transaction submitted");

        Ok(SubmitTransactionResponse {
            tx_hash: format!("0x{}", hex::encode(tx_hash.0)),
            status: "accepted".to_string(),
        })
    }

    async fn get_chain_info(&self) -> Result<ChainInfoResponse, RpcError> {
        let bc = self.blockchain.lock().await;

        let height = bc.blocks.len().saturating_sub(1) as u64;
        let latest = bc
            .blocks
            .last()
            .map(|b| b.block_hash)
            .unwrap_or(blockchain_core::types::BlockHash([0u8; 32]));

        let snapshot = bc.state.snapshot();
        let total_supply = snapshot
            .accounts
            .values()
            .map(|a| a.balance.amount().value())
            .sum::<u64>();

        Ok(ChainInfoResponse {
            chain_id: bc.chain_id.value(),
            height,
            latest_block_hash: format!("0x{}", hex::encode(latest.0)),
            shutdown_status: bc.shutdown,
            total_supply,
        })
    }

    async fn get_node_info(&self) -> Result<NodeInfoResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        Ok(NodeInfoResponse {
            network_magic: bc.genesis_config.network_magic,
            ceo_public_key_hex: genesis::config::CEO_PUBLIC_KEY_HEX.to_string(),
        })
    }

    async fn health(&self) -> Result<HealthResponse, RpcError> {
        let uptime_seconds = self.start_time.elapsed().as_secs();

        let bc = self.blockchain.lock().await;

        let shutdown_active = genesis::is_shutdown() || bc.shutdown;
        let latest_block_time = bc.blocks.last().map(|b| b.header.timestamp.0).unwrap_or(0);

        let now_ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => 0,
        };

        let peer_count: u64 = self.network_metrics.peers_connected.load(Ordering::Relaxed);
        let sync_status = if bc.blocks.len() <= 1 {
            "syncing"
        } else {
            "synced"
        };

        let status = if shutdown_active {
            "unhealthy"
        } else if now_ts > 0
            && latest_block_time > 0
            && now_ts.saturating_sub(latest_block_time) > 60
        {
            "degraded"
        } else {
            "healthy"
        };

        Ok(HealthResponse {
            status: status.to_string(),
            uptime_seconds,
            peer_count,
            sync_status: sync_status.to_string(),
            latest_block_time,
            shutdown_active,
        })
    }

    async fn get_pending_transactions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionResponse>, RpcError> {
        let limit = limit.unwrap_or(50).min(500);
        let bc = self.blockchain.lock().await;
        let txs = bc.get_pending_transactions(limit);
        Ok(txs.iter().map(TransactionResponse::from_tx).collect())
    }

    async fn get_dcs_state(&self) -> Result<DcsStateResponse, RpcError> {
        if let (Some(dcs), Some(state)) = (&self.dcs, &self.dcs_state) {
            let is_leader = dcs.is_leader().await;
            let leader_id = if is_leader { Some("local".to_string()) } else { None };
            let active_nodes = state.nodes.len() as u32;
            let known_fragments = state.fragments.len() as u32;
            
            Ok(DcsStateResponse {
                is_leader,
                leader_id,
                active_nodes,
                known_fragments,
            })
        } else {
            Err(RpcError::Internal("DCS not initialized".to_string()))
        }
    }

    async fn submit_inference_task(&self, request: SubmitInferenceRequest) -> Result<TaskPlanResponse, RpcError> {
        let dcs = self.dcs.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        let task = InferenceTask {
            inference_id: Uuid::new_v4(),
            model_id: request.model_id,
            prompt: request.prompt,
            max_tokens: request.max_tokens,
            user_id: request.user_id,
        };
        
        let plan = dcs.schedule_inference(task).await
            .map_err(|e| RpcError::Internal(e.to_string()))?;
            
        Ok(TaskPlanResponse {
            inference_id: plan.inference_id.to_string(),
            task_count: plan.tasks.len() as u32,
            pipeline_order: plan.pipeline_order,
            status: "scheduled".to_string(),
        })
    }

    async fn get_task_status(&self, task_id: String) -> Result<TaskStatusResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        let uuid = Uuid::parse_str(&task_id).map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        let task = state.active_tasks.get(&uuid).ok_or(RpcError::InvalidParams("task not found".to_string()))?;
        
        Ok(TaskStatusResponse {
            task_id: task.task_id.to_string(),
            status: format!("{:?}", task.status),
            assigned_node: task.assigned_node.clone(),
        })
    }

    // Orchestrator method implementations
    async fn get_orchestrator_state(&self) -> Result<OrchestratorStateResponse, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
        
        let state = orchestrator.get_orchestrator_state().await;
        
        Ok(OrchestratorStateResponse {
            is_leader: state.is_leader,
            leader_node_id: state.leader_node_id,
            total_blocks: state.total_blocks,
            total_replicas: state.total_replicas,
            healthy_replicas: state.healthy_replicas,
        })
    }

    async fn get_block_assignments(&self, model_id: Option<String>) -> Result<BlockAssignmentsResponse, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
        
        let assignments = orchestrator.get_block_assignments().await;
        
        let mut blocks = Vec::new();
        for (block_id, replicas) in assignments {
            // Get layer range from block_assignment
            let layer_range = orchestrator.block_assignment.read().await
                .get_block(block_id)
                .map(|b| b.layer_range)
                .unwrap_or((0, 0));
            
            let replica_infos: Vec<ReplicaInfo> = replicas.into_iter().map(|r| {
                // Get health status
                let health = orchestrator.replica_manager.get_replica_health(&r.node_id);
                let status = health.map(|h| h.status.to_string()).unwrap_or_else(|| "unknown".to_string());
                
                ReplicaInfo {
                    node_id: r.node_id,
                    status,
                    load_score: r.load_score,
                    last_heartbeat: r.last_heartbeat,
                }
            }).collect();
            
            blocks.push(BlockAssignmentInfo {
                block_id,
                layer_range,
                replicas: replica_infos,
            });
        }
        
        // Sort by block_id
        blocks.sort_by_key(|b| b.block_id);
        
        Ok(BlockAssignmentsResponse {
            model_id: model_id.unwrap_or_else(|| "default".to_string()),
            blocks,
        })
    }

    async fn get_replica_health(&self, block_id: Option<u32>) -> Result<Vec<ReplicaHealthResponse>, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
        
        let health_statuses = orchestrator.get_replica_health(block_id);
        
        let responses: Vec<ReplicaHealthResponse> = health_statuses.into_iter().map(|h| {
            // Get block_id for this replica
            let block = orchestrator.replica_manager.get_block_for_node(&h.node_id).unwrap_or(0);
            
            ReplicaHealthResponse {
                node_id: h.node_id,
                block_id: block,
                status: h.status.to_string(),
                consecutive_misses: h.consecutive_misses,
                last_seen: h.last_seen,
            }
        }).collect();
        
        Ok(responses)
    }

    async fn submit_pipeline_inference(&self, request: PipelineInferenceRequest) -> Result<PipelineInferenceResponse, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
        
        let plan = orchestrator.assign_inference_to_replicas(
            request.model_id,
            request.prompt,
            request.max_tokens,
        ).await.map_err(|e| RpcError::Internal(e.to_string()))?;
        
        let pipeline_route: Vec<String> = plan.pipeline_route.iter().map(|r| r.node_id.clone()).collect();
        
        Ok(PipelineInferenceResponse {
            inference_id: plan.inference_id.to_string(),
            pipeline_route,
            estimated_latency_ms: plan.estimated_latency_ms,
        })
    }

    // NEW: Distributed Inference Metrics Implementations
    async fn get_distributed_inference_stats(&self) -> Result<DistributedInferenceStatsResponse, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;

        // Get global metrics from network metrics
        let global_metrics = self.network_metrics.get_global_metrics();
        
        // Get checkpoint and failover stats
        let checkpoint_stats = self.network_metrics.get_checkpoint_stats();
        let failover_stats = self.network_metrics.get_failover_stats();
        
        // Count healthy replicas
        let total_replicas = orchestrator.replica_manager.get_all_replicas().len();
        let healthy_replicas = orchestrator.replica_manager.healthy_replica_count();
        
        // Get compression stats from tensor transport
        let (_, _, avg_compression_ratio) = distributed_inference::tensor_transport::TensorTransport::get_compression_stats();
        
        Ok(DistributedInferenceStatsResponse {
            active_pipelines: global_metrics.total_inferences as u32,
            total_inferences_completed: global_metrics.total_tasks,
            avg_pipeline_latency_ms: global_metrics.get_latency() as f64,
            avg_throughput_tasks_per_sec: global_metrics.avg_throughput_tasks_per_sec.load(Ordering::Relaxed) as f64 / 100.0,
            avg_compression_ratio,
            total_blocks: total_replicas as u32,
            healthy_blocks: healthy_replicas as u32,
        })
    }

    async fn get_active_pipelines(&self, limit: Option<u32>) -> Result<Vec<ActivePipelineResponse>, RpcError> {
        let orchestrator = self.orchestrator.as_ref()
            .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
        
        let limit = limit.unwrap_or(50).min(100) as usize;
        
        // Get active inferences from orchestrator
        let active_inferences = orchestrator.get_active_inferences().await;
        
        let mut pipelines = Vec::new();
        for inference in active_inferences.into_iter().take(limit) {
            let elapsed_ms = inference.start_time.elapsed().as_millis() as u64;
            
            // Estimate remaining time based on pipeline progress
            let total_blocks = inference.pipeline_route.len() as u32;
            let current_block_idx = inference.pipeline_route.iter()
                .position(|&b| b == inference.current_block)
                .unwrap_or(0) as u32;
            let progress_pct = if total_blocks > 0 {
                (current_block_idx + 1) as f32 / total_blocks as f32
            } else {
                0.0
            };
            
            // Rough estimate: if we've taken elapsed_ms for (progress_pct) of the work,
            // remaining should be proportional
            let estimated_remaining_ms = if progress_pct > 0.0 {
                ((elapsed_ms as f32 / progress_pct) - elapsed_ms as f32) as u64
            } else {
                elapsed_ms * total_blocks as u64 // Fallback: assume each block takes same time
            };
            
            pipelines.push(ActivePipelineResponse {
                inference_id: inference.inference_id.to_string(),
                model_id: inference.model_id,
                pipeline_route: inference.pipeline_route.iter()
                    .filter_map(|block_id| {
                        orchestrator.replica_manager.get_healthy_replicas(*block_id)
                            .first()
                            .map(|r| r.node_id.clone())
                    })
                    .collect(),
                current_block: inference.current_block,
                elapsed_ms,
                estimated_remaining_ms,
            });
        }
        
        Ok(pipelines)
    }

    async fn get_block_latency_metrics(&self, block_id: Option<u32>) -> Result<Vec<BlockLatencyMetrics>, RpcError> {
        // Get all node metrics from network metrics
        let node_metrics = self.network_metrics.get_all_node_metrics();
        
        let mut metrics = Vec::new();
        
        if let Some(bid) = block_id {
            // Get metrics for specific block
            // Find nodes that are assigned to this block
            let orchestrator = self.orchestrator.as_ref()
                .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
            
            let replicas = orchestrator.replica_manager.get_healthy_replicas(bid);
            for replica in replicas {
                if let Some(node_metric) = node_metrics.iter()
                    .find(|m| m.node_id == replica.node_id) {
                    let tasks_completed = node_metric.tasks_completed.load(Ordering::Relaxed);
                    let tasks_failed = node_metric.tasks_failed.load(Ordering::Relaxed);
                    let avg_compute_time = if tasks_completed > 0 {
                        node_metric.total_compute_time_ms.load(Ordering::Relaxed) as f64 / tasks_completed as f64
                    } else {
                        0.0
                    };
                    
                    metrics.push(BlockLatencyMetrics {
                        block_id: bid,
                        avg_compute_time_ms: avg_compute_time,
                        avg_transfer_time_ms: node_metric.get_latency() as f64,
                        tasks_completed,
                        tasks_failed,
                    });
                }
            }
        } else {
            // Get metrics for all blocks
            // Aggregate by block assignment
            let orchestrator = self.orchestrator.as_ref()
                .ok_or(RpcError::Internal("Orchestrator not initialized".to_string()))?;
            
            // Get all block IDs
            let all_replicas = orchestrator.replica_manager.get_all_replicas();
            let block_ids: std::collections::HashSet<u32> = all_replicas.iter()
                .map(|r| r.block_id)
                .collect();
            
            for bid in block_ids {
                let replicas = orchestrator.replica_manager.get_healthy_replicas(bid);
                let mut total_compute_time = 0.0;
                let mut total_transfer_time = 0.0;
                let mut total_tasks_completed = 0u64;
                let mut total_tasks_failed = 0u64;
                let mut replica_count = 0u32;
                
                for replica in replicas {
                    if let Some(node_metric) = node_metrics.iter()
                        .find(|m| m.node_id == replica.node_id) {
                        let tasks_completed = node_metric.tasks_completed.load(Ordering::Relaxed);
                        let tasks_failed = node_metric.tasks_failed.load(Ordering::Relaxed);
                        
                        if tasks_completed > 0 {
                            total_compute_time += node_metric.total_compute_time_ms.load(Ordering::Relaxed) as f64 / tasks_completed as f64;
                        }
                        total_transfer_time += node_metric.get_latency() as f64;
                        total_tasks_completed += tasks_completed;
                        total_tasks_failed += tasks_failed;
                        replica_count += 1;
                    }
                }
                
                if replica_count > 0 {
                    metrics.push(BlockLatencyMetrics {
                        block_id: bid,
                        avg_compute_time_ms: total_compute_time / replica_count as f64,
                        avg_transfer_time_ms: total_transfer_time / replica_count as f64,
                        tasks_completed: total_tasks_completed,
                        tasks_failed: total_tasks_failed,
                    });
                }
            }
        }
        
        // Sort by block_id
        metrics.sort_by_key(|m| m.block_id);
        
        Ok(metrics)
    }

    async fn get_compression_stats(&self) -> Result<CompressionStatsResponse, RpcError> {
        let (total_tensors, bytes_saved, avg_ratio) = 
            distributed_inference::tensor_transport::TensorTransport::get_compression_stats();
        
        Ok(CompressionStatsResponse {
            total_tensors_compressed: total_tensors,
            avg_compression_ratio: avg_ratio,
            total_bytes_saved: bytes_saved,
            quantization_enabled: true, // We have quantization enabled by default
        })
    }
}

#[rpc(server)]
pub trait RewardRpc {
    #[method(name = "reward_getNodeEarnings")]
    async fn get_node_earnings(&self, node_id: String) -> Result<NodeEarningsResponse, RpcError>;
    
    #[method(name = "reward_getTaskReward")]
    async fn get_task_reward(&self, task_id: String) -> Result<TaskRewardResponse, RpcError>;
    
    #[method(name = "reward_listPendingRewards")]
    async fn list_pending_rewards(&self, node_id: String) -> Result<PendingRewardsResponse, RpcError>;
    
    #[method(name = "reward_getStorageRewardEstimate")]
    async fn get_storage_reward_estimate(&self, node_id: String) -> Result<StorageRewardEstimateResponse, RpcError>;
    
    #[method(name = "reward_getRewardHistory")]
    async fn get_reward_history(&self, node_id: String, limit: Option<u32>) -> Result<RewardHistoryResponse, RpcError>;
    
    // NEW: VRAM Pool and Metrics methods
    #[method(name = "getVramPoolStats")]
    async fn get_vram_pool_stats(&self) -> Result<VramPoolStatsResponse, RpcError>;
    
    #[method(name = "getNodeMetrics")]
    async fn get_node_metrics(&self, node_id: Option<String>) -> Result<Vec<NodeMetricsResponse>, RpcError>;
    
    #[method(name = "getRewardLeaderboard")]
    async fn get_reward_leaderboard(&self, limit: Option<u32>) -> Result<RewardLeaderboardResponse, RpcError>;
}

#[rpc(server)]
pub trait StakingGovernanceRpc {
    // Staking methods
    #[method(name = "staking_listStakes")]
    async fn list_stakes(&self, role_filter: Option<String>) -> Result<StakeListResponse, RpcError>;
    
    #[method(name = "staking_getStake")]
    async fn get_stake(&self, address: String) -> Result<Option<StakeInfoResponse>, RpcError>;
    
    #[method(name = "staking_submitStakeTx")]
    async fn submit_stake_tx(&self, request: SubmitStakeTxRequest) -> Result<StakeTxResponse, RpcError>;
    
    #[method(name = "staking_submitUnstakeTx")]
    async fn submit_unstake_tx(&self, request: SubmitUnstakeTxRequest) -> Result<StakeTxResponse, RpcError>;
    
    #[method(name = "staking_submitClaimStakeTx")]
    async fn submit_claim_stake_tx(&self, request: SubmitClaimStakeTxRequest) -> Result<StakeTxResponse, RpcError>;
    
    // Governance methods
    #[method(name = "governance_listProposals")]
    async fn list_proposals(&self, status_filter: Option<String>) -> Result<ProposalListResponse, RpcError>;
    
    #[method(name = "governance_getProposal")]
    async fn get_proposal(&self, proposal_id: String) -> Result<Option<ProposalResponse>, RpcError>;
    
    #[method(name = "governance_listVotes")]
    async fn list_votes(&self, proposal_id: String) -> Result<VoteListResponse, RpcError>;
    
    #[method(name = "governance_submitVote")]
    async fn submit_vote(&self, request: SubmitVoteRequest) -> Result<SubmitVoteResponse, RpcError>;
}

pub struct RewardRpcImpl {
    pub dcs: Option<Arc<DynamicScheduler>>,
    pub dcs_state: Option<Arc<GlobalState>>,
}

impl RewardRpcImpl {
    pub fn new(
        dcs: Option<Arc<DynamicScheduler>>,
        dcs_state: Option<Arc<GlobalState>>,
    ) -> Self {
        Self { dcs, dcs_state }
    }
}

#[async_trait]
impl RewardRpcServer for RewardRpcImpl {
    async fn get_node_earnings(&self, node_id: String) -> Result<NodeEarningsResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        let record = state.get_node_earnings(&node_id);
        
        if let Some(r) = record {
            Ok(NodeEarningsResponse {
                node_id: r.node_id,
                total_earned: r.total_earned.value(),
                compute_rewards: r.compute_rewards.value(),
                storage_rewards: r.storage_rewards.value(),
                tasks_completed: r.tasks_completed,
                fragments_hosted: r.fragments_hosted,
                last_reward_timestamp: r.last_reward_timestamp,
            })
        } else {
            // Return empty record for new nodes
            Ok(NodeEarningsResponse {
                node_id: node_id.clone(),
                total_earned: 0,
                compute_rewards: 0,
                storage_rewards: 0,
                tasks_completed: 0,
                fragments_hosted: 0,
                last_reward_timestamp: 0,
            })
        }
    }
    
    async fn get_task_reward(&self, task_id: String) -> Result<TaskRewardResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        let uuid = Uuid::parse_str(&task_id).map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        // Check if task exists in active tasks
        if let Some(task) = state.active_tasks.get(&uuid) {
            let status = match task.status {
                distributed_compute::task::TaskStatus::Pending => "pending",
                distributed_compute::task::TaskStatus::Running => "running",
                distributed_compute::task::TaskStatus::Completed => "completed",
                distributed_compute::task::TaskStatus::Failed => "failed",
            };
            
            // Look up reward history for this task to get actual reward data
            let mut reward_amount = 0u64;
            let mut proof_verified = false;
            let mut compute_time_ms = 0u64;
            
            // Search reward history for this task
            for entry_ref in state.reward_history.iter() {
                let history = entry_ref.value();
                for reward_entry in history {
                    if let Some(ref entry_task_id) = reward_entry.task_id {
                        if entry_task_id == &task_id && reward_entry.reward_type == "compute" {
                            reward_amount = reward_entry.amount.value();
                            proof_verified = reward_entry.proof_verified;
                            // Use timestamp as proxy for compute time if available
                            if compute_time_ms == 0 {
                                compute_time_ms = 1000; // Default placeholder
                            }
                        }
                    }
                }
            }
            
            return Ok(TaskRewardResponse {
                task_id: task.task_id.to_string(),
                reward_amount,
                status: status.to_string(),
                proof_verified,
                compute_time_ms,
                fragment_count: task.required_fragments.len() as u32,
            });
        }
        
        // Task not found in active tasks - check if completed and has reward history
        for entry_ref in state.reward_history.iter() {
            let history = entry_ref.value();
            for reward_entry in history {
                if let Some(ref entry_task_id) = reward_entry.task_id {
                    if entry_task_id == &task_id {
                        return Ok(TaskRewardResponse {
                            task_id: task_id.clone(),
                            reward_amount: reward_entry.amount.value(),
                            status: "completed".to_string(),
                            proof_verified: reward_entry.proof_verified,
                            compute_time_ms: 1000, // Placeholder - would come from proof
                            fragment_count: 0, // Not stored in history
                        });
                    }
                }
            }
        }
        
        Err(RpcError::InvalidParams("task not found".to_string()))
    }
    
    async fn list_pending_rewards(&self, node_id: String) -> Result<PendingRewardsResponse, RpcError> {
        let dcs = self.dcs.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        let pending = dcs.get_pending_reward(&node_id).await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        let mut rewards = Vec::new();
        if pending.value() > 0 {
            rewards.push(PendingReward {
                amount: pending.value(),
                reason: "compute_task".to_string(),
                timestamp: now,
            });
        }
        
        Ok(PendingRewardsResponse {
            node_id: node_id.clone(),
            pending_rewards: rewards,
            total_pending: pending.value(),
        })
    }
    
    async fn get_storage_reward_estimate(&self, node_id: String) -> Result<StorageRewardEstimateResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        // Get node info
        let node = state.nodes.get(&node_id)
            .ok_or(RpcError::InvalidParams("node not found".to_string()))?;
        
        // Calculate total stake pool
        let total_stake: u64 = state.nodes.iter()
            .map(|n| n.value().stake.value())
            .sum();
        
        // Calculate total VRAM
        let total_vram: f32 = state.nodes.iter()
            .map(|n| n.value().vram_total_gb)
            .sum();
        
        // Calculate VRAM allocated for this node
        let vram_allocated = state.calculate_vram_allocated(&node_id);
        
        // Calculate stake weight
        let stake_weight = if total_stake > 0 {
            node.stake.value() as f32 / total_stake as f32
        } else {
            0.0
        };
        
        // Calculate vram weight
        let vram_weight = if total_vram > 0.0 {
            vram_allocated / total_vram
        } else {
            0.0
        };
        
        // Estimate hourly reward (10,000 AIGEN pool)
        const HOURLY_STORAGE_POOL: f32 = 10_000.0;
        let estimated_reward = (HOURLY_STORAGE_POOL * stake_weight * vram_weight) as u64;
        
        // Count fragments hosted
        let fragments_hosted = state.fragments.iter()
            .filter(|f| f.value().replicas.contains(&node_id))
            .count() as u32;
        
        Ok(StorageRewardEstimateResponse {
            node_id: node_id.clone(),
            estimated_hourly_reward: estimated_reward,
            vram_allocated_gb: vram_allocated,
            stake_weight,
            fragments_hosted,
        })
    }
    
    async fn get_reward_history(&self, node_id: String, limit: Option<u32>) -> Result<RewardHistoryResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        // Get reward history from GlobalState (reads actual per-reward events)
        let limit_val = limit.unwrap_or(100).min(1000) as usize;
        let history_entries = state.get_reward_history(&node_id, limit_val);
        
        // Convert to RPC response type
        let rewards: Vec<RewardHistoryEntry> = history_entries
            .into_iter()
            .map(|entry| RewardHistoryEntry {
                amount: entry.amount.value(),
                reward_type: entry.reward_type,
                timestamp: entry.timestamp,
                task_id: entry.task_id,
            })
            .collect();
        
        Ok(RewardHistoryResponse {
            node_id: node_id.clone(),
            rewards,
            total_count: state.reward_history.get(&node_id).map(|h| h.len() as u32).unwrap_or(0),
        })
    }

    /// Get VRAM pool statistics
    async fn get_vram_pool_stats(&self) -> Result<VramPoolStatsResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        // Calculate total VRAM and active nodes
        let mut total_vram = 0.0f32;
        let mut allocated_vram = 0.0f32;
        let mut active_nodes = 0u32;
        
        for node in state.nodes.iter() {
            let node = node.value();
            total_vram += node.vram_total_gb;
            allocated_vram += node.vram_allocated_gb;
            active_nodes += 1;
        }
        
        // Calculate total fragments and avg replicas
        let total_fragments = state.fragments.len() as u32;
        let total_replicas: usize = state.fragments.iter()
            .map(|f| f.replicas.len())
            .sum();
        let avg_replicas = if total_fragments > 0 {
            total_replicas as f32 / total_fragments as f32
        } else {
            0.0
        };
        
        Ok(VramPoolStatsResponse {
            total_vram_gb: total_vram,
            allocated_vram_gb: allocated_vram,
            active_nodes,
            total_fragments,
            avg_replicas,
        })
    }

    /// Get node metrics
    async fn get_node_metrics(&self, node_id: Option<String>) -> Result<Vec<NodeMetricsResponse>, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        
        let mut metrics = Vec::new();
        
        if let Some(id) = node_id {
            // Get specific node metrics
            if let Some(node) = state.nodes.get(&id) {
                let reward_record = state.get_node_earnings(&id).unwrap_or_default();
                let fragments_hosted = state.fragments.iter()
                    .filter(|f| f.value().replicas.contains(&id))
                    .count() as u32;
                
                metrics.push(NodeMetricsResponse {
                    node_id: id,
                    vram_total_gb: node.vram_total_gb,
                    vram_free_gb: node.vram_free_gb,
                    vram_allocated_gb: node.vram_allocated_gb,
                    fragments_hosted,
                    stake: node.stake.value(),
                    total_earned: reward_record.total_earned.value(),
                    status: if node.load_score < 0.5 { "healthy".to_string() } else { "busy".to_string() },
                    load_score: node.load_score,
                });
            }
        } else {
            // Get all node metrics
            for node_entry in state.nodes.iter() {
                let node = node_entry.value();
                let id = node.node_id.clone();
                let reward_record = state.get_node_earnings(&id).unwrap_or_default();
                let fragments_hosted = state.fragments.iter()
                    .filter(|f| f.value().replicas.contains(&id))
                    .count() as u32;
                
                metrics.push(NodeMetricsResponse {
                    node_id: id,
                    vram_total_gb: node.vram_total_gb,
                    vram_free_gb: node.vram_free_gb,
                    vram_allocated_gb: node.vram_allocated_gb,
                    fragments_hosted,
                    stake: node.stake.value(),
                    total_earned: reward_record.total_earned.value(),
                    status: if node.load_score < 0.5 { "healthy".to_string() } else { "busy".to_string() },
                    load_score: node.load_score,
                });
            }
        }
        
        Ok(metrics)
    }

    /// Get reward leaderboard
    async fn get_reward_leaderboard(&self, limit: Option<u32>) -> Result<RewardLeaderboardResponse, RpcError> {
        let state = self.dcs_state.as_ref().ok_or(RpcError::Internal("DCS not initialized".to_string()))?;
        let limit = limit.unwrap_or(100) as usize;
        
        let mut entries: Vec<RewardLeaderboardEntry> = Vec::new();
        
        for reward_entry in state.node_rewards.iter() {
            let record = reward_entry.value();
            entries.push(RewardLeaderboardEntry {
                node_id: record.node_id.clone(),
                total_earned: record.total_earned.value(),
                compute_rewards: record.compute_rewards.value(),
                storage_rewards: record.storage_rewards.value(),
                tasks_completed: record.tasks_completed,
            });
        }
        
        // Sort by total earned descending
        entries.sort_by(|a, b| b.total_earned.cmp(&a.total_earned));
        
        // Apply limit
        let total_count = entries.len() as u32;
        entries.truncate(limit);
        
        Ok(RewardLeaderboardResponse {
            entries,
            total_count,
        })
    }
}

pub struct StakingGovernanceRpcImpl {
    pub blockchain: Arc<Mutex<blockchain_core::chain::Blockchain>>,
}

impl StakingGovernanceRpcImpl {
    pub fn new(blockchain: Arc<Mutex<blockchain_core::chain::Blockchain>>) -> Self {
        Self { blockchain }
    }
}

#[async_trait]
impl StakingGovernanceRpcServer for StakingGovernanceRpcImpl {
    async fn list_stakes(&self, role_filter: Option<String>) -> Result<StakeListResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        
        let stakes_list = if let Some(role_str) = role_filter {
            let role = parse_stake_role(&role_str)?;
            bc.state.list_stakes_by_role(&role)
        } else {
            bc.state.list_all_stakes()
        };
        
        let stakes: Vec<StakeInfoResponse> = stakes_list
            .into_iter()
            .map(|(address, stake)| StakeInfoResponse {
                staker_address: address,
                staked_amount: stake.staked_amount.value(),
                staked_since: stake.staked_since,
                pending_unstake: stake.pending_unstake.value(),
                unstake_available_at: stake.unstake_available_at,
                role: stake_role_to_string(&stake.role),
                total_rewards_claimed: stake.total_rewards_claimed.value(),
                last_slash_timestamp: stake.last_slash_timestamp,
            })
            .collect();
        
        Ok(StakeListResponse {
            total_count: stakes.len() as u32,
            stakes,
        })
    }
    
    async fn get_stake(&self, address: String) -> Result<Option<StakeInfoResponse>, RpcError> {
        let bc = self.blockchain.lock().await;
        
        if let Some(stake) = bc.state.get_stake(&address) {
            Ok(Some(StakeInfoResponse {
                staker_address: address,
                staked_amount: stake.staked_amount.value(),
                staked_since: stake.staked_since,
                pending_unstake: stake.pending_unstake.value(),
                unstake_available_at: stake.unstake_available_at,
                role: stake_role_to_string(&stake.role),
                total_rewards_claimed: stake.total_rewards_claimed.value(),
                last_slash_timestamp: stake.last_slash_timestamp,
            }))
        } else {
            Ok(None)
        }
    }
    
    async fn submit_stake_tx(&self, request: SubmitStakeTxRequest) -> Result<StakeTxResponse, RpcError> {
        use blockchain_core::transaction::StakeTx;
        use blockchain_core::types::Timestamp;
        
        let role = parse_stake_role(&request.role)?;
        
        // Create stake transaction
        let mut stake_tx = StakeTx::new(
            request.staker.clone(),
            request.amount,
            role,
            request.timestamp,
        ).map_err(|e| RpcError::Internal(e.to_string()))?;
        
        // Parse and attach signature
        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams("signature must be 64 bytes".to_string()));
        }
        let signature = ed25519_dalek::Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature: {e}")))?;
        stake_tx.signature = signature;
        
        // Apply to blockchain state
        let bc = self.blockchain.lock().await;
        bc.state.apply_stake_transaction(&stake_tx)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        
        Ok(StakeTxResponse {
            success: true,
            tx_hash: to_hex_tx_hash(&stake_tx.tx_hash),
            message: format!("Staked {} AIGEN for {} role", request.amount, request.role),
        })
    }
    
    async fn submit_unstake_tx(&self, request: SubmitUnstakeTxRequest) -> Result<StakeTxResponse, RpcError> {
        use blockchain_core::transaction::UnstakeTx;
        
        let mut unstake_tx = UnstakeTx::new(
            request.staker.clone(),
            request.amount,
            request.timestamp,
        ).map_err(|e| RpcError::Internal(e.to_string()))?;
        
        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams("signature must be 64 bytes".to_string()));
        }
        let signature = ed25519_dalek::Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature: {e}")))?;
        unstake_tx.signature = signature;
        
        let bc = self.blockchain.lock().await;
        bc.state.apply_unstake_transaction(&unstake_tx)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        
        Ok(StakeTxResponse {
            success: true,
            tx_hash: to_hex_tx_hash(&unstake_tx.tx_hash),
            message: format!("Unstaking {} AIGEN (7-day cooldown)", request.amount),
        })
    }
    
    async fn submit_claim_stake_tx(&self, request: SubmitClaimStakeTxRequest) -> Result<StakeTxResponse, RpcError> {
        use blockchain_core::transaction::ClaimStakeTx;
        
        let mut claim_tx = ClaimStakeTx::new(
            request.staker.clone(),
            request.timestamp,
        ).map_err(|e| RpcError::Internal(e.to_string()))?;
        
        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams("signature must be 64 bytes".to_string()));
        }
        let signature = ed25519_dalek::Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature: {e}")))?;
        claim_tx.signature = signature;
        
        let bc = self.blockchain.lock().await;
        bc.state.apply_claim_stake_transaction(&claim_tx)
            .map_err(|e| RpcError::Internal(e.to_string()))?;
        
        Ok(StakeTxResponse {
            success: true,
            tx_hash: to_hex_tx_hash(&claim_tx.tx_hash),
            message: "Unstaked tokens claimed successfully".to_string(),
        })
    }
    
    async fn list_proposals(&self, status_filter: Option<String>) -> Result<ProposalListResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        
        let all_proposals: Vec<_> = bc.state.snapshot().model_proposals.into_iter().collect();
        
        let filtered: Vec<ProposalResponse> = all_proposals
            .into_iter()
            .filter(|(_, p)| {
                if let Some(ref filter) = status_filter {
                    let status_str = match p.status {
                        0 => "Pending",
                        1 => "Approved",
                        2 => "Rejected",
                        3 => "AutoApproved",
                        _ => "Unknown",
                    };
                    status_str.eq_ignore_ascii_case(filter)
                } else {
                    true
                }
            })
            .map(|(id, p)| ProposalResponse {
                proposal_id: id,
                model_id: Some(p.model_id),
                current_version: Some(p.current_version),
                new_version: Some(p.new_version),
                description: p.description,
                submitted_by: p.submitted_by,
                submitted_at: p.submitted_at,
                status: match p.status {
                    0 => "Pending".to_string(),
                    1 => "Approved".to_string(),
                    2 => "Rejected".to_string(),
                    3 => "AutoApproved".to_string(),
                    _ => "Unknown".to_string(),
                },
                approved_at: p.approved_at,
                rejected_at: p.rejected_at,
                rejection_reason: p.rejection_reason,
                auto_approved: p.auto_approved,
            })
            .collect();
        
        Ok(ProposalListResponse {
            total_count: filtered.len() as u32,
            proposals: filtered,
        })
    }
    
    async fn get_proposal(&self, proposal_id: String) -> Result<Option<ProposalResponse>, RpcError> {
        let bc = self.blockchain.lock().await;
        
        if let Some(p) = bc.state.get_model_proposal(&proposal_id) {
            Ok(Some(ProposalResponse {
                proposal_id: proposal_id.clone(),
                model_id: Some(p.model_id),
                current_version: Some(p.current_version),
                new_version: Some(p.new_version),
                description: p.description,
                submitted_by: p.submitted_by,
                submitted_at: p.submitted_at,
                status: match p.status {
                    0 => "Pending".to_string(),
                    1 => "Approved".to_string(),
                    2 => "Rejected".to_string(),
                    3 => "AutoApproved".to_string(),
                    _ => "Unknown".to_string(),
                },
                approved_at: p.approved_at,
                rejected_at: p.rejected_at,
                rejection_reason: p.rejection_reason,
                auto_approved: p.auto_approved,
            }))
        } else {
            Ok(None)
        }
    }
    
    async fn list_votes(&self, proposal_id: String) -> Result<VoteListResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        
        let votes_data = bc.state.get_votes_for_proposal(&proposal_id);
        let tally = bc.state.tally_votes(&proposal_id);
        
        let votes: Vec<VoteResponse> = votes_data
            .into_iter()
            .map(|v| VoteResponse {
                proposal_id: v.proposal_id,
                voter_address: v.voter_address,
                vote: match v.vote {
                    0 => "Approve".to_string(),
                    1 => "Reject".to_string(),
                    2 => "Abstain".to_string(),
                    _ => "Unknown".to_string(),
                },
                comment: v.comment,
                timestamp: v.timestamp,
                stake_weight: v.stake_weight,
            })
            .collect();
        
        Ok(VoteListResponse {
            votes,
            tally: VoteTallyResponse {
                proposal_id: tally.proposal_id,
                total_approve_weight: tally.total_approve_weight,
                total_reject_weight: tally.total_reject_weight,
                total_abstain_weight: tally.total_abstain_weight,
                total_voting_power: tally.total_voting_power,
                unique_voters: tally.unique_voters,
                approval_percentage: tally.approval_percentage,
                participation_percentage: tally.participation_percentage,
            },
        })
    }
    
    async fn submit_vote(&self, request: SubmitVoteRequest) -> Result<SubmitVoteResponse, RpcError> {
        use blockchain_core::state::GovernanceVote;
        use blockchain_core::crypto::{verify_signature, PublicKey};
        
        // Parse vote value
        let vote_value = match request.vote.to_lowercase().as_str() {
            "approve" => 0,
            "reject" => 1,
            "abstain" => 2,
            _ => return Err(RpcError::InvalidParams("invalid vote value".to_string())),
        };
        
        // Verify signature (user must sign their vote)
        // Message format: "vote:{proposal_id}:{vote}:{timestamp}"
        let msg = format!("vote:{}:{}:{}", request.proposal_id, request.vote, request.timestamp);
        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams("signature must be 64 bytes".to_string()));
        }
        let signature = ed25519_dalek::Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature: {e}")))?;
        
        // Parse and verify the voter's public key
        let pk_bytes = parse_hex_bytes(&request.voter_public_key)?;
        if pk_bytes.len() != 32 {
            return Err(RpcError::InvalidParams("public key must be 32 bytes".to_string()));
        }
        let public_key = PublicKey::try_from(pk_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid public key: {e}")))?;
        
        // Verify the signature over the message
        if !verify_signature(msg.as_bytes(), &signature, &public_key) {
            return Err(RpcError::InvalidSignature);
        }
        
        let bc = self.blockchain.lock().await;
        
        // Check if proposal exists
        if bc.state.get_model_proposal(&request.proposal_id).is_none() {
            return Err(RpcError::ProposalNotFound);
        }
        
        // Create vote record
        let vote = GovernanceVote {
            proposal_id: request.proposal_id.clone(),
            voter_address: request.voter_address.clone(),
            vote: vote_value,
            comment: request.comment,
            timestamp: request.timestamp,
            stake_weight: 0, // Will be filled by record_governance_vote
        };
        
        // Record the vote with specific error mapping
        bc.state.record_governance_vote(vote)
            .map_err(|e| match e {
                blockchain_core::types::BlockchainError::InvalidTransaction => {
                    // InvalidTransaction can mean: no stake, zero weight, or already voted
                    // Check if voter has stake to determine which error to return
                    if bc.state.get_stake(&request.voter_address).is_none() {
                        RpcError::InsufficientStake
                    } else {
                        // If stake exists but still InvalidTransaction, it's either 
                        // zero weight or already voted
                        let existing_votes = bc.state.get_votes_for_proposal(&request.proposal_id);
                        if existing_votes.iter().any(|v| v.voter_address == request.voter_address) {
                            RpcError::AlreadyVoted
                        } else {
                            RpcError::InsufficientStake
                        }
                    }
                }
                _ => RpcError::Internal(e.to_string()),
            })?;
        
        Ok(SubmitVoteResponse {
            success: true,
            proposal_id: request.proposal_id,
            vote: request.vote,
        })
    }
}
