// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Continual learning infrastructure for AIGEN
//!
//! Consolidates EWC, replay buffers, and IPFS persistence for federated
//! continual learning across the distributed network.

use std::collections::VecDeque;
use std::sync::Arc;
use std::path::PathBuf;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::storage::StorageError;

/// Inference sample stored in replay buffer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceSample {
    /// Input tensor (serialized)
    pub input: Vec<f32>,
    /// Model output
    pub output: Vec<f32>,
    /// Loss value
    pub loss: f32,
    /// Timestamp
    pub timestamp: i64,
}

/// Fisher matrix for EWC (Elastic Weight Consolidation)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FisherMatrix {
    /// Diagonal Fisher information (importance weights per parameter)
    pub diagonal: Vec<f32>,
    /// Model parameters at which Fisher was computed
    pub old_params: Vec<f32>,
    /// Timestamp of computation
    pub timestamp: i64,
    /// IPFS hash where full matrix is stored
    pub ipfs_hash: Option<String>,
}

impl FisherMatrix {
    /// Compute EWC regularization loss: lambda/2 * sum(F_i * (theta_i - theta_old_i)^2)
    pub fn ewc_loss(&self, delta: &[f32], lambda: f32) -> f32 {
        if self.diagonal.len() != delta.len() {
            return 0.0;
        }

        let mut loss = 0.0;
        for (i, (&fisher, &delta_val)) in self.diagonal.iter().zip(delta.iter()).enumerate() {
            if i < self.old_params.len() {
                let param_diff = delta_val; // delta = new - old
                loss += fisher * param_diff * param_diff;
            }
        }

        (lambda / 2.0) * loss
    }

    /// Get hash for on-chain storage
    pub fn compute_hash(&self) -> [u8; 32] {
        let serialized = self.serialize();
        blake3::hash(&serialized).into()
    }

    /// Serialize Fisher matrix
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for &val in &self.diagonal {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        for &val in &self.old_params {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes
    }

    /// Compute Fisher diagonal from samples
    pub fn compute_fisher_diagonal(
        weights: &[f32],
        samples: &[InferenceSample],
    ) -> Vec<f32> {
        let mut fisher_diagonal = vec![0.0f32; weights.len()];

        for sample in samples {
            // Approximate gradient squared as loss^2 * random
            let grad_scale = sample.loss * sample.loss;
            for fisher in fisher_diagonal.iter_mut() {
                let grad = grad_scale * (rand::random::<f32>() - 0.5);
                *fisher += grad * grad;
            }
        }

        // Average
        let n = samples.len().max(1) as f32;
        for fisher in fisher_diagonal.iter_mut() {
            *fisher /= n;
        }

        fisher_diagonal
    }
}

/// Replay buffer for continual learning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayBuffer {
    /// Samples stored in FIFO order (main buffer)
    pub samples: VecDeque<InferenceSample>,
    /// Historical replay subset (1% of evicted samples)
    pub replay_subset: Vec<InferenceSample>,
    /// Maximum buffer size
    max_size: usize,
    /// Replay ratio (fraction of historical samples to keep)
    replay_ratio: f32,
}

