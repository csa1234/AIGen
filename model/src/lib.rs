//! Model registry and metadata layer for AIGEN's proof-of-intelligence consensus.
//!
//! The registry tracks model metadata and shard hashes referenced by
//! `ComputationMetadata::model_id`, providing a foundation for PoI verification
//! and future distributed storage backends.

pub mod registry;
pub mod sharding;
pub mod storage;
pub mod inference;
pub mod verification;
pub mod tiers;
pub mod batch;

pub use registry::{ModelError, ModelMetadata, ModelRegistry, ModelShard, ShardLocation};
pub use sharding::{combine_shards, split_model_file, verify_shard_integrity, ShardError};
pub use storage::{HttpStorage, IpfsStorage, LocalStorage, StorageBackend, StorageError};
pub use inference::{
    InferenceEngine,
    InferenceError,
    InferenceMetrics,
    InferenceOutput,
    InferenceStats,
    InferenceTensor,
    LoadedModel,
    ModelCache,
    SharedInferenceMetrics,
};
pub use verification::{
    deterministic_inference,
    deterministic_inference_with_engine,
    global_inference_engine,
    hash_inference_key,
    model_exists,
    outputs_match,
    set_global_inference_engine,
    VerificationCache,
    VerificationError,
    DEFAULT_VERIFICATION_CACHE_CAPACITY,
    DEFAULT_VERIFICATION_EPSILON,
};
pub use tiers::{
    AccessLogEntry,
    DefaultPaymentProvider,
    PaymentOutcome,
    PaymentProvider,
    PaymentRecord,
    PaymentStatus,
    RateLimitDecision,
    Subscription,
    SubscriptionStatus,
    SubscriptionTier,
    TierConfig,
    TierError,
    TierManager,
    default_tier_configs,
    now_timestamp,
    resolve_model_access,
    tier_from_address,
};
pub use batch::{
    BatchError,
    BatchJob,
    BatchJobInfo,
    BatchJobMetrics,
    BatchJobStatus,
    BatchPaymentPayload,
    BatchPriority,
    BatchQueue,
    BatchRequest,
    BatchWorker,
    CompletionEstimate,
    QueueStats,
    VolumeDiscountTracker,
    calculate_volume_discount,
    validate_batch_payment,
};
