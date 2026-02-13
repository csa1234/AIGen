// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use uuid::Uuid;

use crate::block_assignment::{BlockAssignment, StaticBlockConfig};
use crate::pipeline_message::{ActivationChunk, InferenceStart, PipelineMessage};
use crate::tensor_transport::{CompressedTensor, TensorMetadata, TensorTransport};

/// Tracks active inference requests in the pipeline
#[derive(Clone, Debug)]
pub struct ActiveInference {
    pub inference_id: Uuid,
    pub model_id: String,
    pub prompt: String,
    pub current_block: u32,
    pub pipeline_route: Vec<u32>,
    pub start_time: Instant,
    pub token_pipeline: Vec<TokenState>, // NEW: Track token generation pipeline state
}

/// Token state for computation/communication overlap
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenState {
    Pending,
    InFlight { block_id: u32 },
    Completed,
}

/// Batch configuration for dynamic batching
#[derive(Clone, Debug)]
pub struct BatchConfig {
    pub max_batch_size: usize,
    pub max_wait_ms: u64,
    pub enable_batching: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 8,
            max_wait_ms: 100, // 100ms max wait time
            enable_batching: true,
        }
    }
}

/// Inference request for batching queue
#[derive(Clone, Debug)]
pub struct InferenceRequest {
    pub model_id: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub user_id: String,
    pub timestamp: Instant,
    pub inference_id: Uuid, // Pre-generated ID for tracking
}

/// Batching metrics for performance tracking
#[derive(Debug)]
pub struct BatchingMetrics {
    pub batches_created: AtomicU64,
    pub batches_dispatched: AtomicU64,
    pub batches_completed: AtomicU64,
    pub batches_timed_out: AtomicU64,
    pub avg_batch_size: AtomicU32,
    pub batch_wait_time_ms: AtomicU64,
}

impl Default for BatchingMetrics {
    fn default() -> Self {
        Self {
            batches_created: AtomicU64::new(0),
            batches_dispatched: AtomicU64::new(0),
            batches_completed: AtomicU64::new(0),
            batches_timed_out: AtomicU64::new(0),
            avg_batch_size: AtomicU32::new(0),
            batch_wait_time_ms: AtomicU64::new(0),
        }
    }
}

impl BatchingMetrics {
    pub fn record_batch(&self, batch_size: usize, wait_time_ms: u64) {
        self.batches_created.fetch_add(1, Ordering::Relaxed);
        
        // Update average batch size using exponential moving average
        let size_fixed = (batch_size as u32 * 100) as u32; // Fixed-point
        let prev = self.avg_batch_size.load(Ordering::Relaxed);
        let new = if prev == 0 {
            size_fixed
        } else {
            (prev + size_fixed) / 2
        };
        self.avg_batch_size.store(new, Ordering::Relaxed);
        
        // Update average wait time
        let prev_wait = self.batch_wait_time_ms.load(Ordering::Relaxed);
        let new_wait = if prev_wait == 0 {
            wait_time_ms
        } else {
            (prev_wait + wait_time_ms) / 2
        };
        self.batch_wait_time_ms.store(new_wait, Ordering::Relaxed);
    }
    