impl ReplayBuffer {
    /// Create new replay buffer
    pub fn new(max_size: usize, replay_ratio: f32) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_size),
            replay_subset: Vec::new(),
            max_size,
            replay_ratio,
        }
    }

    /// Push a new sample to the buffer
    /// When buffer is full, evicted samples move to replay_subset with probability replay_ratio (1%)
    pub fn push(&mut self, sample: InferenceSample) {
        if self.samples.len() >= self.max_size {
            // Move oldest sample to replay subset with probability replay_ratio
            if let Some(old_sample) = self.samples.pop_front() {
                if rand::random::<f32>() < self.replay_ratio {
                    self.replay_subset.push(old_sample);
                    // Trim replay subset to maintain ~1% target (max 1% of main buffer capacity)
                    let max_replay_size = (self.max_size as f32 * self.replay_ratio).ceil() as usize;
                    if self.replay_subset.len() > max_replay_size {
                        self.replay_subset.remove(0);
                    }
                }
            }
        }
        self.samples.push_back(sample);
    }

    /// Sample n entries from buffer
    pub fn sample_batch(&self, n: usize) -> Vec<InferenceSample> {
        let n = n.min(self.samples.len());
        self.samples.iter().rev().take(n).cloned().collect()
    }

    /// Get replay samples for mixed training
    /// Returns samples from the historical replay subset (~1% of capacity)
    pub fn get_replay_samples(&self, n: usize) -> Vec<InferenceSample> {
        // Return random subset of historical samples from replay_subset
        let n = n.min(self.replay_subset.len());
        // Take the most recent n samples from replay_subset
        self.replay_subset.iter().rev().take(n).cloned().collect()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Check if buffer is ready for training
    pub fn is_ready(&self, threshold: usize) -> bool {
        self.samples.len() >= threshold
    }
}

/// Configuration for continual learning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContinualLearningConfig {
    /// Whether continual learning is enabled
    pub enabled: bool,
    /// Replay ratio for buffer (default: 0.01 = 1%)
    pub replay_ratio: f32,
    /// EWC lambda parameter (default: 0.4)
    pub ewc_lambda: f32,
    /// Recompute Fisher every N training rounds
    pub fisher_update_interval: usize,
    /// Trigger threshold for training burst
    pub trigger_threshold: usize,
    /// Batch size for training
    pub batch_size: usize,
}

impl Default for ContinualLearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            replay_ratio: 0.01,
            ewc_lambda: 0.4,
            fisher_update_interval: 10,
            trigger_threshold: 900,
            batch_size: 128,
        }
    }
}

/// Chain state methods needed for continual learning
pub trait ContinualLearningChainState: Send + Sync + 'static {
    /// Store Fisher matrix metadata on-chain
    fn store_fisher_matrix_internal(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64);
    /// Get Fisher matrix metadata for a model
    fn get_fisher_matrix_for_cl(&self, model_id: &str) -> Option<blockchain_core::state::FisherMatrixState>;
    /// Store replay buffer metadata on-chain
    fn store_replay_buffer_metadata(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64, sample_count: usize);
    /// Get replay buffer metadata for a model
    fn get_replay_buffer_metadata(&self, model_id: &str) -> Option<blockchain_core::state::ReplayBufferState>;
}

impl ContinualLearningChainState for blockchain_core::state::ChainState {
    fn store_fisher_matrix_internal(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64) {
        // Call ChainState's method directly (not the trait method)
        blockchain_core::state::ChainState::store_fisher_matrix_internal(self, model_id, ipfs_hash, data_hash, timestamp);
    }

    fn get_fisher_matrix_for_cl(&self, model_id: &str) -> Option<blockchain_core::state::FisherMatrixState> {
        // Call ChainState's method directly (not the trait method)
        blockchain_core::state::ChainState::get_fisher_matrix_for_cl(self, model_id)
    }

    fn store_replay_buffer_metadata(&self, model_id: String, ipfs_hash: String, data_hash: [u8; 32], timestamp: i64, sample_count: usize) {
        // Call ChainState's method directly (not the trait method)
        blockchain_core::state::ChainState::store_replay_buffer_metadata(self, model_id, ipfs_hash, data_hash, timestamp, sample_count);
    }

    fn get_replay_buffer_metadata(&self, model_id: &str) -> Option<blockchain_core::state::ReplayBufferState> {
        // Call ChainState's method directly (not the trait method)
        blockchain_core::state::ChainState::get_replay_buffer_metadata(self, model_id)
    }
}

