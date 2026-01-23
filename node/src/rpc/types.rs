use blockchain_core::types::{BlockHash, TxHash};
use blockchain_core::{Block, Transaction};
use jsonrpsee::types::ErrorObjectOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("block not found")]
    BlockNotFound,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("unauthorized")]
    Unauthorized,
    #[error("shutdown active")]
    ShutdownActive,
    #[error("model not found")]
    ModelNotFound,
    #[error("shard not found")]
    ShardNotFound,
    #[error("insufficient tier")]
    InsufficientTier,
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("batch queue full")]
    BatchQueueFull,
    #[error("invalid chat format")]
    InvalidChatFormat,
    #[error("payment required")]
    PaymentRequired,
    #[error("context length exceeded")]
    ContextLengthExceeded,
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl RpcError {
    fn code(&self) -> i32 {
        match self {
            RpcError::BlockNotFound => 1001,
            RpcError::InvalidSignature => 1002,
            RpcError::Unauthorized => 1003,
            RpcError::ShutdownActive => 1004,
            RpcError::ModelNotFound => 2001,
            RpcError::ShardNotFound => 2002,
            RpcError::InsufficientTier => 2003,
            RpcError::QuotaExceeded => 2004,
            RpcError::BatchQueueFull => 2005,
            RpcError::InvalidChatFormat => 2006,
            RpcError::PaymentRequired => 2007,
            RpcError::ContextLengthExceeded => 2008,
            RpcError::InvalidParams(_) => -32602,
            RpcError::Internal(_) => -32603,
        }
    }

    fn message(&self) -> String {
        self.to_string()
    }
}

