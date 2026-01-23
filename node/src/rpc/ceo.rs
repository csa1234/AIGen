use std::sync::Arc;

use blockchain_core::Blockchain;
use genesis::CeoSignature;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use tokio::sync::Mutex;

use crate::rpc::types::{
    parse_hex_bytes, parse_tier, AdminHealthRequest, AdminHealthResponse, ApproveModelUpgradeRequest,
    GetMetricsRequest, GetMetricsResponse, InitNewModelRequest, InitNewModelResponse,
    ModelUpgradeResponse, RejectUpgradeRequest, RpcError, ShutdownRequest, SipActionRequest,
    SipActionResponse, SipStatusResponse, SubmitGovVoteRequest, SuccessResponse, GovVoteResponse,
};

fn verify_ceo_request(message: &[u8], signature_hex: &str) -> Result<CeoSignature, RpcError> {
    use ed25519_dalek::Signature;

    let sig_bytes = parse_hex_bytes(signature_hex)?;
    if sig_bytes.len() != 64 {
        return Err(RpcError::InvalidParams(
            "signature must be 64 bytes".to_string(),
        ));
    }
    let sig = Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
    let ceo_sig = CeoSignature(sig);

    genesis::verify_ceo_signature(message, &ceo_sig).map_err(|_| RpcError::InvalidSignature)?;

    Ok(ceo_sig)
}

// CEO RPC trait for administrative operations
#[rpc(server)]
pub trait CeoRpc {
    #[method(name = "submitShutdown")]
    async fn submit_shutdown(&self, request: ShutdownRequest) -> Result<SuccessResponse, RpcError>;

    #[method(name = "approveSIP")]
    async fn approve_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError>;

    #[method(name = "vetoSIP")]
    async fn veto_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError>;

    #[method(name = "getSIPStatus")]
    async fn get_sip_status(&self, proposal_id: String) -> Result<SipStatusResponse, RpcError>;

    #[method(name = "getHealth")]
    async fn get_health(&self, request: AdminHealthRequest) -> Result<AdminHealthResponse, RpcError>;

    #[method(name = "getMetrics")]
    async fn get_metrics(&self, request: GetMetricsRequest) -> Result<GetMetricsResponse, RpcError>;

    #[method(name = "initNewModel")]
    async fn init_new_model(&self, request: InitNewModelRequest) -> Result<InitNewModelResponse, RpcError>;

    #[method(name = "approveModelUpgrade")]
    async fn approve_model_upgrade(&self, request: ApproveModelUpgradeRequest) -> Result<ModelUpgradeResponse, RpcError>;

    #[method(name = "rejectUpgrade")]
    async fn reject_upgrade(&self, request: RejectUpgradeRequest) -> Result<ModelUpgradeResponse, RpcError>;

    #[method(name = "submitGovVote")]
    async fn submit_gov_vote(&self, request: SubmitGovVoteRequest) -> Result<GovVoteResponse, RpcError>;
}

#[derive(Clone)]
pub struct CeoRpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub model_registry: Arc<model::ModelRegistry>,
    pub inference_engine: Arc<model::InferenceEngine>,
    #[allow(dead_code)]
    pub tier_manager: Arc<model::TierManager>,
    #[allow(dead_code)]
    pub batch_queue: Arc<model::BatchQueue>,
    pub network_metrics: network::metrics::SharedNetworkMetrics,
}

impl CeoRpcMethods {
    pub fn new(
        blockchain: Arc<Mutex<Blockchain>>,
        model_registry: Arc<model::ModelRegistry>,
        inference_engine: Arc<model::InferenceEngine>,
        tier_manager: Arc<model::TierManager>,
        batch_queue: Arc<model::BatchQueue>,
        network_metrics: network::metrics::SharedNetworkMetrics,
    ) -> Self {
        Self {
            blockchain,
            model_registry,
            inference_engine,
            tier_manager,
            batch_queue,
            network_metrics,
        }
    }
}

#[async_trait]
impl CeoRpcServer for CeoRpcMethods {
    async fn submit_shutdown(&self, request: ShutdownRequest) -> Result<SuccessResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;

        let msg = format!(
            "shutdown:{}:{}:{}:{}",
            network_magic, request.timestamp, request.nonce, request.reason
        )
        .into_bytes();

