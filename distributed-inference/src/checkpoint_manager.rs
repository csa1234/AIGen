// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Checkpoint Manager for Distributed Inference
//!
//! Manages checkpoint lifecycle with LRU eviction and configurable storage backends.
//! Checkpoints are saved every N tokens (default 10) for recovery during failover.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::pipeline_message::Checkpoint;
use crate::tensor_transport::CompressedTensor;

/// Key for identifying a checkpoint in storage
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CheckpointKey {
    pub inference_id: Uuid,
    pub block_id: u32,
    pub token_index: u32,
}

/// Stored checkpoint with metadata for LRU eviction
#[derive(Clone, Debug)]
pub struct StoredCheckpoint {
    pub checkpoint: Checkpoint,
    pub size_bytes: usize,
    pub created_at: Instant,
    pub access_count: std::sync::atomic::AtomicU32,
}

impl StoredCheckpoint {
    pub fn new(checkpoint: Checkpoint, size_bytes: usize) -> Self {
        Self {
            checkpoint,
            size_bytes,
            created_at: Instant::now(),
            access_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn record_access(&self) {
        self.access_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Storage backend options for checkpoints
#[derive(Clone, Debug)]
pub enum CheckpointStorageBackend {
    /// In-memory storage using DashMap (fastest, no persistence)
    Memory,
    /// Disk-based storage with optional persistence
    Disk(PathBuf),
    /// Distributed storage across multiple nodes (future implementation)
    Distributed,
}

impl Default for CheckpointStorageBackend {
    fn default() -> Self {
        CheckpointStorageBackend::Memory
    }
}

/// Configuration for checkpoint manager
#[derive(Clone, Debug)]
pub struct CheckpointConfig {
    /// Save checkpoint every N tokens (default: 10)
    pub checkpoint_interval: u32,
    /// Maximum checkpoints to keep per inference (default: 5)
    pub max_checkpoints_per_inference: usize,
    /// Storage backend selection
    pub storage_backend: CheckpointStorageBackend,
    /// Maximum total checkpoints across all inferences (LRU eviction)
    pub max_total_checkpoints: usize,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            checkpoint_interval: 10,
            max_checkpoints_per_inference: 5,
            storage_backend: CheckpointStorageBackend::Memory,
            max_total_checkpoints: 1000,
        }
    }
}

/// Manager for checkpoint storage, retrieval, and lifecycle
pub struct CheckpointManager {
    storage: Arc<DashMap<CheckpointKey, StoredCheckpoint>>,
    config: RwLock<CheckpointConfig>,
    /// Track inference -> checkpoints mapping for cleanup
    inference_checkpoints: Arc<DashMap<Uuid, Vec<CheckpointKey>>>,
    /// Total storage bytes tracked
    total_storage_bytes: std::sync::atomic::AtomicUsize,
}

impl CheckpointManager {
    /// Create a new checkpoint manager with default configuration
    pub fn new() -> Self {
        Self::with_config(CheckpointConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: CheckpointConfig) -> Self {
        Self {
            storage: Arc::new(DashMap::new()),
            config: RwLock::new(config),
            inference_checkpoints: Arc::new(DashMap::new()),
            total_storage_bytes: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Save a checkpoint and return its hash
    pub async fn save_checkpoint(
        &self,
        inference_id: Uuid,
        block_id: u32,
        token_index: u32,
        tensor: CompressedTensor,
    ) -> Result<[u8; 32], CheckpointManagerError> {
        let checkpoint = Checkpoint::new(inference_id, block_id, tensor);
        let checkpoint_hash = checkpoint.checkpoint_hash;
        let size_bytes = std::mem::size_of::<Checkpoint>() + checkpoint.tensor.data.len();

        let key = CheckpointKey {
            inference_id,
            block_id,
            token_index,
        };

        let stored = StoredCheckpoint::new(checkpoint, size_bytes);

        // Store in memory
        self.storage.insert(key.clone(), stored);
        
        // Track inference -> checkpoint mapping
        self.inference_checkpoints
            .entry(inference_id)
            .or_insert_with(Vec::new)
            .push(key);

        // Update total storage
        self.total_storage_bytes
            .fetch_add(size_bytes, std::sync::atomic::Ordering::Relaxed);

        // Evict old checkpoints if necessary
        self.evict_old_checkpoints().await;

        tracing::debug!(
            "Checkpoint saved: inference={}, block={}, token={}, hash={:02x?}",
            inference_id, block_id, token_index, &checkpoint_hash[..8]
        );

        Ok(checkpoint_hash)
    }

    /// Get the latest checkpoint for an inference and block
    pub fn get_latest_checkpoint(
        &self,
        inference_id: Uuid,
        block_id: u32,
    ) -> Option<Checkpoint> {
        // Find all checkpoints for this inference and block
        let checkpoints: Vec<_> = self
            .storage
            .iter()
            .filter(|entry| {
                entry.key().inference_id == inference_id && entry.key().block_id == block_id
            })
            .collect();

        if checkpoints.is_empty() {
            return None;
        }

        // Get the one with highest token index (latest)
        let latest = checkpoints
            .into_iter()
            .max_by_key(|entry| entry.key().token_index)?;

        // Record access for LRU
        latest.value().record_access();

        Some(latest.value().checkpoint.clone())
    }

    /// Get checkpoint before a specific token index (for precise recovery)
    pub fn get_checkpoint_before_token(
        &self,
        inference_id: Uuid,
        block_id: u32,
        token_index: u32,
    ) -> Option<Checkpoint> {
        let checkpoints: Vec<_> = self
            .storage
            .iter()
            .filter(|entry| {
                let key = entry.key();
                key.inference_id == inference_id
                    && key.block_id == block_id
                    && key.token_index < token_index
            })
            .collect();

        if checkpoints.is_empty() {
            return None;
        }

        // Get the one with highest token index that's still < token_index
        let latest = checkpoints
            .into_iter()
            .max_by_key(|entry| entry.key().token_index)?;

        latest.value().record_access();
        Some(latest.value().checkpoint.clone())
    }

    /// Get a specific checkpoint by key
    pub fn get_checkpoint(&self, key: &CheckpointKey) -> Option<Checkpoint> {
        self.storage.get(key).map(|entry| {
            entry.value().record_access();
            entry.value().checkpoint.clone()
        })
    }

    /// Remove all checkpoints for a completed inference
    pub async fn cleanup_inference(&self, inference_id: Uuid) {
        if let Some((_, keys)) = self.inference_checkpoints.remove(&inference_id) {
            for key in keys {
                if let Some((_, stored)) = self.storage.remove(&key) {
                    self.total_storage_bytes
                        .fetch_sub(stored.size_bytes, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        tracing::debug!("Cleaned up all checkpoints for inference {}", inference_id);
    }

    /// LRU eviction when storage limit is reached
    async fn evict_old_checkpoints(&self) {
        let max_checkpoints = self.config.read().await.max_total_checkpoints;
        let current_count = self.storage.len();

        if current_count <= max_checkpoints {
            return;
        }

        // Need to evict - find oldest, least accessed checkpoints
        let to_evict = current_count - max_checkpoints;

        // Collect all entries with their access info
        let mut entries: Vec<_> = self
            .storage
            .iter()
            .map(|entry| {
                let key = entry.key().clone();
                let stored = entry.value();
                let access_count = stored.access_count.load(std::sync::atomic::Ordering::Relaxed);
                let age = stored.created_at.elapsed();
                (key, access_count, age, stored.size_bytes)
            })
            .collect();

        // Sort by access count (ascending) then age (descending)
        entries.sort_by(|a, b| {
            a.1.cmp(&b.1) // Lower access count first
                .then_with(|| b.2.cmp(&a.2)) // Older first
        });

        // Evict the least accessed, oldest entries
        for (key, _, _, size_bytes) in entries.into_iter().take(to_evict) {
            if let Some((_, stored)) = self.storage.remove(&key) {
                self.total_storage_bytes
                    .fetch_sub(size_bytes, std::sync::atomic::Ordering::Relaxed);
                
                // Also remove from inference mapping
                if let Some(mut inference_keys) = self.inference_checkpoints.get_mut(&key.inference_id) {
                    inference_keys.retain(|k| k != &key);
                }

                tracing::debug!(
                    "Evicted checkpoint: inference={}, block={}, token={}",
                    key.inference_id, key.block_id, key.token_index
                );
            }
        }
    }

    /// Get current storage statistics
    pub fn get_stats(&self) -> CheckpointStats {
        CheckpointStats {
            total_checkpoints: self.storage.len() as u64,
            total_storage_bytes: self.total_storage_bytes.load(std::sync::atomic::Ordering::Relaxed) as u64,
            inferences_tracked: self.inference_checkpoints.len() as u64,
        }
    }

    /// Update configuration
    pub async fn set_config(&self, config: CheckpointConfig) {
        *self.config.write().await = config;
    }

    /// Get current configuration
    pub async fn get_config(&self) -> CheckpointConfig {
        self.config.read().await.clone()
    }

    /// Get checkpoint interval
    pub async fn checkpoint_interval(&self) -> u32 {
        self.config.read().await.checkpoint_interval
    }

    /// Should we save a checkpoint at this token index?
    pub async fn should_checkpoint(&self, token_index: u32) -> bool {
        let interval = self.config.read().await.checkpoint_interval;
        token_index % interval == 0
    }
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Error types for checkpoint operations
#[derive(Debug, thiserror::Error)]
pub enum CheckpointManagerError {
    #[error("checkpoint storage failed: {0}")]
    StorageFailed(String),
    #[error("checkpoint not found: inference={inference_id}, block={block_id}")]
    NotFound { inference_id: Uuid, block_id: u32 },
    #[error("integrity check failed for checkpoint")]
    IntegrityFailed,
    #[error("storage backend error: {0}")]
    BackendError(String),
}

/// Statistics for checkpoint storage
#[derive(Clone, Debug, Default)]
pub struct CheckpointStats {
    pub total_checkpoints: u64,
    pub total_storage_bytes: u64,
    pub inferences_tracked: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor_transport::{TensorMetadata, TensorTransport};

    fn create_test_tensor() -> CompressedTensor {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
        };
        let data: Vec<u8> = vec![0; 100];
        TensorTransport::compress(&data, metadata, 1).unwrap()
    }

    #[tokio::test]
    async fn test_checkpoint_save_and_load() {
        let manager = CheckpointManager::new();
        let inference_id = Uuid::new_v4();
        let tensor = create_test_tensor();

        // Save checkpoint
        let hash = manager
            .save_checkpoint(inference_id, 0, 10, tensor)
            .await
            .unwrap();

        assert_ne!(hash, [0u8; 32]);

        // Load latest
        let loaded = manager.get_latest_checkpoint(inference_id, 0);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().checkpoint_hash, hash);
    }

    #[tokio::test]
    async fn test_get_checkpoint_before_token() {
        let manager = CheckpointManager::new();
        let inference_id = Uuid::new_v4();

        // Save checkpoints at tokens 0, 10, 20
        for token in [0, 10, 20] {
            let tensor = create_test_tensor();
            manager
                .save_checkpoint(inference_id, 0, token, tensor)
                .await
                .unwrap();
        }

        // Get checkpoint before token 15 - should return token 10
        let checkpoint = manager.get_checkpoint_before_token(inference_id, 0, 15);
        assert!(checkpoint.is_some());
        assert_eq!(checkpoint.unwrap().block_id, 0);
    }

    #[tokio::test]
    async fn test_cleanup_inference() {
        let manager = CheckpointManager::new();
        let inference_id = Uuid::new_v4();

        // Save some checkpoints
        for token in [0, 10, 20] {
            let tensor = create_test_tensor();
            manager
                .save_checkpoint(inference_id, 0, token, tensor)
                .await
                .unwrap();
        }

        assert_eq!(manager.get_stats().total_checkpoints, 3);

        // Cleanup
        manager.cleanup_inference(inference_id).await;

        assert_eq!(manager.get_stats().total_checkpoints, 0);
        assert!(manager.get_latest_checkpoint(inference_id, 0).is_none());
    }

    #[tokio::test]
    async fn test_should_checkpoint() {
        let config = CheckpointConfig {
            checkpoint_interval: 10,
            ..Default::default()
        };
        let manager = CheckpointManager::with_config(config);

        assert!(manager.should_checkpoint(0).await);
        assert!(manager.should_checkpoint(10).await);
        assert!(manager.should_checkpoint(20).await);
        assert!(!manager.should_checkpoint(5).await);
        assert!(!manager.should_checkpoint(15).await);
    }

    #[tokio::test]
    async fn test_eviction() {
        let config = CheckpointConfig {
            max_total_checkpoints: 2,
            ..Default::default()
        };
        let manager = CheckpointManager::with_config(config);
        let inference_id = Uuid::new_v4();

        // Save 3 checkpoints (should trigger eviction of 1)
        for token in [0, 10, 20] {
            let tensor = create_test_tensor();
            manager
                .save_checkpoint(inference_id, 0, token, tensor)
                .await
                .unwrap();
        }

        // Should have only 2 after eviction
        assert_eq!(manager.get_stats().total_checkpoints, 2);
    }
}
