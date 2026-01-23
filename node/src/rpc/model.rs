use std::sync::Arc;

use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::server::{PendingSubscriptionSink, SubscriptionMessage};
use tracing::{error, info, warn};

use blockchain_core::crypto::PublicKey;
use blockchain_core::Blockchain;
use ed25519_dalek::VerifyingKey;
use tokio::sync::Mutex;

use crate::rpc::types::*;

#[rpc(server)]
pub trait ModelRpc {
    #[method(name = "loadModel")]
    async fn load_model(&self, request: LoadModelRequest) -> Result<LoadModelResponse, RpcError>;

    #[method(name = "listModels")]
    async fn list_models(
        &self,
        user_address: Option<String>,
    ) -> Result<ModelListResponse, RpcError>;

    #[method(name = "getModelInfo")]
    async fn get_model_info(
        &self,
        request: GetModelInfoRequest,
    ) -> Result<ModelInfoResponse, RpcError>;

    #[method(name = "getShard")]
    async fn get_shard(&self, request: GetShardRequest) -> Result<ShardInfoResponse, RpcError>;

    #[method(name = "subscribeTier")]
    async fn subscribe_tier(
        &self,
        request: SubscribeTierRequest,
    ) -> Result<SubscriptionResponse, RpcError>;

    #[method(name = "checkQuota")]
    async fn check_quota(&self, request: CheckQuotaRequest) -> Result<QuotaResponse, RpcError>;

    #[method(name = "submitBatchRequest")]
    async fn submit_batch_request(
        &self,
        request: SubmitBatchRequest,
    ) -> Result<BatchSubmitResponse, RpcError>;

    #[method(name = "getBatchStatus")]
    async fn get_batch_status(
        &self,
        request: GetBatchStatusRequest,
    ) -> Result<BatchStatusResponse, RpcError>;

    #[method(name = "listUserJobs")]
    async fn list_user_jobs(
        &self,
        request: ListUserJobsRequest,
    ) -> Result<BatchJobListResponse, RpcError>;

    #[method(name = "getTierInfo")]
    async fn get_tier_info(&self) -> Result<TierInfoResponse, RpcError>;

    #[method(name = "chatCompletion")]
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, RpcError>;

    #[subscription(name = "subscribeChatCompletion", item = ChatCompletionChunk)]
    async fn subscribe_chat_completion(&self, request: ChatCompletionRequest);
}

#[derive(Clone)]
pub struct ModelRpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub model_registry: Arc<model::ModelRegistry>,
    pub tier_manager: Arc<model::TierManager>,
    pub batch_queue: Arc<model::BatchQueue>,
    pub inference_engine: Arc<model::InferenceEngine>,
}

impl ModelRpcMethods {
    pub fn new(
        blockchain: Arc<Mutex<Blockchain>>,
        model_registry: Arc<model::ModelRegistry>,
        tier_manager: Arc<model::TierManager>,
        batch_queue: Arc<model::BatchQueue>,
        inference_engine: Arc<model::InferenceEngine>,
    ) -> Self {
        Self {
            blockchain,
            model_registry,
            tier_manager,
            batch_queue,
            inference_engine,
        }
    }
}

#[async_trait]
impl ModelRpcServer for ModelRpcMethods {
    async fn load_model(&self, request: LoadModelRequest) -> Result<LoadModelResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        let model_id = request.model_id.trim();
        if model_id.is_empty() {
            return Err(RpcError::InvalidParams("model_id is required".to_string()));
        }
        info!(model_id = %model_id, user_address = %request.user_address, "load_model request received");
        
        // Verify wallet signature and validate transaction
        verify_wallet_signature(&request.transaction)?;
        let tx = request.transaction.into_tx()?;
        
        // Validate that the transaction sender matches the user_address
        if tx.sender != request.user_address {
            return Err(RpcError::InvalidParams(
                "transaction sender does not match user_address".to_string()
            ));
        }
        
        // Validate model access based on tier
        let now = model::now_timestamp();
        validate_model_access(
            &self.tier_manager,
            &self.inference_engine.registry(),
            &request.user_address,
            model_id,
            now,
        )?;
        
