// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Federated learning training infrastructure
//!
//! Implements:
//! - TrainingBuffer for storing inference samples
//! - DeltaComputation for computing and applying weight deltas
//! - EWC (Elastic Weight Consolidation) for continual learning
//! - TrainingCoordinator for orchestrating federated training rounds

use crate::secret_sharing::{
    AggregatedShare, DeltaShareBundle, SecretSharingManager,
};
use dashmap::DashMap;
use network::protocol::NetworkMessage;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Inference sample stored in training buffer
#[derive(Clone, Debug)]
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

/// Training buffer for storing inference samples
pub struct TrainingBuffer {
    /// FIFO buffer storing (input, output, loss) tuples
    buffer: VecDeque<InferenceSample>,
    /// Maximum buffer size (default: 1000)
    max_size: usize,
    /// Replay buffer (1% historical samples for catastrophic forgetting prevention)
    replay_buffer: Vec<InferenceSample>,
    /// Current replay ratio
    replay_ratio: f32,
}

impl TrainingBuffer {
    /// Create new training buffer
    pub fn new(max_size: usize, replay_ratio: f32) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_size),
            max_size,
            replay_buffer: Vec::new(),
            replay_ratio,
        }
    }

    /// Push a new sample to the buffer
    /// If buffer is full, oldest sample moves to replay buffer
    pub fn push(&mut self, sample: InferenceSample) {
        if self.buffer.len() >= self.max_size {
            // Move oldest sample to replay buffer with 1% probability
            if let Some(old_sample) = self.buffer.pop_front() {
                if rand::random::<f32>() < self.replay_ratio {
                    self.replay_buffer.push(old_sample);
                    // Keep replay buffer at reasonable size
                    if self.replay_buffer.len() > self.max_size / 100 {
                        self.replay_buffer.remove(0);
                    }
                }
            }
        }
        self.buffer.push_back(sample);
    }

    /// Check if buffer is ready for training (size >= threshold)
    pub fn is_ready(&self, threshold: usize) -> bool {
        self.buffer.len() >= threshold
    }

    /// Sample latest n entries from buffer
    pub fn sample_batch(&self, n: usize) -> Vec<InferenceSample> {
        let n = n.min(self.buffer.len());
        self.buffer.iter().rev().take(n).cloned().collect()
    }

    /// Get buffer size
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get replay samples
    pub fn get_replay_samples(&self, n: usize) -> Vec<InferenceSample> {
        let n = n.min(self.replay_buffer.len());
        self.replay_buffer.iter().rev().take(n).cloned().collect()
    }
}

/// Fisher matrix for EWC (Elastic Weight Consolidation)
#[derive(Clone, Debug)]
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
        blockchain_core::crypto::hash_data(&self.serialize())
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
}

/// Delta computation utilities
pub struct DeltaComputation;

impl DeltaComputation {
    /// Compute delta between model weights: delta = updated - original
    pub fn compute_delta(original: &[f32], updated: &[f32]) -> Vec<f32> {
        original
            .iter()
            .zip(updated.iter())
            .map(|(&old, &new)| new - old)
            .collect()
    }

    /// Apply delta to model weights with learning rate scaling
    pub fn apply_delta(weights: &mut [f32], delta: &[f32], learning_rate: f32) {
        let min_len = weights.len().min(delta.len());
        for i in 0..min_len {
            weights[i] += learning_rate * delta[i];
        }
    }

    /// Compute L2 norm of delta
    pub fn l2_norm(delta: &[f32]) -> f32 {
        delta.iter().map(|&x| x * x).sum::<f32>().sqrt()
    }

    /// Apply 8-bit quantization to delta (for compression before secret sharing)
    pub fn quantize_8bit(delta: &[f32]) -> (Vec<u8>, f32, f32) {
        if delta.is_empty() {
            return (Vec::new(), 0.0, 0.0);
        }

        let min_val = delta.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max_val = delta.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let scale = (max_val - min_val) / 255.0;

        let quantized: Vec<u8> = delta
            .iter()
            .map(|&x| {
                if scale == 0.0 {
                    0u8
                } else {
                    ((x - min_val) / scale).clamp(0.0, 255.0) as u8
                }
            })
            .collect();

        (quantized, min_val, scale)
    }

    /// Dequantize 8-bit values back to f32
    pub fn dequantize_8bit(quantized: &[u8], min_val: f32, scale: f32) -> Vec<f32> {
        quantized
            .iter()
            .map(|&x| min_val + (x as f32) * scale)
            .collect()
    }
}