    pub fn record_batch_dispatched(&self) {
        self.batches_dispatched.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_batch_completed(&self) {
        self.batches_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_batch_timeout(&self) {
        self.batches_timed_out.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> (u64, u64, u64, u64, f32, u64) {
        let created = self.batches_created.load(Ordering::Relaxed);
        let dispatched = self.batches_dispatched.load(Ordering::Relaxed);
        let completed = self.batches_completed.load(Ordering::Relaxed);
        let timed_out = self.batches_timed_out.load(Ordering::Relaxed);
        let avg_size = self.avg_batch_size.load(Ordering::Relaxed) as f32 / 100.0;
        let avg_wait = self.batch_wait_time_ms.load(Ordering::Relaxed);
        (created, dispatched, completed, timed_out, avg_size, avg_wait)
    }
}

/// Batched inference result
#[derive(Clone, Debug)]
pub struct BatchedInference {
    pub batch_id: Uuid,
    pub inference_ids: Vec<Uuid>,
    pub start_message: InferenceStart,
}

/// Coordinates static pipeline execution
pub struct StaticPipelineCoordinator {
    assignment: Arc<RwLock<BlockAssignment>>,
    active_inferences: Arc<RwLock<HashMap<Uuid, ActiveInference>>>,
    my_block_id: Option<u32>,
    is_coordinator: bool,
    // NEW: Batching fields
    batch_config: BatchConfig,
    pending_requests: Arc<RwLock<Vec<InferenceRequest>>>,
    batching_metrics: Arc<BatchingMetrics>,
    batch_sender: Option<mpsc::Sender<BatchedInference>>,
    batch_receiver: Option<mpsc::Receiver<BatchedInference>>,
}

impl StaticPipelineCoordinator {
    /// Create coordinator from static config
    pub fn new(
        config: StaticBlockConfig,
        my_block_id: Option<u32>,
        is_coordinator: bool,
    ) -> Self {
        let assignment = config.to_block_assignment();

        Self {
            assignment: Arc::new(RwLock::new(assignment)),
            active_inferences: Arc::new(RwLock::new(HashMap::new())),
            my_block_id,
            is_coordinator,
            // NEW: Initialize batching fields
            batch_config: BatchConfig::default(),
            pending_requests: Arc::new(RwLock::new(Vec::new())),
            batching_metrics: Arc::new(BatchingMetrics::default()),
            batch_sender: None,
            batch_receiver: None,
        }
    }

    /// Create coordinator with custom batch config
    pub fn with_batch_config(
        config: StaticBlockConfig,
        my_block_id: Option<u32>,
        is_coordinator: bool,
        batch_config: BatchConfig,
    ) -> Self {
        let assignment = config.to_block_assignment();
        
        // Create channel for batched inferences if batching is enabled
        let (sender, receiver) = if batch_config.enable_batching && is_coordinator {
            let (tx, rx) = mpsc::channel(100);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        Self {
            assignment: Arc::new(RwLock::new(assignment)),
            active_inferences: Arc::new(RwLock::new(HashMap::new())),
            my_block_id,
            is_coordinator,
            batch_config,
            pending_requests: Arc::new(RwLock::new(Vec::new())),
            batching_metrics: Arc::new(BatchingMetrics::default()),
            batch_sender: sender,
            batch_receiver: receiver,
        }
    }

    /// Start a new inference (coordinator only)
    pub async fn start_inference(
        &self,
        model_id: String,
        prompt: String,
        max_tokens: u32,
    ) -> Result<Uuid> {
        if !self.is_coordinator {
            return Err(anyhow!("Only coordinator can start inference"));
        }

        let assignment = self.assignment.read().await;
        let pipeline_route: Vec<u32> = assignment.get_block_ids();

        if pipeline_route.is_empty() {
            return Err(anyhow!("No blocks configured in pipeline"));
        }

        let msg = InferenceStart::new(model_id.clone(), prompt.clone(), max_tokens, pipeline_route.clone());
        let inference_id = msg.inference_id;

        // Track active inference
        let active = ActiveInference {
            inference_id,
            model_id,
            prompt,
            current_block: pipeline_route[0],
            pipeline_route: pipeline_route.clone(),
            start_time: Instant::now(),
            token_pipeline: vec![TokenState::Pending; pipeline_route.len()],
        };

        self.active_inferences.write().await.insert(inference_id, active);

        Ok(inference_id)
    }

    /// Get next block in pipeline
    pub async fn get_next_block(&self, inference_id: Uuid, current_block: u32) -> Option<u32> {
        let inferences = self.active_inferences.read().await;
        let active = inferences.get(&inference_id)?;

        let current_idx = active.pipeline_route.iter().position(|&b| b == current_block)?;
        active.pipeline_route.get(current_idx + 1).copied()
    }

    /// Check if this is the final block
    pub async fn is_final_block(&self, inference_id: Uuid, block_id: u32) -> bool {
        let inferences = self.active_inferences.read().await;
        if let Some(active) = inferences.get(&inference_id) {
            active.pipeline_route.last() == Some(&block_id)
        } else {
            false
        }
    }

    /// Complete inference and remove from tracking
    pub async fn complete_inference(&self, inference_id: Uuid) -> Option<ActiveInference> {
        self.active_inferences.write().await.remove(&inference_id)
    }

    /// Get my block ID
    pub fn my_block_id(&self) -> Option<u32> {
        self.my_block_id
    }

    /// Check if I should process this block
    pub fn should_process_block(&self, block_id: u32) -> bool {
        self.my_block_id == Some(block_id)
    }

    // =========================================================================
    // NEW: Dynamic Batching Methods
    // =========================================================================

    /// Enqueue an inference request for batching
    pub async fn enqueue_inference_request(&self, request: InferenceRequest) -> Result<()> {
        if !self.is_coordinator {
            return Err(anyhow!("Only coordinator can enqueue inference requests"));
        }

        if !self.batch_config.enable_batching {
            // If batching is disabled, process immediately
            return Err(anyhow!("Batching is disabled, use start_inference directly"));
        }

        let mut pending = self.pending_requests.write().await;
        pending.push(request);

        // If we've reached max batch size, trigger batch processing
        if pending.len() >= self.batch_config.max_batch_size {
            drop(pending); // Release lock before processing
            self.flush_pending_batches().await?;
        }

        Ok(())
    }

    /// Batch pending inference requests and send via channel
    pub async fn batch_inference_requests(&self) -> Result<()> {
        let mut pending = self.pending_requests.write().await;
        
        if pending.is_empty() {
            return Ok(());
        }

        // Calculate batch size (up to max_batch_size)
        let batch_size = pending.len().min(self.batch_config.max_batch_size);
        let batch_start_time = pending[0].timestamp;
        let wait_time_ms = batch_start_time.elapsed().as_millis() as u64;

        // Take requests up to batch_size
        let batch_requests: Vec<InferenceRequest> = pending.drain(0..batch_size).collect();
        drop(pending); // Release lock

        if batch_requests.is_empty() {
            return Ok(());
        }

        // Group by model_id (for simplicity, we process one model per batch)
        let model_id = batch_requests[0].model_id.clone();
        let inference_ids: Vec<Uuid> = batch_requests.iter().map(|r| r.inference_id).collect();

        // Combine prompts for batched processing
        let combined_prompt = batch_requests
            .iter()
            .map(|r| r.prompt.clone())
            .collect::<Vec<_>>()
            .join("\n[BATCH_SEP]\n");

        // Use max_tokens from first request
        let max_tokens = batch_requests[0].max_tokens;

        // Get pipeline route
        let assignment = self.assignment.read().await;
        let pipeline_route: Vec<u32> = assignment.get_block_ids();
        drop(assignment);

        // Create batched inference start message
        let batch_id = Uuid::new_v4();
        let start_message = InferenceStart::new(
            model_id.clone(),
            combined_prompt,
            max_tokens,
            pipeline_route.clone(),
        );

        // Record metrics
        self.batching_metrics.record_batch(batch_size, wait_time_ms);

        // Track each inference in the batch
        let mut active_inferences = self.active_inferences.write().await;
        for request in &batch_requests {
            let active = ActiveInference {
                inference_id: request.inference_id,
                model_id: request.model_id.clone(),
                prompt: request.prompt.clone(),
                current_block: pipeline_route[0],
                pipeline_route: pipeline_route.clone(),
                start_time: request.timestamp,
                token_pipeline: vec![TokenState::Pending; pipeline_route.len()],
            };
            active_inferences.insert(request.inference_id, active);
        }
        drop(active_inferences);

        // Create batch and send via channel
        let batch = BatchedInference {
            batch_id,
            inference_ids,
            start_message,
        };

        // Send batch via channel if sender exists
        if let Some(sender) = &self.batch_sender {
            sender.send(batch).await.map_err(|e| {
                anyhow!("Failed to send batch to channel: {}", e)
            })?;
        }

        Ok(())
    }

    /// Process batch result and split back to individual requests
    pub async fn process_batch_result(
        &self,
        batch_id: Uuid,
        inference_ids: Vec<Uuid>,
        outputs: Vec<String>,
    ) -> Result<HashMap<Uuid, String>> {
        if inference_ids.len() != outputs.len() {
            return Err(anyhow!(
                "Mismatched batch result: {} inference_ids but {} outputs",
                inference_ids.len(),
                outputs.len()
            ));
        }

        // Map inference_ids to outputs
        let mut results = HashMap::new();
        for (id, output) in inference_ids.into_iter().zip(outputs.into_iter()) {
            results.insert(id, output);
            
            // Complete the inference
            self.complete_inference(id).await;
        }

        Ok(results)
    }

    /// Flush pending batches immediately via channel
    pub async fn flush_pending_batches(&self) -> Result<()> {
        // Process batches until no more pending requests
        loop {
            let pending_count = self.pending_requests.read().await.len();
            if pending_count == 0 {
                break;
            }
            self.batch_inference_requests().await?;
        }
        Ok(())
    }

    /// Start the background batch processor loop
    pub fn start_batch_processor(self: Arc<Self>) {
        if !self.batch_config.enable_batching || !self.is_coordinator {
            return;
        }

        let interval_duration = Duration::from_millis(self.batch_config.max_wait_ms / 2);
        let pending_requests = self.pending_requests.clone();
        let batch_config = self.batch_config.clone();
        let batching_metrics = self.batching_metrics.clone();
        let coordinator = self.clone();

        tokio::spawn(async move {
            let mut interval = interval(interval_duration);
            
            loop {
                interval.tick().await;
                
                // Check if there are pending requests that have exceeded max_wait_ms
                let should_flush = {
                    let pending = pending_requests.read().await;
                    if pending.is_empty() {
                        false
                    } else {
                        let first_request_time = pending[0].timestamp;
                        first_request_time.elapsed().as_millis() as u64 >= batch_config.max_wait_ms
                    }
                };

                if should_flush {
                    // Also flush if we have any pending requests
                    let pending_count = pending_requests.read().await.len();
                    if pending_count > 0 {
                        if let Err(e) = coordinator.flush_pending_batches().await {
                            eprintln!("Batch flush error: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Take ownership of batch_receiver for dispatcher loop
    pub fn take_batch_receiver(&mut self) -> Option<mpsc::Receiver<BatchedInference>> {
        self.batch_receiver.take()
    }

    /// Get batch sender (for external use)
    pub fn get_batch_sender(&self) -> Option<mpsc::Sender<BatchedInference>> {
        self.batch_sender.clone()
    }
    pub fn get_batching_metrics(&self) -> Arc<BatchingMetrics> {
        self.batching_metrics.clone()
    }

    /// Get batch configuration
    pub fn get_batch_config(&self) -> &BatchConfig {
        &self.batch_config
    }

    /// Update batch configuration
    pub fn set_batch_config(&mut self, config: BatchConfig) {
        self.batch_config = config;
    }

    /// Get the timestamp of the first pending request (for timeout checking)
    pub async fn get_first_pending_time(&self) -> Option<Instant> {
        let pending = self.pending_requests.read().await;
        pending.first().map(|r| r.timestamp)
    }

    /// Get pending request count
    pub async fn get_pending_request_count(&self) -> usize {
        self.pending_requests.read().await.len()
    }

    // =========================================================================
    // NEW: Computation/Communication Overlap Methods
    // =========================================================================

    /// Update token state for a specific block
    pub async fn update_token_state(
        &self,
        inference_id: Uuid,
        token_idx: usize,
        state: TokenState,
    ) -> Result<()> {
        let mut inferences = self.active_inferences.write().await;
        
        if let Some(active) = inferences.get_mut(&inference_id) {
            if token_idx < active.token_pipeline.len() {
                active.token_pipeline[token_idx] = state;
                Ok(())
            } else {
                Err(anyhow!("Token index out of bounds"))
            }
        } else {
            Err(anyhow!("Inference not found"))
        }
    }

    /// Get current token pipeline depth (number of in-flight tokens)
    pub async fn get_pipeline_depth(&self, inference_id: Uuid) -> usize {
        let inferences = self.active_inferences.read().await;
        
        if let Some(active) = inferences.get(&inference_id) {
            active.token_pipeline.iter()
                .filter(|&&s| matches!(s, TokenState::InFlight { .. }))
                .count()
        } else {
            0
        }
    }

    /// Check if we should apply backpressure (downstream is slow)
    pub async fn should_apply_backpressure(&self, inference_id: Uuid, max_depth: usize) -> bool {
        self.get_pipeline_depth(inference_id).await >= max_depth
    }
}