        let exists = self
            .model_registry
            .model_exists(model_id)
            .map_err(|e| {
                error!(error = %e, "model_exists failed");
                map_model_error(e)
            })?;
        if !exists {
            return Err(RpcError::ModelNotFound);
        }
        let engine = self.inference_engine.clone();
        let mid = model_id.to_string();
        let res = tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(engine.cache().get_or_load(&mid))
        })
        .await;
        match res {
            Ok(Ok(_)) => {
                info!(model_id = %model_id, "model loaded");
            }
            Ok(Err(e)) => {
                error!(error = %e, model_id = %model_id, "failed to load model");
                return Err(RpcError::Internal(e.to_string()));
            }
            Err(_) => {
                error!(model_id = %model_id, "failed to join load task");
                return Err(RpcError::Internal("load task failed".to_string()));
            }
        }
        info!(model_id = %model_id, "model loaded successfully");
        Ok(LoadModelResponse {
            success: true,
            message: "model loaded".to_string(),
            model_id: model_id.to_string(),
        })
    }

    async fn list_models(
        &self,
        user_address: Option<String>,
    ) -> Result<ModelListResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        match user_address {
            Some(addr) => {
                info!(user_address = %addr, "list_models request received");
                let now = model::now_timestamp();
                let tier = self
                    .tier_manager
                    .ensure_subscription_active(&addr, now)
                    .map(|s| s.tier)
                    .unwrap_or(model::SubscriptionTier::Free);
                let models = self
                    .model_registry
                    .list_models_for_tier(tier)
                    .map_err(|e| {
                        error!(error = %e, "list_models_for_tier failed");
                        map_model_error(e)
                    })?;
                let items = models.into_iter().map(to_model_info).collect();
                Ok(ModelListResponse { models: items })
            }
            None => {
                info!("list_models request received");
                let models = self.model_registry.list_models().map_err(|e| {
                    error!(error = %e, "list_models failed");
                    map_model_error(e)
                })?;
                let items = models.into_iter().map(to_model_info).collect();
                Ok(ModelListResponse { models: items })
            }
        }
    }

    async fn get_model_info(
        &self,
        request: GetModelInfoRequest,
    ) -> Result<ModelInfoResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        let id = request.model_id;
        info!(model_id = %id, "get_model_info request received");
        let model = self.model_registry.get_model(&id).map_err(|e| {
            error!(error = %e, "get_model failed");
            map_model_error(e)
        })?;
        Ok(to_model_info(model))
    }

    async fn get_shard(&self, request: GetShardRequest) -> Result<ShardInfoResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        let id = request.model_id;
        let idx = request.shard_index;
        info!(model_id = %id, shard_index = %idx, "get_shard request received");
        let shard = self
            .model_registry
            .get_shard(&id, idx)
            .map_err(|e| map_model_error(e))?;
        let locs = self
            .model_registry
            .get_shard_locations(&id, idx)
            .map_err(|e| map_model_error(e))?;
        Ok(ShardInfoResponse {
            model_id: shard.model_id,
            shard_index: shard.shard_index,
            total_shards: shard.total_shards,
            size: shard.size,
            hash: to_hex_bytes(&shard.hash),
            ipfs_cid: shard.ipfs_cid,
            http_urls: shard.http_urls,
            locations: locs
                .into_iter()
                .map(|l| ShardLocationResponse {
                    node_id: l.node_id,
                    backend_type: l.backend_type,
                    location_uri: l.location_uri,
                    last_verified: l.last_verified,
                    is_healthy: l.is_healthy,
                })
                .collect(),
        })
    }

    async fn subscribe_tier(
        &self,
        request: SubscribeTierRequest,
    ) -> Result<SubscriptionResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        let expected_tier = parse_tier(&request.tier)?;
        info!(tier = %request.tier, "subscribe_tier request received");
        verify_wallet_signature(&request.transaction)?;
        let tx = request.transaction.into_tx()?;
        
        // Decode and validate SubscriptionPayload from transaction payload
        let payload = tx.payload.as_ref()
            .ok_or_else(|| RpcError::InvalidParams("transaction payload is required".to_string()))?;
        let subscription_payload: model::tiers::SubscriptionPayload = 
            serde_json::from_slice(payload).map_err(|_| RpcError::InvalidParams("invalid subscription payload".to_string()))?;
        
        // Validate tier and duration_months match the request
        if subscription_payload.tier != expected_tier {
            return Err(RpcError::InvalidParams(format!(
                "tier mismatch: request tier {} does not match payload tier {}",
                request.tier,
                tier_to_string(subscription_payload.tier)
            )));
        }
        if subscription_payload.duration_months != request.duration_months {
            return Err(RpcError::InvalidParams(format!(
                "duration_months mismatch: request duration {} does not match payload duration {}",
                request.duration_months,
                subscription_payload.duration_months
            )));
        }
        
        let now = model::now_timestamp();
        let sub = self
            .tier_manager
            .subscribe_user_with_transaction(&tx, genesis::CEO_WALLET, now)
            .map_err(|e| map_tier_error(e))?;
        let cfg = self.tier_manager.get_config(sub.tier).map_err(|e| map_tier_error(e))?;
        let remaining = cfg.request_limit.saturating_sub(sub.requests_used);
        Ok(SubscriptionResponse {
            success: true,
            user_address: sub.user_address,
            tier: tier_to_string(sub.tier),
            expiry_timestamp: sub.expiry_timestamp,
            requests_remaining: remaining,
        })
    }

    async fn check_quota(&self, request: CheckQuotaRequest) -> Result<QuotaResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        info!(user_address = %request.user_address, "check_quota request received");
        let now = model::now_timestamp();
        
        // Try to refresh subscription state, fallback to Free tier if no subscription
        match self.tier_manager.ensure_subscription_active(&request.user_address, now) {
            Ok(sub) => {
                let cfg = self
                    .tier_manager
                    .get_config(sub.tier)
                    .map_err(|e| map_tier_error(e))?;
                let remaining = cfg.request_limit.saturating_sub(sub.requests_used);
                let reset_at = sub.last_reset_timestamp.saturating_add(cfg.window_seconds);
                Ok(QuotaResponse {
                    user_address: sub.user_address,
                    tier: tier_to_string(sub.tier),
                    requests_used: sub.requests_used,
                    requests_remaining: remaining,
                    reset_at,
                })
            }
            Err(_) => {
                // No subscription exists, return Free tier config with zero requests_used
                let free_tier = model::SubscriptionTier::Free;
                let cfg = self
                    .tier_manager
                    .get_config(free_tier)
                    .map_err(|e| map_tier_error(e))?;
                let reset_at = now.saturating_add(cfg.window_seconds);
                Ok(QuotaResponse {
                    user_address: request.user_address,
                    tier: tier_to_string(free_tier),
                    requests_used: 0,
                    requests_remaining: cfg.request_limit,
                    reset_at,
                })
            }
        }
    }

    async fn submit_batch_request(
        &self,
        request: SubmitBatchRequest,
    ) -> Result<BatchSubmitResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        let expected_priority = parse_batch_priority(&request.priority)?;
        info!(
            model_id = %request.model_id,
            priority = %request.priority,
            "submit_batch_request received"
        );
        let input = parse_hex_bytes(&request.input_data)?;
        verify_wallet_signature(&request.transaction)?;
        let tx = request.transaction.into_tx()?;
        let now = model::now_timestamp();
        validate_model_access(
            &self.tier_manager,
            &self.inference_engine.registry(),
            &tx.sender,
            &request.model_id,
            now,
        )?;
        
        // Parse batch payment payload to get the resolved priority
        let batch_payload = model::batch::validate_batch_payment(
            &tx,
            &genesis::CEO_WALLET,
            None,
            request.model_id.clone(),
            input.clone(),
        ).map_err(|e| RpcError::InvalidParams(format!("invalid batch payment: {}", e)))?;
        
        // Verify priority matches the payment payload
        if batch_payload.priority != expected_priority {
            return Err(RpcError::InvalidParams(format!(
                "priority mismatch: request priority {} does not match payload priority {}",
                request.priority,
                batch_priority_to_string(batch_payload.priority)
            )));
        }
        
        let decision = self.tier_manager.check_quota_readonly(&tx.sender, now).map_err(|e| map_tier_error(e))?;
        if !decision.allowed {
            warn!(
                user_address = %tx.sender,
                retry_after = ?decision.retry_after_secs,
                "rate limited"
            );
            return Err(RpcError::QuotaExceeded);
        }
        let job_id = self
            .batch_queue
            .validate_and_submit(&tx, request.model_id.clone(), input)
            .await
            .map_err(|e| map_batch_error(e))?;
        let est = self
            .batch_queue
            .estimate_completion_time(batch_payload.priority, now)
            .await;
        Ok(BatchSubmitResponse {
            success: true,
            job_id,
            scheduled_time: est.scheduled_time,
            estimated_completion: est.estimated_completion_time,
        })
    }

    async fn get_batch_status(
        &self,
        request: GetBatchStatusRequest,
    ) -> Result<BatchStatusResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        info!(job_id = %request.job_id, "get_batch_status received");
        let status = self.batch_queue.get_job_status(&request.job_id).await;
        let info = status.ok_or(RpcError::InvalidParams("job not found".to_string()))?;
        Ok(BatchStatusResponse {
            job_id: info.request_id,
            status: info.status.to_string(),
            priority: batch_priority_to_string(info.priority),
            submission_time: info.submission_time,
            scheduled_time: info.scheduled_time,
            completion_time: info.completion_time,
            result: info.result.map(|r| to_hex_bytes(&r)),
            error_message: info.error_message,
            ad_injected: info.ad_injected,
        })
    }

    async fn list_user_jobs(
        &self,
        request: ListUserJobsRequest,
    ) -> Result<BatchJobListResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        info!(user_address = %request.user_address, status = ?request.status, "list_user_jobs received");
        let status = request
            .status
            .as_ref()
            .map(|s| parse_job_status(s))
            .transpose()?;
        let jobs = self
            .batch_queue
            .list_user_jobs(&request.user_address, status)
            .await;
        let items = jobs
            .into_iter()
            .map(|info| BatchStatusResponse {
                job_id: info.request_id,
                status: info.status.to_string(),
                priority: batch_priority_to_string(info.priority),
                submission_time: info.submission_time,
                scheduled_time: info.scheduled_time,
                completion_time: info.completion_time,
                result: info.result.map(|r| to_hex_bytes(&r)),
                error_message: info.error_message,
                ad_injected: info.ad_injected,
            })
            .collect();
        Ok(BatchJobListResponse { jobs: items })
    }

    async fn get_tier_info(&self) -> Result<TierInfoResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        info!("get_tier_info request received");
        let tiers = [
            model::SubscriptionTier::Free,
            model::SubscriptionTier::Basic,
            model::SubscriptionTier::Pro,
            model::SubscriptionTier::Unlimited,
        ];
        let mut resp = Vec::new();
        for t in tiers {
            let cfg = self.tier_manager.get_config(t).map_err(|e| map_tier_error(e))?;
            resp.push(TierConfigResponse {
                tier: tier_to_string(cfg.tier),
                monthly_cost: cfg.monthly_cost,
                request_limit: cfg.request_limit,
                max_context_size: cfg.max_context_size,
                features: cfg.features.iter().map(feature_to_string).collect(),
            });
        }
        Ok(TierInfoResponse { tiers: resp })
    }

    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, RpcError> {
        if genesis::is_shutdown() {
            return Err(RpcError::ShutdownActive);
        }
        if request.stream {
            return Err(RpcError::InvalidParams(
                "streaming requires subscribeChatCompletion".to_string(),
            ));
        }
        
        let now = model::now_timestamp();
        let user_address = extract_user_address(&request)?;
        
        validate_model_access(
            &self.tier_manager,
            &self.inference_engine.registry(),
            &user_address,
            &request.model_id,
            now,
        )?;
        
        let payment_validated = if let Some(ref tx) = request.transaction {
            validate_chat_payment(
                &self.blockchain,
                tx,
                &user_address,
                &request.model_id,
                request.max_tokens,
            )
            .await?;
            true
        } else {
            let decision = self.tier_manager.check_quota_readonly(&user_address, now)
                .map_err(|e| map_tier_error(e))?;
            if !decision.allowed {
                return Err(RpcError::PaymentRequired);
            }
            false
        };
        
        let inputs = messages_to_tensors(&request.messages, &request.model_id)?;
        let engine = self.inference_engine.clone();
        let model_id = request.model_id.clone();
        let user_for_run = user_address.clone();
        let outputs = tokio::task::spawn_blocking(move || {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(engine.run_inference(&model_id, inputs, &user_for_run))
        })
        .await
        .map_err(|_| RpcError::Internal("inference task join failed".to_string()))?
        .map_err(|e| RpcError::Internal(e.to_string()))?;
        let response = outputs_to_chat_response(&request.model_id, outputs, &request.messages)?;
        
        if !payment_validated {
            let _ = self.tier_manager.record_request(&user_address, None, now);
        }
        
        Ok(response)
    }

    async fn subscribe_chat_completion(
        &self,
        pending: PendingSubscriptionSink,
        request: ChatCompletionRequest,
    ) {
        if !request.stream {
            return;
        }
        if let Ok(sink) = pending.accept().await {
            let now = model::now_timestamp();
            let user_address = match extract_user_address(&request) {
                Ok(a) => a,
                Err(_) => return,
            };
            let allowed = match &request.transaction {
                Some(tx) => {
                    validate_chat_payment(
                        &self.blockchain,
                        tx,
                        &user_address,
                        &request.model_id,
                        request.max_tokens,
                    )
                    .await
                    .is_ok()
                }
                None => {
                    match self.tier_manager.check_quota_readonly(&user_address, now) {
                        Ok(decision) => decision.allowed,
                        Err(_) => false,
                    }
                }
            };
            if !allowed {
                return;
            }
            let inputs = match messages_to_tensors(&request.messages, &request.model_id) {
                Ok(v) => v,
                Err(_) => return,
            };
            let engine = self.inference_engine.clone();
            let model_id = request.model_id.clone();
            let user_for_run = user_address.clone();
            let outputs = match tokio::task::spawn_blocking(move || {
                let handle = tokio::runtime::Handle::current();
                handle.block_on(engine.run_inference(&model_id, inputs, &user_for_run))
            })
            .await
            {
                Ok(Ok(o)) => o,
                _ => return,
            };
            let response_id = uuid::Uuid::new_v4().to_string();
            let text = {
                if let Some(output) = outputs.first() {
                    String::from_utf8(output.data.iter().map(|&f| f as u8).collect())
                        .unwrap_or_else(|_| "".to_string())
                } else {
                    "".to_string()
                }
            };
            let chunk_chars = 256;
            let indices: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            let mut char_pos = 0usize;
            while char_pos < indices.len() {
                let start_byte = indices[char_pos];
                let next_pos = std::cmp::min(char_pos + chunk_chars, indices.len());
                let end_byte = if next_pos < indices.len() {
                    indices[next_pos]
                } else {
                    text.len()
                };
                let part = &text[start_byte..end_byte];
                let chunk = ChatCompletionChunk {
                    id: response_id.clone(),
                    model_id: request.model_id.clone(),
                    delta: ChatMessage {
                        role: "assistant".to_string(),
                        content: part.to_string(),
                    },
                    finish_reason: None,
                };
                if let Ok(payload) = SubscriptionMessage::from_json(&chunk) {
                    if sink.send(payload).await.is_err() {
                        break;
                    }
                }
                char_pos = next_pos;
            }
            let final_chunk = ChatCompletionChunk {
                id: response_id,
                model_id: request.model_id.clone(),
                delta: ChatMessage {
                    role: "assistant".to_string(),
                    content: "".to_string(),
                },
                finish_reason: Some("stop".to_string()),
            };
            if let Ok(payload) = SubscriptionMessage::from_json(&final_chunk) {
                let _ = sink.send(payload).await;
            }
            if request.transaction.is_none() {
                let _ = self.tier_manager.record_request(&user_address, None, now);
            }
        }
    }
}