        let ceo_sig = verify_ceo_request(&msg, &request.signature)?;
        let command = genesis::ShutdownCommand {
            timestamp: request.timestamp,
            reason: request.reason,
            ceo_signature: ceo_sig,
            nonce: request.nonce,
            network_magic,
        };

        genesis::emergency_shutdown(command).map_err(|e| {
            println!("shutdown rejected: {}", e);
            RpcError::Internal(e.to_string())
        })?;

        Ok(SuccessResponse {
            success: true,
            message: "Shutdown initiated".to_string(),
        })
    }

    async fn approve_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError> {
        use ed25519_dalek::Signature;

        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams(
                "signature must be 64 bytes".to_string(),
            ));
        }
        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
        let ceo_sig = genesis::CeoSignature(sig);

        genesis::approve_sip(&request.proposal_id, ceo_sig)
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(SipActionResponse {
            success: true,
            proposal_id: request.proposal_id,
        })
    }

    async fn veto_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError> {
        use ed25519_dalek::Signature;

        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams(
                "signature must be 64 bytes".to_string(),
            ));
        }
        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
        let ceo_sig = genesis::CeoSignature(sig);

        genesis::veto_sip(&request.proposal_id, ceo_sig)
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(SipActionResponse {
            success: true,
            proposal_id: request.proposal_id,
        })
    }

    async fn get_sip_status(&self, proposal_id: String) -> Result<SipStatusResponse, RpcError> {
        let status = genesis::get_sip_status(&proposal_id)
            .ok_or_else(|| RpcError::InvalidParams("proposal not found".to_string()))?;

        let status_str = match status {
            genesis::SipStatus::Pending => "Pending",
            genesis::SipStatus::Approved => "Approved",
            genesis::SipStatus::Vetoed => "Vetoed",
            genesis::SipStatus::Deployed => "Deployed",
        }
        .to_string();

        Ok(SipStatusResponse {
            proposal_id,
            status: status_str,
        })
    }

    async fn get_health(&self, request: AdminHealthRequest) -> Result<AdminHealthResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let msg = format!("admin_health:{}:{}", network_magic, request.timestamp).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;
        drop(bc);

        println!("admin health check requested");

        // Gather node health stats
        let uptime_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let peer_count = self.network_metrics.peers_connected.load(std::sync::atomic::Ordering::Relaxed);

        // Get memory usage (simplified - use sysinfo crate for production)
        let memory_usage_mb = self.inference_engine.get_metrics().current_memory_bytes / (1024 * 1024);

        let node_status = if genesis::is_shutdown() {
            "unhealthy"
        } else if peer_count == 0 {
            "degraded"
        } else {
            "healthy"
        };

        // Gather AI health stats
        let ai_metrics = self.inference_engine.get_metrics();
        let core_model_id = self.inference_engine.cache().get_core_model()
            .unwrap_or(None);
        let core_model_loaded = core_model_id.is_some();

        let cache_hit_rate = if ai_metrics.cache_hits + ai_metrics.cache_misses > 0 {
            (ai_metrics.cache_hits as f32) / ((ai_metrics.cache_hits + ai_metrics.cache_misses) as f32)
        } else {
            0.0
        };

        // Gather blockchain health stats
        let bc = self.blockchain.lock().await;
        let chain_height = bc.blocks.len().saturating_sub(1) as u64;
        let latest_block_time = bc.blocks.last().map(|b| b.header.timestamp.0).unwrap_or(0);
        let pending_transactions = bc.get_pending_transactions(10000).len() as u64;
        let total_supply = bc.state.snapshot().accounts.values()
            .map(|a| a.balance.amount().value())
            .sum::<u64>();
        let shutdown_active = bc.shutdown || genesis::is_shutdown();

        Ok(AdminHealthResponse {
            node_health: crate::rpc::types::NodeHealthStats {
                status: node_status.to_string(),
                uptime_seconds,
                peer_count,
                memory_usage_mb,
                cpu_usage_percent: 0.0,
            },
            ai_health: crate::rpc::types::AiHealthStats {
                models_loaded: ai_metrics.current_models_loaded,
                core_model_loaded,
                core_model_id,
                memory_usage_mb,
                cache_hit_rate,
                average_inference_time_ms: ai_metrics.average_inference_time_ms,
            },
            blockchain_health: crate::rpc::types::BlockchainHealthStats {
                chain_height,
                latest_block_time,
                pending_transactions,
                total_supply,
                shutdown_active,
            },
        })
    }

    async fn get_metrics(&self, request: GetMetricsRequest) -> Result<GetMetricsResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let msg = format!("get_metrics:{}:{}", network_magic, request.timestamp).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;
        drop(bc);

        println!("metrics requested: ai={}, network={}, blockchain={}",
                 request.include_ai, request.include_network, request.include_blockchain);

        let ai_metrics = if request.include_ai {
            let stats = self.inference_engine.get_metrics();
            Some(crate::rpc::types::AiMetricsStats {
                total_inference_runs: stats.total_inference_runs,
                total_inference_failures: stats.total_inference_failures,
                cache_hits: stats.cache_hits,
                cache_misses: stats.cache_misses,
                cache_evictions: stats.cache_evictions,
                total_models_loaded: stats.total_models_loaded,
                total_model_load_failures: stats.total_model_load_failures,
                current_models_loaded: stats.current_models_loaded,
                current_memory_bytes: stats.current_memory_bytes,
                total_inference_time_ms: stats.total_inference_time_ms,
                average_inference_time_ms: stats.average_inference_time_ms,
            })
        } else {
            None
        };

        let network_metrics = if request.include_network {
            use std::sync::atomic::Ordering;
            Some(crate::rpc::types::NetworkMetricsStats {
                peers_connected: self.network_metrics.peers_connected.load(Ordering::Relaxed),
                messages_sent: self.network_metrics.messages_sent.load(Ordering::Relaxed),
                messages_received: self.network_metrics.messages_received.load(Ordering::Relaxed),
                bytes_sent: self.network_metrics.bytes_sent.load(Ordering::Relaxed),
                bytes_received: self.network_metrics.bytes_received.load(Ordering::Relaxed),
                model_shards_transferred: self.network_metrics.model_shards_received.load(Ordering::Relaxed),
            })
        } else {
            None
        };

        let blockchain_metrics = if request.include_blockchain {
            let bc = self.blockchain.lock().await;
            let snapshot = bc.state.snapshot();
            let pending_batch_jobs = bc.state.list_pending_batch_jobs().len() as u64;

            Some(crate::rpc::types::BlockchainMetricsStats {
                total_blocks: bc.blocks.len() as u64,
                total_transactions: bc.blocks.iter().map(|b| b.transactions.len() as u64).sum(),
                pending_transactions: bc.get_pending_transactions(10000).len() as u64,
                total_subscriptions: snapshot.subscriptions.len() as u64,
                total_batch_jobs: snapshot.batch_jobs.len() as u64,
                pending_batch_jobs,
            })
        } else {
            None
        };

        Ok(GetMetricsResponse {
            ai_metrics,
            network_metrics,
            blockchain_metrics,
        })
    }

    async fn init_new_model(&self, request: InitNewModelRequest) -> Result<InitNewModelResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let hashes_str = request.verification_hashes.join(":");
        let msg = format!(
            "init_model:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            network_magic, request.timestamp, request.model_id, request.version,
            request.total_size, request.shard_count,
            request.verification_hashes.len(), request.is_core_model,
            request.minimum_tier.as_deref().unwrap_or(""),
            request.is_experimental, request.name,
            hashes_str
        ).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;
        drop(bc);

        println!("initializing new model: id={}, version={}, is_core={}",
                 request.model_id, request.version, request.is_core_model);

        // Parse verification hashes from hex
        let mut verification_hashes = Vec::new();
        for hash_hex in &request.verification_hashes {
            let hash_bytes = parse_hex_bytes(hash_hex)?;
            if hash_bytes.len() != 32 {
                return Err(RpcError::InvalidParams("hash must be 32 bytes".to_string()));
            }
            let mut hash_arr = [0u8; 32];
            hash_arr.copy_from_slice(&hash_bytes);
            verification_hashes.push(hash_arr);
        }

        if verification_hashes.len() != request.shard_count as usize {
            return Err(RpcError::InvalidParams(
                "verification_hashes count must match shard_count".to_string()
            ));
        }

        // Parse minimum tier
        let minimum_tier = request.minimum_tier
            .as_ref()
            .map(|t| parse_tier(t))
            .transpose()?;

        // Create model metadata
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let metadata = model::ModelMetadata {
            model_id: request.model_id.clone(),
            name: request.name,
            version: request.version,
            total_size: request.total_size,
            shard_count: request.shard_count,
            verification_hashes,
            is_core_model: request.is_core_model,
            minimum_tier,
            is_experimental: request.is_experimental,
            created_at: now,
        };

        // Register model
        self.model_registry.register_model(metadata)
            .map_err(|e| {
                println!("failed to register model: {}", e);
                RpcError::Internal(e.to_string())
            })?;

        println!("model initialized successfully: {}", request.model_id);

        Ok(InitNewModelResponse {
            success: true,
            model_id: request.model_id,
            message: "model initialized successfully".to_string(),
        })
    }

    async fn approve_model_upgrade(&self, request: ApproveModelUpgradeRequest) -> Result<ModelUpgradeResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let msg = format!(
            "approve_upgrade:{}:{}:{}",
            network_magic, request.timestamp, request.proposal_id
        ).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;

        println!("approving model upgrade: {}", request.proposal_id);

        // Get proposal
        let mut proposal = bc.state.get_model_proposal(&request.proposal_id)
            .ok_or_else(|| RpcError::InvalidParams("proposal not found".to_string()))?;

        if proposal.status != 0 {
            return Err(RpcError::InvalidParams("proposal already processed".to_string()));
        }

        // Update proposal status
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        proposal.status = 1;
        proposal.approved_at = Some(now);
        bc.state.set_model_proposal(request.proposal_id.clone(), proposal);

        println!("model upgrade approved: {}", request.proposal_id);

        Ok(ModelUpgradeResponse {
            success: true,
            proposal_id: request.proposal_id,
            status: "approved".to_string(),
        })
    }

    async fn reject_upgrade(&self, request: RejectUpgradeRequest) -> Result<ModelUpgradeResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let msg = format!(
            "reject_upgrade:{}:{}:{}:{}",
            network_magic, request.timestamp, request.proposal_id, request.reason
        ).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;

        println!("rejecting model upgrade: {}, reason: {}", request.proposal_id, request.reason);

        // Get proposal
        let mut proposal = bc.state.get_model_proposal(&request.proposal_id)
            .ok_or_else(|| RpcError::InvalidParams("proposal not found".to_string()))?;

        if proposal.status != 0 {
            return Err(RpcError::InvalidParams("proposal already processed".to_string()));
        }

        // Update proposal status
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        proposal.status = 2;
        proposal.rejected_at = Some(now);
        proposal.rejection_reason = Some(request.reason);
        bc.state.set_model_proposal(request.proposal_id.clone(), proposal);

        println!("model upgrade rejected: {}", request.proposal_id);

        Ok(ModelUpgradeResponse {
            success: true,
            proposal_id: request.proposal_id,
            status: "rejected".to_string(),
        })
    }

    async fn submit_gov_vote(&self, request: SubmitGovVoteRequest) -> Result<GovVoteResponse, RpcError> {
        // Verify CEO signature
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;
        let msg = format!(
            "gov_vote:{}:{}:{}:{}",
            network_magic, request.timestamp, request.proposal_id, request.vote
        ).into_bytes();
        verify_ceo_request(&msg, &request.signature)?;

        println!("recording governance vote: proposal={}, vote={}",
                 request.proposal_id, request.vote);

        // Parse vote
        let vote_value = match request.vote.to_lowercase().as_str() {
            "approve" => 0,
            "reject" => 1,
            "abstain" => 2,
            _ => return Err(RpcError::InvalidParams("invalid vote value".to_string())),
        };

        // Record vote
        let vote = blockchain_core::state::GovernanceVote {
            proposal_id: request.proposal_id.clone(),
            voter_address: genesis::CEO_WALLET.to_string(),
            vote: vote_value,
            comment: request.comment,
            timestamp: request.timestamp,
        };

        bc.state.record_governance_vote(vote);

        println!("governance vote recorded: {}", request.proposal_id);

        Ok(GovVoteResponse {
            success: true,
            proposal_id: request.proposal_id,
            vote: request.vote,
        })
    }
}
