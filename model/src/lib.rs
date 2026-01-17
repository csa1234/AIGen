//! Model registry and metadata layer for AIGEN's proof-of-intelligence consensus.
//!
//! The registry tracks model metadata and shard hashes referenced by
//! `ComputationMetadata::model_id`, providing a foundation for PoI verification
//! and future distributed storage backends.

pub mod registry;
pub mod sharding;
pub mod storage;

pub use registry::{ModelError, ModelMetadata, ModelRegistry, ModelShard, ShardLocation};
pub use sharding::{combine_shards, split_model_file, verify_shard_integrity, ShardError};
pub use storage::{HttpStorage, IpfsStorage, LocalStorage, StorageBackend, StorageError};