fn feature_to_string(f: &model::tiers::FeatureFlag) -> String {
    match f {
        model::tiers::FeatureFlag::BasicInference => "BasicInference".to_string(),
        model::tiers::FeatureFlag::AdvancedInference => "AdvancedInference".to_string(),
        model::tiers::FeatureFlag::ModelTraining => "ModelTraining".to_string(),
        model::tiers::FeatureFlag::BatchProcessing => "BatchProcessing".to_string(),
        model::tiers::FeatureFlag::PriorityQueue => "PriorityQueue".to_string(),
        model::tiers::FeatureFlag::ApiAccess => "ApiAccess".to_string(),
        model::tiers::FeatureFlag::CustomModels => "CustomModels".to_string(),
    }
}

fn to_model_info(m: model::ModelMetadata) -> ModelInfoResponse {
    ModelInfoResponse {
        model_id: m.model_id,
        name: m.name,
        version: m.version,
        total_size: m.total_size,
        shard_count: m.shard_count,
        is_core_model: m.is_core_model,
        minimum_tier: m.minimum_tier.map(tier_to_string),
        is_experimental: m.is_experimental,
    }
}

fn parse_job_status(s: &str) -> Result<model::BatchJobStatus, RpcError> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "pending" => Ok(model::BatchJobStatus::Pending),
        "scheduled" => Ok(model::BatchJobStatus::Scheduled),
        "processing" => Ok(model::BatchJobStatus::Processing),
        "completed" => Ok(model::BatchJobStatus::Completed),
        "failed" => Ok(model::BatchJobStatus::Failed),
        _ => Err(RpcError::InvalidParams("invalid job status".to_string())),
    }
}

