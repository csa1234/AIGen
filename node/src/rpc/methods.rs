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
}

#[derive(Clone)]
pub struct RpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
    pub network_metrics: network::metrics::SharedNetworkMetrics,
    pub start_time: std::sync::Arc<std::time::Instant>,
    pub dcs: Option<Arc<DynamicScheduler>>,
    pub dcs_state: Option<Arc<GlobalState>>,
}

impl RpcMethods {
    pub fn new(
        blockchain: Arc<Mutex<Blockchain>>,
        tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
        network_metrics: network::metrics::SharedNetworkMetrics,
        dcs: Option<Arc<DynamicScheduler>>,
        dcs_state: Option<Arc<GlobalState>>,
    ) -> Self {
        Self {
            blockchain,
            tx_broadcast,
            network_metrics,
            start_time: std::sync::Arc::new(std::time::Instant::now()),
            dcs,
            dcs_state,
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
}
