// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Model registry and metadata layer for AIGEN's proof-of-intelligence consensus.
//!
//! The registry tracks model metadata and shard hashes referenced by
//! `ComputationMetadata::model_id`, providing a foundation for PoI verification
//! and future distributed storage backends.

pub mod ads;
pub mod batch;
pub mod continual_learning;
pub mod inference;
pub mod registry;
pub mod sharding;
pub mod storage;
pub mod tiers;
pub mod verification;

pub use ads::{
    default_ad_templates, AdConfig, AdError, AdImpression, AdImpressionLog, AdManager, AdTemplate,
    UpgradePromptType,
};
pub use batch::{
    calculate_volume_discount, validate_batch_payment, BatchError, BatchJob, BatchJobInfo,
    BatchJobMetrics, BatchJobStatus, BatchPaymentPayload, BatchPriority, BatchQueue, BatchRequest,
    BatchWorker, CompletionEstimate, QueueStats, VolumeDiscountTracker, CHAT_PRICE_PER_1K_TOKENS,
};
pub use inference::{
    BlockExecutor, InferenceEngine, InferenceError, InferenceMetrics, InferenceOutput,
    InferenceStats, InferenceTensor, LayerBlockConfig, LoadedModel, ModelBlockSplitter, ModelCache,
    SharedInferenceMetrics,
};
pub use registry::{ModelError, ModelMetadata, ModelRegistry, ModelShard, ShardLocation};
pub use sharding::{combine_shards, split_model_file, verify_shard_integrity, ShardError};
pub use storage::{HttpStorage, IpfsStorage, LocalStorage, StorageBackend, StorageError};
pub use tiers::{
    default_tier_configs, now_timestamp, resolve_model_access, tier_from_address, AccessLogEntry,
    DefaultPaymentProvider, PaymentOutcome, PaymentProvider, PaymentRecord, PaymentStatus,
    RateLimitDecision, Subscription, SubscriptionStatus, SubscriptionTier, TierConfig, TierError,
    TierManager,
};
pub use verification::{
    deterministic_inference, deterministic_inference_with_engine, global_inference_engine,
    hash_inference_key, model_exists, outputs_match, set_global_inference_engine,
    VerificationCache, VerificationError, DEFAULT_VERIFICATION_CACHE_CAPACITY,
    DEFAULT_VERIFICATION_EPSILON,
};
pub use continual_learning::{
    ContinualLearningConfig, ContinualLearningError, ReplayBuffer,
    FisherMatrix, InferenceSample, ContinualLearningChainState,
};
pub type ContinualLearningManager = continual_learning::ContinualLearningManager<blockchain_core::state::ChainState, crate::storage::IpfsStorage>;
pub use blockchain_core::state::FisherMatrixState;