impl From<RpcError> for ErrorObjectOwned {
    fn from(e: RpcError) -> Self {
        ErrorObjectOwned::owned(e.code(), e.message(), None::<()>)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockResponse {
    pub block_hash: String,
    pub header: BlockHeaderResponse,
    pub transactions: Vec<TransactionResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeaderResponse {
    pub version: u32,
    pub previous_hash: String,
    pub merkle_root: String,
    pub timestamp: i64,
    pub poi_proof: Option<String>,
    pub ceo_signature: Option<String>,
    pub shutdown_flag: bool,
    pub block_height: u64,
    pub priority: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub sender: String,
    pub sender_public_key: Option<String>,
    pub receiver: String,
    pub amount: u64,
    pub signature: String,
    pub timestamp: i64,
    pub nonce: u64,
    pub priority: bool,
    pub tx_hash: String,
    pub fee_base: u64,
    pub fee_priority: u64,
    pub fee_burn: u64,
    pub chain_id: u64,
    pub payload: Option<String>,
    pub ceo_signature: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainInfoResponse {
    pub chain_id: u64,
    pub height: u64,
    pub latest_block_hash: String,
    pub shutdown_status: bool,
    pub total_supply: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
    pub peer_count: u64,
    pub sync_status: String,
    pub latest_block_time: i64,
    pub shutdown_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub address: String,
    pub balance: u64,
    pub nonce: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitTransactionResponse {
    pub tx_hash: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShutdownRequest {
    pub timestamp: i64,
    pub reason: String,
    pub nonce: u64,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SipActionRequest {
    pub proposal_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SipActionResponse {
    pub success: bool,
    pub proposal_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SipStatusResponse {
    pub proposal_id: String,
    pub status: String,
}

pub fn to_hex_block_hash(h: &BlockHash) -> String {
    format!("0x{}", hex::encode(h.0))
}

pub fn to_hex_tx_hash(h: &TxHash) -> String {
    format!("0x{}", hex::encode(h.0))
}

pub fn to_hex_bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, RpcError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|e| RpcError::InvalidParams(format!("invalid hex: {e}")))
}

impl BlockResponse {
    pub fn from_block(b: &Block) -> Self {
        BlockResponse {
            block_hash: to_hex_block_hash(&b.block_hash),
            header: BlockHeaderResponse {
                version: b.header.version.0,
                previous_hash: to_hex_block_hash(&b.header.previous_hash),
                merkle_root: to_hex_block_hash(&b.header.merkle_root),
                timestamp: b.header.timestamp.0,
                poi_proof: b.header.poi_proof.as_ref().map(|p| to_hex_bytes(p)),
                ceo_signature: b
                    .header
                    .ceo_signature
                    .as_ref()
                    .map(|s| to_hex_bytes(&s.inner().to_bytes())),
                shutdown_flag: b.header.shutdown_flag,
                block_height: b.header.block_height.0,
                priority: b.header.priority,
            },
            transactions: b
                .transactions
                .iter()
                .map(TransactionResponse::from_tx)
                .collect(),
        }
    }
}

impl TransactionResponse {
    pub fn from_tx(tx: &Transaction) -> Self {
        TransactionResponse {
            sender: tx.sender.clone(),
            sender_public_key: None,
            receiver: tx.receiver.clone(),
            amount: tx.amount.value(),
            signature: to_hex_bytes(&tx.signature.to_bytes()[..]),
            timestamp: tx.timestamp.0,
            nonce: tx.nonce.value(),
            priority: tx.priority,
            tx_hash: to_hex_tx_hash(&tx.tx_hash),
            fee_base: tx.fee.base_fee.value(),
            fee_priority: tx.fee.priority_fee.value(),
            fee_burn: tx.fee.burn_amount.value(),
            chain_id: tx.chain_id.value(),
            payload: tx.payload.as_ref().map(|p| to_hex_bytes(p)),
            ceo_signature: tx
                .ceo_signature
                .as_ref()
                .map(|s| to_hex_bytes(&s.inner().to_bytes()[..])),
        }
    }

    pub fn into_tx(self) -> Result<Transaction, RpcError> {
        use blockchain_core::types::{Amount, ChainId, Fee, Nonce, Timestamp, TxHash};
        use ed25519_dalek::Signature;

        let sig_bytes = parse_hex_bytes(&self.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams(
                "signature must be 64 bytes".to_string(),
            ));
        }
        let signature = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;

        let hash_bytes = parse_hex_bytes(&self.tx_hash)?;
        if hash_bytes.len() != 32 {
            return Err(RpcError::InvalidParams(
                "tx_hash must be 32 bytes".to_string(),
            ));
        }
        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&hash_bytes);

        #[allow(non_snake_case)]
        let payload = match self.payload {
            Some(p) => Some(parse_hex_bytes(&p)?),
            None => None,
        };

        #[allow(non_snake_case)]
        let ceo_signature = match self.ceo_signature {
            Some(s) => {
                let b = parse_hex_bytes(&s)?;
                if b.len() != 64 {
                    return Err(RpcError::InvalidParams(
                        "ceo_signature must be 64 bytes".to_string(),
                    ));
                }
                let sig = Signature::try_from(b.as_slice())
                    .map_err(|e| RpcError::InvalidParams(format!("invalid ceo sig bytes: {e}")))?;
                Some(genesis::CeoSignature(sig))
            }
            None => None,
        };

        let tx = Transaction {
            sender: self.sender,
            receiver: self.receiver,
            amount: Amount::new(self.amount),
            signature,
            timestamp: Timestamp(self.timestamp),
            nonce: Nonce::new(self.nonce),
            priority: self.priority,
            tx_hash: TxHash(hash_arr),
            fee: Fee {
                base_fee: Amount::new(self.fee_base),
                priority_fee: Amount::new(self.fee_priority),
                burn_amount: Amount::new(self.fee_burn),
            },
            chain_id: ChainId(self.chain_id),
            payload,
            ceo_signature,
        };

        let expected_hash = blockchain_core::hash_transaction(&tx);
        if expected_hash != tx.tx_hash {
            return Err(RpcError::InvalidParams(
                "tx_hash does not match transaction contents".to_string(),
            ));
        }

        let _ = self.sender_public_key;

        Ok(tx)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadModelRequest {
    pub model_id: String,
    pub user_address: String,
    pub transaction: TransactionResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadModelResponse {
    pub success: bool,
    pub message: String,
    pub model_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetModelInfoRequest {
    pub model_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetShardRequest {
    pub model_id: String,
    pub shard_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscribeTierRequest {
    pub tier: String,
    pub duration_months: u32,
    pub transaction: TransactionResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckQuotaRequest {
    pub user_address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitBatchRequest {
    pub priority: String,
    pub model_id: String,
    pub input_data: String,
    pub transaction: TransactionResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBatchStatusRequest {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListUserJobsRequest {
    pub user_address: String,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelListResponse {
    pub models: Vec<ModelInfoResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfoResponse {
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub total_size: u64,
    pub shard_count: u32,
    pub is_core_model: bool,
    pub minimum_tier: Option<String>,
    pub is_experimental: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardLocationResponse {
    pub node_id: String,
    pub backend_type: String,
    pub location_uri: String,
    pub last_verified: i64,
    pub is_healthy: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardInfoResponse {
    pub model_id: String,
    pub shard_index: u32,
    pub total_shards: u32,
    pub size: u64,
    pub hash: String,
    pub ipfs_cid: Option<String>,
    pub http_urls: Vec<String>,
    pub locations: Vec<ShardLocationResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    pub success: bool,
    pub user_address: String,
    pub tier: String,
    pub expiry_timestamp: i64,
    pub requests_remaining: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaResponse {
    pub user_address: String,
    pub tier: String,
    pub requests_used: u64,
    pub requests_remaining: u64,
    pub reset_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchSubmitResponse {
    pub success: bool,
    pub job_id: String,
    pub scheduled_time: i64,
    pub estimated_completion: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchStatusResponse {
    pub job_id: String,
    pub status: String,
    pub priority: String,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub completion_time: Option<i64>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub ad_injected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchJobListResponse {
    pub jobs: Vec<BatchStatusResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierInfoResponse {
    pub tiers: Vec<TierConfigResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierConfigResponse {
    pub tier: String,
    pub monthly_cost: u64,
    pub request_limit: u64,
    pub max_context_size: u32,
    pub features: Vec<String>,
}

// Chat message structures
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,        // "system", "user", "assistant"
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub model_id: String,
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub user_address: Option<String>,
    pub transaction: Option<TransactionResponse>,  // For pay-per-use
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub model_id: String,
    pub choices: Vec<ChatChoice>,
    pub usage: ChatUsage,
    pub ad_injected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,  // "stop", "length", "error"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub model_id: String,
    pub delta: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatPaymentPayload {
    pub user_address: String,
    pub model_id: String,
    pub max_tokens: u32,
}

pub fn parse_tier(s: &str) -> Result<model::SubscriptionTier, RpcError> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "free" => Ok(model::SubscriptionTier::Free),
        "basic" => Ok(model::SubscriptionTier::Basic),
        "pro" => Ok(model::SubscriptionTier::Pro),
        "unlimited" => Ok(model::SubscriptionTier::Unlimited),
        _ => Err(RpcError::InvalidParams("invalid tier".to_string())),
    }
}

pub fn tier_to_string(tier: model::SubscriptionTier) -> String {
    match tier {
        model::SubscriptionTier::Free => "Free".to_string(),
        model::SubscriptionTier::Basic => "Basic".to_string(),
        model::SubscriptionTier::Pro => "Pro".to_string(),
        model::SubscriptionTier::Unlimited => "Unlimited".to_string(),
    }
}

pub fn parse_batch_priority(s: &str) -> Result<model::BatchPriority, RpcError> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "standard" => Ok(model::BatchPriority::Standard),
        "batch" => Ok(model::BatchPriority::Batch),
        "economy" => Ok(model::BatchPriority::Economy),
        _ => Err(RpcError::InvalidParams("invalid priority".to_string())),
    }
}

pub fn batch_priority_to_string(priority: model::BatchPriority) -> String {
    match priority {
        model::BatchPriority::Standard => "Standard".to_string(),
        model::BatchPriority::Batch => "Batch".to_string(),
        model::BatchPriority::Economy => "Economy".to_string(),
    }
}