fn map_model_error(e: model::ModelError) -> RpcError {
    match e {
        model::ModelError::ModelNotFound => RpcError::ModelNotFound,
        model::ModelError::ShardNotFound => RpcError::ShardNotFound,
        model::ModelError::Shutdown => RpcError::ShutdownActive,
        other => RpcError::Internal(other.to_string()),
    }
}

fn map_tier_error(e: model::TierError) -> RpcError {
    match e {
        model::TierError::SubscriptionInactive | model::TierError::SubscriptionNotFound => {
            RpcError::InsufficientTier
        }
        model::TierError::RateLimited => RpcError::QuotaExceeded,
        model::TierError::Shutdown => RpcError::ShutdownActive,
        other => RpcError::Internal(other.to_string()),
    }
}

fn map_batch_error(e: model::BatchError) -> RpcError {
    match e {
        model::BatchError::QueueFull => RpcError::BatchQueueFull,
        model::BatchError::ModelNotFound => RpcError::ModelNotFound,
        model::BatchError::Shutdown => RpcError::ShutdownActive,
        other => RpcError::Internal(other.to_string()),
    }
}

fn verify_wallet_signature(tx: &TransactionResponse) -> Result<(), RpcError> {
    let pk_hex = tx
        .sender_public_key
        .clone()
        .ok_or_else(|| RpcError::InvalidParams("sender_public_key is required".to_string()))?;
    let pk_bytes = parse_hex_bytes(&pk_hex)?;
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
    let tx_obj = tx.clone().into_tx()?;
    let derived_addr = blockchain_core::derive_address_from_pubkey(&public_key);
    if !derived_addr.eq_ignore_ascii_case(&tx_obj.sender) {
        return Err(RpcError::InvalidParams(
            "sender does not match sender_public_key".to_string(),
        ));
    }
    tx_obj
        .verify_with_pubkey(&public_key)
        .map_err(|_| RpcError::InvalidParams("invalid transaction signature".to_string()))?;
    Ok(())
}