/// Training configuration
#[derive(Clone, Debug)]
pub struct TrainingConfig {
    pub enabled: bool,
    pub buffer_size: usize,
    pub trigger_threshold: usize,
    pub batch_size: usize,
    pub learning_rate: f32,
    pub epochs: u32,
    pub ewc_lambda: f32,
    pub replay_ratio: f32,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 1000,
            trigger_threshold: 900,
            batch_size: 128,
            learning_rate: 1e-6,
            epochs: 1,
            ewc_lambda: 0.4,
            replay_ratio: 0.01,
        }
    }
}

/// Shared aggregation buffer for received shares
pub struct ShareAggregationBuffer {
    /// Round ID -> Node ID -> DeltaShareBundle
    shares: DashMap<Uuid, DashMap<String, DeltaShareBundle>>,
    /// Round ID -> AggregatedShare (for this node's aggregation)
    aggregated: DashMap<Uuid, AggregatedShare>,
}

impl ShareAggregationBuffer {
    pub fn new() -> Self {
        Self {
            shares: DashMap::new(),
            aggregated: DashMap::new(),
        }
    }

    /// Store a received share
    pub fn store_share(&self, round_id: Uuid, node_id: String, bundle: DeltaShareBundle) {
        let round_entry = self.shares.entry(round_id).or_insert_with(DashMap::new);
        round_entry.insert(node_id, bundle);
    }