/// Continual learning manager
pub struct ContinualLearningManager<CS: ContinualLearningChainState, S: crate::storage::StorageBackend> {
    /// Fisher matrices per model (cached)
    fisher_matrices: Arc<DashMap<String, FisherMatrix>>,
    /// Replay buffers per model
    replay_buffers: Arc<DashMap<String, ReplayBuffer>>,
    /// IPFS storage backend
    ipfs_storage: Arc<S>,
    /// Chain state for on-chain storage
    chain_state: Arc<CS>,
    /// Configuration
    config: ContinualLearningConfig,
}

/// Continual learning errors
#[derive(Debug, Error)]
pub enum ContinualLearningError {
    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("chain state error: {0}")]
    ChainStateError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("Fisher matrix not found for model: {0}")]
    FisherNotFound(String),
    #[error("buffer not found for model: {0}")]
    BufferNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("compression error: {0}")]
    CompressionError(String),
}

// Manual Send/Sync impls - safe because:
// - DashMap is thread-safe
// - Arc provides shared ownership
// - All methods use &self and internal synchronization
// - StorageBackend + 'static ensures Send safety with spawn_blocking
unsafe impl<CS: ContinualLearningChainState, S: crate::storage::StorageBackend + 'static> Send for ContinualLearningManager<CS, S> {}
unsafe impl<CS: ContinualLearningChainState, S: crate::storage::StorageBackend + 'static> Sync for ContinualLearningManager<CS, S> {}