fn validate_model_access(
    tier_manager: &model::TierManager,
    registry: &model::ModelRegistry,
    user_address: &str,
    model_id: &str,
    now: i64,
) -> Result<(), RpcError> {
    let allowed = tier_manager
        .can_access_model(registry, user_address, model_id, now)
        .map_err(|e| map_tier_error(e))?;
    if !allowed {
        return Err(RpcError::InsufficientTier);
    }
    Ok(())
}

fn extract_user_address(request: &ChatCompletionRequest) -> Result<String, RpcError> {
    if let Some(addr) = &request.user_address {
        return Ok(addr.clone());
    }
    if let Some(ref tx) = request.transaction {
        return Ok(tx.sender.clone());
    }
    Err(RpcError::InvalidParams("user_address required".to_string()))
}

async fn validate_chat_payment(
    blockchain: &Arc<Mutex<Blockchain>>,
    tx: &TransactionResponse,
    user_address: &str,
    model_id: &str,
    max_tokens: Option<u32>,
) -> Result<ChatPaymentPayload, RpcError> {
    verify_wallet_signature(tx)?;
    let tx_obj = tx.clone().into_tx()?;
    
    let payload = tx_obj.payload.as_ref()
        .ok_or_else(|| RpcError::InvalidParams("payment payload required".to_string()))?;
    let chat_payload: ChatPaymentPayload = serde_json::from_slice(payload)
        .map_err(|_| RpcError::InvalidParams("invalid chat payment payload".to_string()))?;
    
    // Validate sender matches
    if tx_obj.sender != user_address {
        return Err(RpcError::InvalidParams("sender mismatch".to_string()));
    }
    
    // Validate model_id matches
    if chat_payload.model_id != model_id {
        return Err(RpcError::InvalidParams("model_id mismatch".to_string()));
    }
    
    // Validate receiver is CEO wallet
    if tx_obj.receiver != genesis::CEO_WALLET {
        return Err(RpcError::InvalidParams("receiver must be CEO wallet".to_string()));
    }
    
    // Validate payment amount (e.g., 1 AIGEN per 1000 tokens)
    let expected_amount = calculate_chat_price(max_tokens.unwrap_or(chat_payload.max_tokens));
    let actual_amount = tx_obj.amount.value();
    if actual_amount < expected_amount {
        return Err(RpcError::InvalidParams(format!(
            "insufficient payment: expected {}, got {}",
            expected_amount, actual_amount
        )));
    }
    
    // Verify inclusion in chain or presence in mempool
    let bc = blockchain.lock().await;
    let included_in_block = bc
        .blocks
        .iter()
        .any(|b| b.transactions.iter().any(|t| t.tx_hash == tx_obj.tx_hash));
    let in_pool = bc
        .get_pending_transactions(10_000)
        .iter()
        .any(|t| t.tx_hash == tx_obj.tx_hash);
    if !included_in_block && !in_pool {
        return Err(RpcError::InvalidParams(
            "payment transaction not found in mempool or chain".to_string(),
        ));
    }
    
    Ok(chat_payload)
}