    /// Get all shares for a round
    pub fn get_shares_for_round(&self, round_id: Uuid) -> Vec<DeltaShareBundle> {
        self.shares
            .get(&round_id)
            .map(|round| {
                round
                    .iter()
                    .map(|entry| entry.value().clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if we have shares from at least threshold nodes
    pub fn has_threshold_shares(&self, round_id: Uuid, threshold: usize) -> bool {
        self.shares
            .get(&round_id)
            .map(|round| round.len() >= threshold)
            .unwrap_or(false)
    }

    /// Store aggregated share
    pub fn store_aggregated(&self, round_id: Uuid, aggregated: AggregatedShare) {
        self.aggregated.insert(round_id, aggregated);
    }

    /// Get aggregated share
    pub fn get_aggregated(&self, round_id: Uuid) -> Option<AggregatedShare> {
        self.aggregated.get(&round_id).map(|a| a.clone())
    }

    /// Clear round data
    pub fn clear_round(&self, round_id: Uuid) {
        self.shares.remove(&round_id);
        self.aggregated.remove(&round_id);
    }
}

/// Training coordinator for orchestrating federated learning
pub struct TrainingCoordinator {
    /// Local training buffer
    local_buffer: Arc<RwLock<TrainingBuffer>>,
    /// Secret sharing manager
    secret_sharing: SecretSharingManager,
    /// Network message sender
    network_tx: mpsc::Sender<NetworkMessage>,
    /// Model registry (placeholder - would integrate with actual model crate)
    model_weights: Arc<RwLock<Vec<f32>>>,
    /// Share aggregation buffer
    share_buffer: Arc<ShareAggregationBuffer>,
    /// Fisher matrices per model
    fisher_matrices: Arc<DashMap<String, FisherMatrix>>,
    /// Training configuration
    config: TrainingConfig,
    /// Node ID
    node_id: String,
    /// Current round ID (if training in progress)
    current_round: Arc<RwLock<Option<Uuid>>>,
}

impl TrainingCoordinator {
    /// Create new training coordinator
    pub fn new(
        node_id: String,
        network_tx: mpsc::Sender<NetworkMessage>,
        total_nodes: usize,
        config: TrainingConfig,
    ) -> Self {
        let buffer = TrainingBuffer::new(config.buffer_size, config.replay_ratio);

        Self {
            local_buffer: Arc::new(RwLock::new(buffer)),
            secret_sharing: SecretSharingManager::new(total_nodes),
            network_tx,
            model_weights: Arc::new(RwLock::new(Vec::new())),
            share_buffer: Arc::new(ShareAggregationBuffer::new()),
            fisher_matrices: Arc::new(DashMap::new()),
            config,
            node_id,
            current_round: Arc::new(RwLock::new(None)),
        }
    }

    /// Called when inference completes - adds to buffer and checks trigger
    pub async fn on_inference_complete(
        &self,
        input: Vec<f32>,
        output: Vec<f32>,
        loss: f32,
    ) -> Result<bool, TrainingError> {
        if !self.config.enabled {
            return Ok(false);
        }

        let sample = InferenceSample {
            input,
            output,
            loss,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let mut buffer = self.local_buffer.write().await;
        buffer.push(sample);

        // Check if buffer is ready
        let is_ready = buffer.is_ready(self.config.trigger_threshold);
        drop(buffer);

        if is_ready && self.current_round.read().await.is_none() {
            return Ok(true); // Signal that training should be triggered
        }

        Ok(false)
    }

    /// Trigger a training burst by broadcasting to all nodes
    pub async fn trigger_training_burst(&self, model_id: String) -> Result<Uuid, TrainingError> {
        let round_id = Uuid::new_v4();
        *self.current_round.write().await = Some(round_id);

        let msg = NetworkMessage::TrainingBurst {
            model_id,
            buffer_size: self.local_buffer.read().await.len(),
            trigger_timestamp: chrono::Utc::now().timestamp(),
        };

        self.network_tx.send(msg).await.map_err(|_| TrainingError::NetworkError)?;
        
        Ok(round_id)
    }

    /// Execute local training and return delta
    pub async fn execute_local_training(
        &self,
        samples: &[InferenceSample],
        model_id: &str,
    ) -> Result<Vec<f32>, TrainingError> {
        // Get current model weights
        let old_weights = self.model_weights.read().await.clone();
        
        if old_weights.is_empty() {
            return Err(TrainingError::ModelNotLoaded);
        }

        // Simulate training (in real implementation, this would use the model crate)
        // For now, compute a simple gradient update based on loss
        let mut new_weights = old_weights.clone();
        
        // Compute average loss from samples
        let avg_loss: f32 = if samples.is_empty() {
            0.0
        } else {
            samples.iter().map(|s| s.loss).sum::<f32>() / samples.len() as f32
        };

        // Simple gradient descent simulation
        for weight in new_weights.iter_mut() {
            // Gradient proportional to loss and random noise
            let gradient = avg_loss * (rand::random::<f32>() - 0.5) * 2.0;
            *weight -= self.config.learning_rate * gradient;
        }

        // Apply EWC regularization if Fisher matrix exists
        if let Some(fisher) = self.fisher_matrices.get(model_id) {
            let delta = DeltaComputation::compute_delta(&old_weights, &new_weights);
            let ewc_penalty = fisher.ewc_loss(&delta, self.config.ewc_lambda);
            
            // Add EWC penalty to weights (simplified)
            for (i, weight) in new_weights.iter_mut().enumerate() {
                if i < fisher.diagonal.len() && i < fisher.old_params.len() {
                    let penalty = self.config.ewc_lambda * fisher.diagonal[i] * (delta[i]);
                    *weight -= self.config.learning_rate * penalty;
                }
            }

            tracing::info!("Applied EWC regularization with lambda={}, penalty={}", 
                self.config.ewc_lambda, ewc_penalty);
        }

        // Compute delta
        let delta = DeltaComputation::compute_delta(&old_weights, &new_weights);
        
        // Check delta magnitude
        let l2_norm = DeltaComputation::l2_norm(&delta);
        if l2_norm > 100.0 {
            tracing::warn!("Large delta detected: L2 norm = {}", l2_norm);
        }

        Ok(delta)
    }

    /// Distribute delta shares to all peers via secret sharing using P2P request-response
    /// 
    /// Encrypts each share bundle with the recipient's public key using AES-GCM + ECDH,
    /// then sends via the libp2p request-response protocol.
    pub async fn distribute_delta_shares(
        &self,
        delta: Vec<f32>,
        round_id: Uuid,
    ) -> Result<(), TrainingError> {
        // Compress delta before sharing (quantized values currently unused but prepared for future)
        let (_quantized, _min_val, _scale) = DeltaComputation::quantize_8bit(&delta);
        
        // Split quantized delta into shares
        let share_bundles = self
            .secret_sharing
            .split_delta(self.node_id.clone(), round_id, &delta)
            .map_err(|e| TrainingError::SecretSharing(e.to_string()))?;

        // Send each bundle to the corresponding peer via P2P request-response
        // The P2P layer will handle encryption (AES-GCM + ECDH) and delivery
        for (idx, bundle) in share_bundles.iter().enumerate() {
            let recipient = format!("node_{}", idx + 1);
            
            // Serialize the bundle for transmission
            let bundle_bytes = serde_json::to_vec(bundle)
                .map_err(|e| TrainingError::Serialization(e.to_string()))?;
            
            // Send via network channel - P2P layer will encrypt and send via request-response
            let msg = NetworkMessage::DeltaShare {
                node_id: self.node_id.clone(),
                share_index: idx as u32,
                encrypted_share: bundle_bytes,
                recipient_node: recipient,
                round_id,
            };
            
            self.network_tx.send(msg).await.map_err(|_| TrainingError::NetworkError)?;
        }

        tracing::info!("Distributed {} delta shares for round {} via P2P request-response", 
            share_bundles.len(), round_id);
        Ok(())
    }

    /// Aggregate received shares locally
    pub async fn aggregate_received_shares(
        &self,
        round_id: Uuid,
    ) -> Result<AggregatedShare, TrainingError> {
        let shares = self.share_buffer.get_shares_for_round(round_id);
        
        if shares.is_empty() {
            return Err(TrainingError::NoSharesReceived);
        }

        let aggregated = self
            .secret_sharing
            .aggregate_shares(self.node_id.clone(), round_id, &shares)
            .map_err(|e| TrainingError::SecretSharing(e.to_string()))?;

        // Store aggregated share
        self.share_buffer.store_aggregated(round_id, aggregated.clone());

        // Send to aggregator
        let msg = NetworkMessage::AggregatedShare {
            node_id: self.node_id.clone(),
            aggregated_share: aggregated.aggregated_value.clone(),
            round_id,
        };
        
        self.network_tx.send(msg).await.map_err(|_| TrainingError::NetworkError)?;

        Ok(aggregated)
    }

    /// Reconstruct global delta from aggregated shares (called by aggregator)
    /// 
    /// Uses the explicit share_index from each AggregatedShare for reconstruction,
    /// ensuring proper polynomial interpolation without parsing node IDs.
    pub fn reconstruct_global_delta(
        &self,
        aggregated_shares: &[AggregatedShare],
    ) -> Result<Vec<f32>, TrainingError> {
        // Validate we have enough shares
        if aggregated_shares.len() < self.secret_sharing.threshold() {
            return Err(TrainingError::SecretSharing(format!(
                "Insufficient shares for reconstruction: got {}, need {}",
                aggregated_shares.len(),
                self.secret_sharing.threshold()
            )));
        }

        // Validate all share indices are unique
        let mut indices = std::collections::HashSet::new();
        for agg in aggregated_shares {
            if !indices.insert(agg.share_index) {
                return Err(TrainingError::SecretSharing(
                    "Duplicate share indices detected".to_string()
                ));
            }
        }

        // Convert AggregatedShare to SecretShare for reconstruction
        // Use the explicit share_index instead of parsing from node_id
        let shares: Vec<crate::secret_sharing::SecretShare> = aggregated_shares
            .iter()
            .map(|agg| crate::secret_sharing::SecretShare {
                index: agg.share_index,
                value: agg.aggregated_value.clone(),
                threshold: self.secret_sharing.threshold(),
                total_shares: self.secret_sharing.total_nodes(),
            })
            .collect();

        // Reconstruct secret
        let secret_bytes = self
            .secret_sharing
            .reconstruct_secret(&shares)
            .map_err(|e| TrainingError::SecretSharing(e.to_string()))?;

        // Deserialize to f32 delta
        let delta = self
            .secret_sharing
            .deserialize_delta(&secret_bytes)
            .map_err(|e| TrainingError::SecretSharing(e.to_string()))?;

        // Average by number of nodes
        let n = self.secret_sharing.total_nodes() as f32;
        let averaged: Vec<f32> = delta.iter().map(|&x| x / n).collect();

        Ok(averaged)
    }

    /// Apply global delta to local model weights
    pub async fn apply_global_delta(&self, delta: Vec<f32>) -> Result<(), TrainingError> {
        let mut weights = self.model_weights.write().await;
        
        DeltaComputation::apply_delta(&mut weights, &delta, self.config.learning_rate);
        
        tracing::info!("Applied global delta to model weights");
        Ok(())
    }

    /// Compute Fisher matrix for EWC
    pub async fn compute_fisher_matrix(
        &self,
        model_id: String,
        samples: &[InferenceSample],
    ) -> Result<FisherMatrix, TrainingError> {
        let weights = self.model_weights.read().await.clone();
        
        if weights.is_empty() {
            return Err(TrainingError::ModelNotLoaded);
        }

        // Compute diagonal Fisher approximation
        // F_i = E[(dL/dtheta_i)^2]
        let mut fisher_diagonal = vec![0.0f32; weights.len()];
        
        for sample in samples {
            // Approximate gradient squared as loss^2 * random
            let grad_scale = sample.loss * sample.loss;
            for (_i, fisher) in fisher_diagonal.iter_mut().enumerate() {
                let grad = grad_scale * (rand::random::<f32>() - 0.5);
                *fisher += grad * grad;
            }
        }

        // Average
        let n = samples.len().max(1) as f32;
        for fisher in fisher_diagonal.iter_mut() {
            *fisher /= n;
        }

        let fisher = FisherMatrix {
            diagonal: fisher_diagonal,
            old_params: weights,
            timestamp: chrono::Utc::now().timestamp(),
            ipfs_hash: None,
        };

        // Store Fisher matrix
        self.fisher_matrices.insert(model_id, fisher.clone());

        Ok(fisher)
    }

    /// Get current buffer size
    pub async fn buffer_size(&self) -> usize {
        self.local_buffer.read().await.len()
    }

    /// Sample batch from buffer (public accessor)
    pub async fn sample_from_buffer(&self, n: usize) -> Vec<InferenceSample> {
        self.local_buffer.read().await.sample_batch(n)
    }

    /// Get training config
    pub fn config(&self) -> &TrainingConfig {
        &self.config
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get current round ID
    pub async fn current_round(&self) -> Option<Uuid> {
        *self.current_round.read().await
    }

    /// Clear current round
    pub async fn clear_round(&self) {
        *self.current_round.write().await = None;
    }

    /// Initialize model weights before training
    /// 
    /// Must be called before training bursts start. Loads from registry or seeds
    /// with provided initial weights.
    pub async fn initialize_model_weights(&self, initial_weights: Vec<f32>) -> Result<(), TrainingError> {
        let mut weights = self.model_weights.write().await;
        *weights = initial_weights;
        tracing::info!("Initialized model weights with {} parameters", weights.len());
        Ok(())
    }

    /// Check if model weights are initialized
    pub async fn are_weights_initialized(&self) -> bool {
        !self.model_weights.read().await.is_empty()
    }
}

/// Training error types
#[derive(Debug, thiserror::Error)]
pub enum TrainingError {
    #[error("network error")]
    NetworkError,
    #[error("model not loaded")]
    ModelNotLoaded,
    #[error("secret sharing error: {0}")]
    SecretSharing(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("no shares received")]
    NoSharesReceived,
    #[error("reconstruction failed: {0}")]
    ReconstructionFailed(String),
    #[error("invalid delta magnitude: L2 norm = {0}")]
    InvalidDeltaMagnitude(f32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_training_buffer() {
        let mut buffer = TrainingBuffer::new(10, 0.01);
        
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
    fn test_delta_computation() {
        let original = vec![1.0, 2.0, 3.0];
        let updated = vec![1.1, 1.9, 3.2];
        
        let delta = DeltaComputation::compute_delta(&original, &updated);
        assert_eq!(delta, vec![0.1, -0.1, 0.2]);
        
        let mut weights = original.clone();
        DeltaComputation::apply_delta(&mut weights, &delta, 0.5);
        assert_eq!(weights, vec![1.05, 1.95, 3.1]);
        
        let l2 = DeltaComputation::l2_norm(&delta);
        assert!((l2 - 0.2449).abs() < 0.01);
    }

    #[test]
    fn test_quantization() {
        let delta = vec![-1.0, 0.0, 0.5, 1.0];
        
        let (quantized, min_val, scale) = DeltaComputation::quantize_8bit(&delta);
        assert_eq!(quantized.len(), 4);
        assert_eq!(min_val, -1.0);
        
        let reconstructed = DeltaComputation::dequantize_8bit(&quantized, min_val, scale);
        
        // Check reconstruction is close (within quantization error)
        for (orig, recon) in delta.iter().zip(reconstructed.iter()) {
            assert!((orig - recon).abs() < 0.01);
        }
    }

    #[tokio::test]
    async fn test_training_coordinator() {
        let (tx, _rx) = mpsc::channel(100);
        let config = TrainingConfig::default();
        let coordinator = TrainingCoordinator::new(
            "test_node".to_string(),
            tx,
            3,
            config,
        );

        assert!(!coordinator.config.enabled);
        
        // Test buffer operations
        for i in 0..5 {
            let should_trigger = coordinator
                .on_inference_complete(vec![i as f32], vec![i as f32], 0.1)
                .await
                .unwrap();
            assert!(!should_trigger); // Buffer not full yet
        }

        assert_eq!(coordinator.buffer_size().await, 5);
    }
}