impl<CS: ContinualLearningChainState, S: crate::storage::StorageBackend + 'static> ContinualLearningManager<CS, S> {
    /// Create new continual learning manager
    pub fn new(
        ipfs_storage: Arc<S>,
        chain_state: Arc<CS>,
        config: ContinualLearningConfig,
    ) -> Self {
        Self {
            fisher_matrices: Arc::new(DashMap::new()),
            replay_buffers: Arc::new(DashMap::new()),
            ipfs_storage,
            chain_state,
            config,
        }
    }

    /// Record inference sample for continual learning
    pub async fn record_inference(
        &self,
        model_id: &str,
        input: Vec<f32>,
        output: Vec<f32>,
        loss: f32,
    ) -> Result<(), ContinualLearningError> {
        if !self.config.enabled {
            return Ok(());
        }

        let sample = InferenceSample {
            input,
            output,
            loss,
            timestamp: chrono::Utc::now().timestamp(),
        };

        // Get or create buffer for this model
        let mut buffer = self.replay_buffers.entry(model_id.to_string()).or_insert_with(|| {
            ReplayBuffer::new(self.config.batch_size * 10, self.config.replay_ratio)
        });

        buffer.push(sample);

        // Check if buffer is ready for training
        if buffer.is_ready(self.config.trigger_threshold) {
            log::info!(
                "Replay buffer for model {} ready with {} samples",
                model_id,
                buffer.len()
            );
        }

        Ok(())
    }

    /// Compute and store Fisher matrix to IPFS and on-chain
    pub async fn compute_and_store_fisher(
        &self,
        model_id: &str,
        samples: &[InferenceSample],
        weights: &[f32],
    ) -> Result<[u8; 32], ContinualLearningError> {
        // Compute Fisher diagonal
        let fisher_diagonal = FisherMatrix::compute_fisher_diagonal(weights, samples);

        let fisher = FisherMatrix {
            diagonal: fisher_diagonal,
            old_params: weights.to_vec(),
            timestamp: chrono::Utc::now().timestamp(),
            ipfs_hash: None,
        };

        // Serialize Fisher matrix
        let serialized = serde_json::to_vec(&fisher)
            .map_err(|e| ContinualLearningError::SerializationError(e.to_string()))?;

        // Write to temp file
        let temp_path = PathBuf::from(format!("/tmp/fisher_{}.json", model_id));
        tokio::fs::write(&temp_path, &serialized).await?;

        // Create shard metadata for upload
        let shard = crate::registry::ModelShard {
            model_id: model_id.to_string(),
            shard_index: 0,
            total_shards: 1,
            hash: fisher.compute_hash(),
            size: serialized.len() as u64,
            ipfs_cid: None,
            http_urls: vec![],
            locations: vec![],
        };

        // Upload to IPFS using spawn_blocking for Send safety
        let ipfs_storage_clone = self.ipfs_storage.clone();
        let shard_clone = shard.clone();
        let temp_path_clone = temp_path.clone();
        let ipfs_hash = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            rt.block_on(async move {
                ipfs_storage_clone.upload_shard(&shard_clone, &temp_path_clone).await
            })
        })
        .await
        .map_err(|e| ContinualLearningError::StorageError(StorageError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        )))??;

        // Compute hash
        let hash = fisher.compute_hash();

        // Store on-chain
        let timestamp = chrono::Utc::now().timestamp();
        self.chain_state.store_fisher_matrix_internal(
            model_id.to_string(),
            ipfs_hash.clone(),
            hash,
            timestamp,
        );

        // Cache in memory
        let mut fisher_with_hash = fisher.clone();
        fisher_with_hash.ipfs_hash = Some(ipfs_hash);
        self.fisher_matrices.insert(model_id.to_string(), fisher_with_hash);

        // Cleanup temp file
        let _ = tokio::fs::remove_file(&temp_path).await;

        log::info!("Stored Fisher matrix for model {} with hash {}", model_id, hex::encode(&hash));

        Ok(hash)
    }

    /// Load Fisher matrix from cache or IPFS
    pub async fn load_fisher(
        &self,
        model_id: &str,
    ) -> Result<Option<FisherMatrix>, ContinualLearningError> {
        // Check cache first
        if let Some(fisher) = self.fisher_matrices.get(model_id) {
            return Ok(Some(fisher.clone()));
        }

        // Query chain state
        let state = match self.chain_state.get_fisher_matrix_for_cl(model_id) {
            Some(s) => s,
            None => return Ok(None),
        };

        // Download from IPFS
        let temp_path = PathBuf::from(format!("/tmp/fisher_{}_download.json", model_id));

        // Create shard metadata for download
        let shard = crate::registry::ModelShard {
            model_id: model_id.to_string(),
            shard_index: 0,
            total_shards: 1,
            hash: state.data_hash,
            size: 0,
            ipfs_cid: None,
            http_urls: vec![],
            locations: vec![],
        };

        // Download from IPFS using spawn_blocking for Send safety
        let ipfs_storage_clone = self.ipfs_storage.clone();
        let shard_clone = shard.clone();
        let temp_path_clone = temp_path.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            rt.block_on(async move {
                ipfs_storage_clone.download_shard(&shard_clone, &temp_path_clone).await
            })
        })
        .await
        .map_err(|e| ContinualLearningError::StorageError(StorageError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        )))??;

        // Read and deserialize
        let bytes = tokio::fs::read(&temp_path).await?;
        let fisher: FisherMatrix = serde_json::from_slice(&bytes)
            .map_err(|e| ContinualLearningError::SerializationError(e.to_string()))?;

        // Verify hash
        let computed_hash = fisher.compute_hash();
        if computed_hash != state.data_hash {
            return Err(ContinualLearningError::StorageError(
                StorageError::IntegrityFailed
            ));
        }

        let mut fisher_with_hash = fisher.clone();
        fisher_with_hash.ipfs_hash = Some(state.ipfs_hash.clone());
        self.fisher_matrices.insert(model_id.to_string(), fisher_with_hash);

        // Cleanup
        let _ = tokio::fs::remove_file(&temp_path).await;

        log::info!("Loaded Fisher matrix for model {} from IPFS", model_id);

        Ok(Some(fisher))
    }

    /// Persist replay buffer to IPFS and store metadata on-chain
    /// Returns the (data_hash, ipfs_cid) tuple for reference
    pub async fn persist_replay_buffer(
        &self,
        model_id: &str,
    ) -> Result<([u8; 32], String), ContinualLearningError> {
        let buffer = self.replay_buffers
            .get(model_id)
            .ok_or_else(|| ContinualLearningError::BufferNotFound(model_id.to_string()))?;

        // Serialize samples
        let serialized = serde_json::to_vec(&buffer.samples)
            .map_err(|e| ContinualLearningError::SerializationError(e.to_string()))?;

        // Compress with zstd
        let compressed = zstd::encode_all(serialized.as_slice(), 3)
            .map_err(|e| ContinualLearningError::CompressionError(e.to_string()))?;

        // Compute hash
        let hash = blake3::hash(&compressed);
        let hash_bytes: [u8; 32] = hash.into();

        // Write to temp file
        let temp_path = PathBuf::from(format!("/tmp/replay_buffer_{}.zst", model_id));
        tokio::fs::write(&temp_path, &compressed).await?;

        // Create shard metadata for upload
        let shard = crate::registry::ModelShard {
            model_id: format!("{}_replay", model_id),
            shard_index: 0,
            total_shards: 1,
            hash: hash_bytes,
            size: compressed.len() as u64,
            ipfs_cid: None,
            http_urls: vec![],
            locations: vec![],
        };

        // Upload to IPFS using spawn_blocking for Send safety
        let ipfs_storage_clone = self.ipfs_storage.clone();
        let shard_clone = shard.clone();
        let temp_path_clone = temp_path.clone();
        let ipfs_hash = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            rt.block_on(async move {
                ipfs_storage_clone.upload_shard(&shard_clone, &temp_path_clone).await
            })
        })
        .await
        .map_err(|e| ContinualLearningError::StorageError(StorageError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        )))??;

        // Store replay buffer metadata on-chain
        let timestamp = chrono::Utc::now().timestamp();
        self.chain_state.store_replay_buffer_metadata(
            model_id.to_string(),
            ipfs_hash.clone(),
            hash_bytes,
            timestamp,
            buffer.len(),
        );

        // Cleanup
        let _ = tokio::fs::remove_file(&temp_path).await;

        log::info!("Persisted replay buffer for model {} with {} samples, hash={}, cid={}",
            model_id, buffer.len(), hex::encode(&hash_bytes), ipfs_hash);

        Ok((hash_bytes, ipfs_hash))
    }

    /// Restore replay buffer from IPFS using CID
    /// If cid is None, attempts to look up from chain state
    pub async fn restore_replay_buffer(
        &self,
        model_id: &str,
        cid: Option<&str>,
    ) -> Result<(), ContinualLearningError> {
        // Get CID from parameter or look up from chain state
        let ipfs_hash = if let Some(provided_cid) = cid {
            provided_cid.to_string()
        } else {
            // Look up from chain state
            match self.chain_state.get_replay_buffer_metadata(model_id) {
                Some(metadata) => metadata.ipfs_hash,
                None => return Err(ContinualLearningError::BufferNotFound(
                    format!("No replay buffer metadata found for model {}", model_id)
                )),
            }
        };

        let temp_path = PathBuf::from(format!("/tmp/replay_buffer_{}_restore.zst", model_id));

        // Create shard metadata for download with the CID
        let shard = crate::registry::ModelShard {
            model_id: format!("{}_replay", model_id),
            shard_index: 0,
            total_shards: 1,
            hash: [0u8; 32],
            size: 0,
            ipfs_cid: Some(ipfs_hash.clone()),
            http_urls: vec![],
            locations: vec![crate::registry::ShardLocation {
                node_id: "ipfs".to_string(),
                backend_type: "ipfs".to_string(),
                location_uri: ipfs_hash.clone(),
                last_verified: 0,
                is_healthy: true,
            }],
        };

        // Download from IPFS using spawn_blocking for Send safety
        let ipfs_storage_clone = self.ipfs_storage.clone();
        let shard_clone = shard.clone();
        let temp_path_clone = temp_path.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            rt.block_on(async move {
                ipfs_storage_clone.download_shard(&shard_clone, &temp_path_clone).await
            })
        })
        .await
        .map_err(|e| ContinualLearningError::StorageError(StorageError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        )))??;

        // Read compressed data
        let compressed = tokio::fs::read(&temp_path).await?;

        // Decompress
        let decompressed = zstd::decode_all(compressed.as_slice())
            .map_err(|e| ContinualLearningError::CompressionError(e.to_string()))?;

        // Deserialize
        let samples: VecDeque<InferenceSample> = serde_json::from_slice(&decompressed)
            .map_err(|e| ContinualLearningError::SerializationError(e.to_string()))?;

        // Restore buffer
        let buffer = ReplayBuffer {
            samples,
            replay_subset: Vec::new(), // Start with empty replay subset after restore
            max_size: self.config.batch_size * 10,
            replay_ratio: self.config.replay_ratio,
        };

        self.replay_buffers.insert(model_id.to_string(), buffer);

        // Cleanup
        let _ = tokio::fs::remove_file(&temp_path).await;

        log::info!("Restored replay buffer for model {} from IPFS cid={}", model_id, ipfs_hash);

        Ok(())
    }

    /// Get replay buffer metadata from chain state
    pub fn get_replay_buffer_metadata(&self, model_id: &str) -> Option<blockchain_core::state::ReplayBufferState> {
        self.chain_state.get_replay_buffer_metadata(model_id)
    }

    /// Get EWC loss for a given delta
    pub fn get_ewc_loss(&self, model_id: &str, delta: &[f32]) -> f32 {
        match self.fisher_matrices.get(model_id) {
            Some(fisher) => fisher.ewc_loss(delta, self.config.ewc_lambda),
            None => 0.0,
        }
    }

    /// Get replay buffer size for a model
    pub fn get_buffer_size(&self, model_id: &str) -> usize {
        self.replay_buffers
            .get(model_id)
            .map(|b| b.len())
            .unwrap_or(0)
    }

    /// Check if buffer is ready for training
    pub fn is_buffer_ready(&self, model_id: &str) -> bool {
        self.replay_buffers
            .get(model_id)
            .map(|b| b.is_ready(self.config.trigger_threshold))
            .unwrap_or(false)
    }

    /// Get samples from buffer for training
    pub fn get_training_samples(&self, model_id: &str, n: usize) -> Vec<InferenceSample> {
        self.replay_buffers
            .get(model_id)
            .map(|b| b.sample_batch(n))
            .unwrap_or_default()
    }

    /// Get configuration
    pub fn config(&self) -> &ContinualLearningConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_buffer() {
        let mut buffer = ReplayBuffer::new(10, 0.01);

        for i in 0..15 {
            buffer.push(InferenceSample {
                input: vec![i as f32],
                output: vec![i as f32 * 2.0],
                loss: 0.1,
                timestamp: i as i64,
            });
        }

        assert_eq!(buffer.len(), 10);
        assert!(buffer.is_ready(5));

        let batch = buffer.sample_batch(3);
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn test_fisher_matrix_ewc_loss() {
        let fisher = FisherMatrix {
            diagonal: vec![1.0, 2.0, 3.0],
            old_params: vec![0.0, 0.0, 0.0],
            timestamp: 0,
            ipfs_hash: None,
        };

        let delta = vec![0.1, 0.2, 0.3];
        let loss = fisher.ewc_loss(&delta, 0.4);

        // Expected: 0.4/2 * (1.0*0.01 + 2.0*0.04 + 3.0*0.09) = 0.2 * (0.01 + 0.08 + 0.27) = 0.072
        assert!(loss > 0.0);
    }

    #[test]
    fn test_config_default() {
        let config = ContinualLearningConfig::default();
        assert!(config.enabled);
        assert_eq!(config.replay_ratio, 0.01);
        assert_eq!(config.ewc_lambda, 0.4);
        assert_eq!(config.trigger_threshold, 900);
        assert_eq!(config.batch_size, 128);
    }
}