fn calculate_chat_price(max_tokens: u32) -> u64 {
    // Price: 1 AIGEN per 1000 tokens (from model constants)
    let base_price_per_1k = model::CHAT_PRICE_PER_1K_TOKENS;
    ((max_tokens as u64 + 999) / 1000) * base_price_per_1k
}

fn messages_to_tensors(
    messages: &[ChatMessage],
    _model_id: &str,
) -> Result<Vec<model::InferenceTensor>, RpcError> {
    let prompt = messages.iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");
    let tokens: Vec<f32> = prompt.bytes()
        .map(|b| b as f32)
        .collect();
    Ok(vec![model::InferenceTensor {
        name: "input_ids".to_string(),
        shape: vec![1, tokens.len() as i64],
        data: tokens,
    }])
}

fn outputs_to_chat_response(
    model_id: &str,
    outputs: Vec<model::InferenceOutput>,
    messages: &[ChatMessage],
) -> Result<ChatCompletionResponse, RpcError> {
    let output_text = if let Some(output) = outputs.first() {
        String::from_utf8(output.data.iter().map(|&f| f as u8).collect())
            .unwrap_or_else(|_| "".to_string())
    } else {
        return Err(RpcError::Internal("no output from model".to_string()));
    };
    let ad_injected = outputs.iter().any(|o| o.name == "ad_text");
    let response_id = uuid::Uuid::new_v4().to_string();
    Ok(ChatCompletionResponse {
        id: response_id,
        model_id: model_id.to_string(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: output_text,
            },
            finish_reason: "stop".to_string(),
        }],
        usage: ChatUsage {
            prompt_tokens: messages.iter().map(|m| m.content.len() as u32).sum(),
            completion_tokens: outputs.first().map(|o| o.data.len() as u32).unwrap_or(0),
            total_tokens: 0,
        },
        ad_injected,
    })
}
