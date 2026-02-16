// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! ONNX-based inference engine with shard loading, caching, and metrics.
//!
//! Note: This module uses the `ort` crate instead of `onnxruntime` because `ort` provides
//! maintained APIs and `load-dynamic` support needed for Windows builds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use dashmap::DashMap;
use genesis::check_shutdown;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionOutputs};
use ort::value::Tensor;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::ads::AdManager;
use crate::registry::{ModelError, ModelShard, WeightFragment};
use crate::sharding::{combine_fragments, verify_shard_integrity, ShardError};
use crate::storage::{StorageBackend, StorageError};
use crate::ModelRegistry;

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Get or initialize the ONNX environment
pub fn get_onnx_environment() -> Result<Arc<ort::environment::Environment>, InferenceError> {
    Err(InferenceError::ModelLoadFailed(
        "ONNX environment initialization not yet implemented".to_string()
    ))
}

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("model not found")]
    ModelNotFound,
    #[error("model load failed: {0}")]
    ModelLoadFailed(String),
    #[error("inference failed: {0}")]
    InferenceFailed(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid output: {0}")]
    InvalidOutput(String),
    #[error("out of memory")]
    OutOfMemory,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("shard error: {0}")]
    ShardError(ShardError),
    #[error("storage error: {0}")]
    StorageError(StorageError),
    #[error("shutdown active")]
    Shutdown,
}

fn default_ort_dylib_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libonnxruntime.dylib"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "libonnxruntime.so"
    }
}

fn init_onnx_runtime() -> Result<(), InferenceError> {
    let init = ORT_INIT.get_or_init(|| {
        let dylib_path = match std::env::var("ORT_DYLIB_PATH") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => PathBuf::from(default_ort_dylib_name()),
        };
        let builder = ort::init_from(&dylib_path).map_err(|err| {
            format!(
                "failed to initialize ONNX Runtime from {}: {err}",
                dylib_path.display()
            )
        })?;
        builder.commit();
        Ok(())
    });

    match init {
        Ok(()) => Ok(()),
        Err(err) => Err(InferenceError::ModelLoadFailed(err.clone())),
    }
}

impl From<ShardError> for InferenceError {
    fn from(err: ShardError) -> Self {
        InferenceError::ShardError(err)
    }
}

impl From<StorageError> for InferenceError {
    fn from(err: StorageError) -> Self {
        InferenceError::StorageError(err)
    }
}

fn ensure_running() -> Result<(), InferenceError> {
    check_shutdown().map_err(|_| InferenceError::Shutdown)
}

fn map_model_error(err: ModelError) -> InferenceError {
    match err {
        ModelError::ModelNotFound => InferenceError::ModelNotFound,
        ModelError::Shutdown => InferenceError::Shutdown,
        other => InferenceError::ModelLoadFailed(other.to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceTensor {
    pub name: String,
    pub shape: Vec<i64>,
    pub data: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOutput {
    pub name: String,
    pub shape: Vec<i64>,
    pub data: Vec<f32>,
}

#[derive(Debug)]
pub struct LoadedModel {
    pub model_id: String,
    pub session: Mutex<Session>,
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
    pub memory_size: usize,
    pub last_used: Arc<RwLock<Instant>>,
    pub is_core: Arc<RwLock<bool>>,
    pub model_path: PathBuf,
    pub shard_dir: PathBuf,
}

impl LoadedModel {
    pub fn touch(&self) {
        *self.last_used.write() = Instant::now();
    }

    pub fn run_inference(
        &self,
        inputs: Vec<InferenceTensor>,
    ) -> Result<Vec<InferenceOutput>, InferenceError> {
        ensure_running()?;
        self.touch();

        let inputs_ordered = self.order_inputs(inputs)?;
        let ort_inputs = inputs_ordered
            .iter()
            .map(|tensor| self.tensor_to_value(tensor))
            .collect::<Result<Vec<(String, Tensor<f32>)>, InferenceError>>()?;

        let mut session = self.session.lock();
        let outputs = session
            .run(ort_inputs)
            .map_err(|err: ort::Error| InferenceError::InferenceFailed(err.to_string()))?;

        self.parse_outputs(outputs)
    }

    fn order_inputs(
        &self,
        inputs: Vec<InferenceTensor>,
    ) -> Result<Vec<InferenceTensor>, InferenceError> {
        if self.input_names.is_empty() {
            return Ok(inputs);
        }

        let mut by_name: HashMap<String, InferenceTensor> = inputs
            .into_iter()
            .map(|tensor| (tensor.name.clone(), tensor))
            .collect();
        let mut ordered = Vec::with_capacity(self.input_names.len());
        for name in &self.input_names {
            let tensor = by_name
                .remove(name)
                .ok_or_else(|| InferenceError::InvalidInput(format!("missing input {name}")))?;
            ordered.push(tensor);
        }

        if !by_name.is_empty() {
            let extras: Vec<String> = by_name.keys().cloned().collect();
            return Err(InferenceError::InvalidInput(format!(
                "unexpected inputs: {}",
                extras.join(", ")
            )));
        }

        Ok(ordered)
    }

    fn tensor_to_value(
        &self,
        tensor: &InferenceTensor,
    ) -> Result<(String, Tensor<f32>), InferenceError> {
        let value = Tensor::from_array((tensor.shape.clone(), tensor.data.clone()))
            .map_err(|err: ort::Error| InferenceError::InvalidInput(err.to_string()))?;
        Ok((tensor.name.clone(), value))
    }

    fn parse_outputs(
        &self,
        outputs: SessionOutputs<'_>,
    ) -> Result<Vec<InferenceOutput>, InferenceError> {
        let mut results = Vec::with_capacity(outputs.len());
        for (index, (name, output)) in outputs.into_iter().enumerate() {
            let (shape, data) = output
                .try_extract_tensor::<f32>()
                .map_err(|err: ort::Error| InferenceError::InvalidOutput(err.to_string()))?;
            let shape: Vec<i64> = shape.iter().copied().collect();
            let data = data.to_vec();
            let output_name = if name.is_empty() {
                self.output_names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("output_{index}"))
            } else {
                name.to_string()
            };
            results.push(InferenceOutput {
                name: output_name,
                shape,
                data,
            });
        }
        Ok(results)
    }
}

/// Errors specific to fragment execution
#[derive(Debug, Error)]
pub enum FragmentExecutorError {
    #[error("fragment not found: {0}")]
    FragmentNotFound(String),
    #[error("fragment load failed: {0}")]
    FragmentLoadFailed(String),
    #[error("fragment integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    #[error("decompression failed: {0}")]
    DecompressionFailed(String),
    #[error("inference error: {0}")]
    InferenceError(#[from] InferenceError),
    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("shard error: {0}")]
    ShardError(#[from] ShardError),
    #[error("invalid layer range: {0}")]
    InvalidLayerRange(String),
    #[error("shutdown active")]
    Shutdown,
}

/// Activation tensor for fragment input/output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentActivation {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub layer_range: (u32, u32),
    pub shape: Vec<i64>,
    pub data: Vec<f32>,
    pub checkpoint_hash: [u8; 32],
    pub timestamp: i64,
}

impl FragmentActivation {
    /// Create a new activation with automatic checkpoint hash
    pub fn new(
        task_id: Uuid,
        inference_id: Uuid,
        layer_range: (u32, u32),
        shape: Vec<i64>,
        data: Vec<f32>,
    ) -> Self {
        let checkpoint_hash = Self::compute_hash(&data);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Self {
            task_id,
            inference_id,
            layer_range,
            shape,
            data,
            checkpoint_hash,
            timestamp,
        }
    }

    /// Compute SHA3-256 hash of activation data
    pub fn compute_hash(data: &[f32]) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        let bytes: &[u8] = bytemuck::cast_slice(data);
        hasher.update(bytes);
        hasher.finalize().into()
    }

    /// Verify integrity
    pub fn verify_integrity(&self) -> bool {
        Self::compute_hash(&self.data) == self.checkpoint_hash
    }

    /// Get total number of elements
    pub fn num_elements(&self) -> usize {
        self.data.len()
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> usize {
        self.data.len() * std::mem::size_of::<f32>()
    }
}

/// FragmentExecutor for loading and executing partial model fragments
pub struct FragmentExecutor {
    pub model_id: String,
    pub fragment_ids: Vec<String>,
    pub layer_range: (u32, u32),
    pub session: Mutex<Session>,
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
    pub memory_size: usize,
    pub vram_allocated_mb: u32,
    pub last_used: Arc<RwLock<Instant>>,
    pub fragment_dir: PathBuf,
}

impl FragmentExecutor {
    pub fn touch(&self) {
        *self.last_used.write() = Instant::now();
    }

    /// Execute forward pass on assigned layer range
    pub fn forward(
        &self,
        input: &FragmentActivation,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        self.touch();

        // Verify input integrity
        if !input.verify_integrity() {
            return Err(FragmentExecutorError::IntegrityCheckFailed(
                "input activation integrity check failed".to_string()
            ));
        }

        // Convert input to ONNX tensor
        let ort_input = self.activation_to_tensor(input)?;

        // Run inference
        let mut session = self.session.lock();
        let outputs = session
            .run(vec![ort_input])
            .map_err(|err| FragmentExecutorError::InferenceError(
                InferenceError::InferenceFailed(err.to_string())
            ))?;

        // Convert outputs back to activation
        let output_activation = self.outputs_to_activation(outputs, input.task_id, input.inference_id)?;

        Ok(output_activation)
    }

    /// Generate a DistributedTaskProof for the fragment execution
    pub fn generate_proof(
        &self,
        task_id: Uuid,
        inference_id: Uuid,
        input_activation: &FragmentActivation,
        output_activation: &FragmentActivation,
        execution_time_ms: u64,
        node_id: &str,
        checkpoint_hash: Option<[u8; 32]>,
        poi_nonce: u64,
    ) -> Result<poi_types::PoIProof, FragmentExecutorError> {
        use poi_types::{WorkType, ComputationMetadata, CompressionMethod, DistributedTaskProof};
        use poi_types::PoIProof;
        use sha3::{Digest, Sha3_256};

        // Compute output_activation_hash with nonce binding: hash(raw_output || nonce)
        let mut hasher = Sha3_256::new();
        let bytes: &[u8] = bytemuck::cast_slice(&output_activation.data);
        hasher.update(bytes);
        hasher.update(&poi_nonce.to_le_bytes());
        let output_activation_hash: [u8; 32] = hasher.finalize().into();

        // Create distributed task proof data with nonce
        let distributed_proof = DistributedTaskProof::new(
            task_id,
            inference_id,
            self.fragment_ids.clone(),
            self.layer_range,
            input_activation.checkpoint_hash,
            output_activation_hash,
            checkpoint_hash,
            execution_time_ms,
            node_id.to_string(),
            poi_nonce,
        );

        // Serialize proof data for verification_data
        let verification_data = serde_json::json!({
            "distributed_task_proof": distributed_proof
        });

        // Create computation metadata
        let computation_metadata = ComputationMetadata {
            rows: 0,
            cols: 0,
            inner: 0,
            iterations: 0,
            model_id: self.model_id.clone(),
            compression_method: CompressionMethod::None,
            original_size: self.memory_size,
        };

        // Create timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Create PoIProof with the provided nonce
        let output_bytes: Vec<u8> = bytemuck::cast_slice(&output_activation.data).to_vec();
        let work_hash = blockchain_core::hash_data(&output_bytes);
        let proof = PoIProof::new(
            work_hash,
            node_id.to_string(),
            timestamp,
            verification_data,
            WorkType::DistributedInference,
            input_activation.checkpoint_hash, // input_hash
            output_bytes, // output_data
            computation_metadata,
            1000, // difficulty
            poi_nonce, // Use the PoI nonce for binding
        );

        Ok(proof)
    }

    fn activation_to_tensor(
        &self,
        activation: &FragmentActivation,
    ) -> Result<(String, Tensor<f32>), FragmentExecutorError> {
        let name = self.input_names.first()
            .cloned()
            .unwrap_or_else(|| "input".to_string());
        
        let value = Tensor::from_array((activation.shape.clone(), activation.data.clone()))
            .map_err(|err| FragmentExecutorError::InferenceError(
                InferenceError::InvalidInput(err.to_string())
            ))?;
        
        Ok((name, value))
    }

    fn outputs_to_activation(
        &self,
        outputs: SessionOutputs<'_>,
        task_id: Uuid,
        inference_id: Uuid,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        // Get first output
        let (_, output) = outputs.into_iter()
            .next()
            .ok_or_else(|| FragmentExecutorError::InferenceError(
                InferenceError::InvalidOutput("no outputs from session".to_string())
            ))?;

        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|err| FragmentExecutorError::InferenceError(
                InferenceError::InvalidOutput(err.to_string())
            ))?;

        let shape: Vec<i64> = shape.iter().copied().collect();
        let data = data.to_vec();

        Ok(FragmentActivation::new(
            task_id,
            inference_id,
            self.layer_range,
            shape,
            data,
        ))
    }
}

/// Configuration for a layer block
#[derive(Clone, Debug)]
pub struct LayerBlockConfig {
    pub block_id: u32,
    pub layer_range: (u32, u32),
    pub model_id: String,
}

/// Splits ONNX models into layer blocks
pub struct ModelBlockSplitter;

impl ModelBlockSplitter {
    /// Split model into N blocks with approximately equal layer distribution
    /// 
    /// # Arguments
    /// * `model_path` - Path to the ONNX model file
    /// * `total_blocks` - Total number of blocks to split the model into
    /// * `block_size` - Optional number of layers per block (if None, computed from total_layers / total_blocks)
    pub fn split_into_blocks(
        model_path: &Path,
        total_blocks: u32,
        block_size: Option<u32>,
    ) -> Result<Vec<LayerBlockConfig>, InferenceError> {
        // Commented out due to distributed_inference dependency removal
        // TODO: Refactor to avoid circular dependency
        /*
        let env = get_onnx_environment()?;
        let session = env
            .new_session_builder()?
            .with_model_from_file(model_path)
            .map_err(|e| InferenceError::ModelLoadFailed(e.to_string()))?;
        let total_layers = Self::estimate_layer_count(&session)?;
        */
        let total_layers = 12u32;

        // Validate total_blocks against actual layer count
        if total_blocks > total_layers {
            return Err(InferenceError::InvalidInput(
                format!("Cannot split {} layers into {} blocks", total_layers, total_blocks)
            ));
        }

        // Use provided block_size or compute from total_layers and total_blocks
        let layers_per_block = block_size.unwrap_or_else(|| {
            (total_layers + total_blocks - 1) / total_blocks
        });

        // Validate block_size against layer count
        if layers_per_block == 0 {
            return Err(InferenceError::InvalidInput(
                "block_size must be greater than 0".to_string()
            ));
        }

        // Compute actual number of blocks needed based on block_size
        let computed_blocks = ((total_layers + layers_per_block - 1) / layers_per_block).min(total_blocks);

        let mut blocks = Vec::new();
        for block_id in 0..computed_blocks {
            let start_layer = block_id * layers_per_block;
            let end_layer = ((block_id + 1) * layers_per_block).min(total_layers).saturating_sub(1);

            // Skip if we've exceeded total layers
            if start_layer >= total_layers {
                break;
            }

            blocks.push(LayerBlockConfig {
                block_id,
                layer_range: (start_layer, end_layer),
                model_id: model_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            });
        }

        Ok(blocks)
    }

    /// Extract a specific block's subgraph from the model
    pub fn extract_block(
        model_path: &Path,
        _block_config: &LayerBlockConfig,
        output_path: &Path,
    ) -> Result<(), InferenceError> {
        // For Phase 2: Use full model with layer masking
        // Phase 3+ can implement true subgraph extraction
        std::fs::copy(model_path, output_path)
            .map_err(|e| InferenceError::ModelLoadFailed(e.to_string()))?;
        Ok(())
    }

    /// Estimate layer count from ONNX graph structure
    #[allow(dead_code)]
    fn estimate_layer_count(_session: &Session) -> Result<u32, InferenceError> {
        Ok(12)
    }
}

/// Separator for batched prompts
pub const BATCH_SEP: &str = "\n[BATCH_SEP]\n";

/// Check if prompt contains batch separator
pub fn is_batched_prompt(prompt: &str) -> bool {
    prompt.contains(BATCH_SEP)
}

/// Split batched prompt into individual prompts
pub fn split_batched_prompt(prompt: &str) -> Vec<String> {
    prompt.split(BATCH_SEP).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Join individual outputs into batched output
pub fn join_batched_outputs(outputs: Vec<String>) -> String {
    outputs.join(BATCH_SEP)
}

/// Executor for a specific layer block in distributed pipeline
pub struct BlockExecutor {
    pub block_id: u32,
    pub layer_range: (u32, u32),
    pub model_id: String,
    fragment_executor: Arc<FragmentExecutor>,
}

impl BlockExecutor {
    /// Create from a loaded FragmentExecutor
    pub fn new(
        block_id: u32,
        layer_range: (u32, u32),
        model_id: String,
        fragment_executor: Arc<FragmentExecutor>,
    ) -> Self {
        Self {
            block_id,
            layer_range,
            model_id,
            fragment_executor,
        }
    }

    /// Execute forward pass for this block
    pub fn forward_block(
        &self,
        input: FragmentActivation,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        // Validate input layer range matches expected
        if input.layer_range.1 + 1 != self.layer_range.0 && input.layer_range != (0, 0) {
            return Err(FragmentExecutorError::InvalidLayerRange(
                format!("Expected input from layers ending at {}, got {:?}",
                    self.layer_range.0.saturating_sub(1), input.layer_range)
            ));
        }

        // Execute via FragmentExecutor
        let mut output = self.fragment_executor.forward(&input)?;

        // Update layer range to this block's output
        output.layer_range = self.layer_range;

        Ok(output)
    }

    /// Execute batched forward pass for this block
    pub fn forward_block_batched(
        &self,
        input: FragmentActivation,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        // Check if this is a batched input by examining data shape or metadata
        // For now, we delegate to single forward pass - actual batching requires
        // tensor concatenation along batch dimension (dim=0)
        self.forward_block(input)
    }

    /// Process batched inference requests
    pub fn execute_batched_inference(
        &self,
        inputs: Vec<FragmentActivation>,
    ) -> Result<Vec<FragmentActivation>, FragmentExecutorError> {
        // Execute each input in the batch
        let mut outputs = Vec::with_capacity(inputs.len());
        for input in inputs {
            let output = self.forward_block(input)?;
            outputs.push(output);
        }
        Ok(outputs)
    }

    /// Generate PoI proof for block execution
    pub fn generate_block_proof(
        &self,
        task_id: Uuid,
        inference_id: Uuid,
        input: &FragmentActivation,
        output: &FragmentActivation,
        execution_time_ms: u64,
        node_id: &str,
        poi_nonce: u64,
    ) -> Result<poi_types::PoIProof, FragmentExecutorError> {
        self.fragment_executor.generate_proof(
            task_id,
            inference_id,
            input,
            output,
            execution_time_ms,
            node_id,
            None, // checkpoint_hash (Phase 4)
            poi_nonce,
        )
    }

    /// Save checkpoint after processing token
    #[allow(dead_code)]
    pub async fn save_checkpoint(
        &self,
        _inference_id: Uuid,
        _token_index: u32,
        _output_activation: &FragmentActivation,
    ) -> Result<[u8; 32], FragmentExecutorError> {
        // Stub implementation - distributed_inference dependency removed
        Ok([0u8; 32])
    }

    /// Load checkpoint for recovery
    #[allow(dead_code)]
    pub async fn load_checkpoint(
        &self,
        _inference_id: Uuid,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        // Stub implementation - distributed_inference dependency removed
        Err(FragmentExecutorError::FragmentNotFound(
            "Checkpoint loading not available without distributed_inference".to_string()
        ))
    }
    
    /// Helper: Convert FragmentActivation to CompressedTensor
    #[allow(dead_code)]
    fn activation_to_compressed_tensor(
        &self,
        _activation: &FragmentActivation,
    ) -> Result<Vec<u8>, FragmentExecutorError> {
        // Stub implementation - distributed_inference dependency removed
        Ok(Vec::new())
    }
    
    /// Helper: Convert CompressedTensor to FragmentActivation
    #[allow(dead_code)]
    fn compressed_tensor_to_activation(
        &self,
        _tensor: &[u8],
        _inference_id: Uuid,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        // Stub implementation - distributed_inference dependency removed
        Err(FragmentExecutorError::DecompressionFailed(
            "Tensor decompression not available without distributed_inference".to_string()
        ))
    }

    /// Execute forward pass with optional checkpoint saving
    pub async fn forward_block_with_checkpoint(
        &self,
        input: FragmentActivation,
        _token_index: u32,
    ) -> Result<FragmentActivation, FragmentExecutorError> {
        self.forward_block(input)
    }
}

/// Result of fragment execution with checkpoint
#[derive(Debug, Clone)]
pub struct FragmentExecutionResult {
    pub output_activation: FragmentActivation,
    pub execution_time_ms: u64,
    pub tensor_shard_index: u32,
    pub total_tensor_shards: u32,
}

/// Partial activation output from tensor parallelism shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialActivation {
    pub inference_id: Uuid,
    pub layer_range: (u32, u32),
    pub shard_index: u32,
    pub total_shards: u32,
    pub data: Vec<f32>,
    pub shape: Vec<i64>,
    pub checkpoint_hash: [u8; 32],
}

impl PartialActivation {
    /// Create a new partial activation from shard execution
    pub fn new(
        inference_id: Uuid,
        layer_range: (u32, u32),
        shard_index: u32,
        total_shards: u32,
        shape: Vec<i64>,
        data: Vec<f32>,
    ) -> Self {
        let checkpoint_hash = Self::compute_hash(&data);
        Self {
            inference_id,
            layer_range,
            shard_index,
            total_shards,
            data,
            shape,
            checkpoint_hash,
        }
    }

    /// Compute SHA3-256 hash of activation data
    pub fn compute_hash(data: &[f32]) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        let bytes: &[u8] = bytemuck::cast_slice(data);
        hasher.update(bytes);
        hasher.finalize().into()
    }

    /// Verify data integrity
    pub fn verify_integrity(&self) -> bool {
        Self::compute_hash(&self.data) == self.checkpoint_hash
    }
}

/// Aggregates partial activations from tensor parallelism shards
pub struct TensorAggregator;

impl TensorAggregator {
    /// Aggregate partial activations using all-reduce sum
    pub fn all_reduce_sum(partials: Vec<PartialActivation>) -> Result<FragmentActivation, FragmentExecutorError> {
        if partials.is_empty() {
            return Err(FragmentExecutorError::InvalidLayerRange(
                "no partial activations to aggregate".to_string()
            ));
        }

        // Verify all partials are compatible
        let first = &partials[0];
        let inference_id = first.inference_id;
        let layer_range = first.layer_range;
        let expected_shape = first.shape.clone();
        let total_shards = first.total_shards;

        if partials.len() as u32 != total_shards {
            return Err(FragmentExecutorError::InvalidLayerRange(
                format!("expected {} partials, got {}", total_shards, partials.len())
            ));
        }

        // Verify all shapes match
        for partial in &partials {
            if partial.shape != expected_shape {
                return Err(FragmentExecutorError::InvalidLayerRange(
                    "partial activation shapes mismatch".to_string()
                ));
            }
            if !partial.verify_integrity() {
                return Err(FragmentExecutorError::IntegrityCheckFailed(
                    format!("partial {} failed integrity check", partial.shard_index)
                ));
            }
        }

        // Sum all partial activations element-wise
        let total_elements = first.data.len();
        let mut aggregated_data = vec![0.0f32; total_elements];

        for partial in &partials {
            for (i, val) in partial.data.iter().enumerate() {
                aggregated_data[i] += val;
            }
        }

        // Create the aggregated activation
        let output = FragmentActivation::new(
            Uuid::new_v4(), // New task ID for aggregated result
            inference_id,
            layer_range,
            expected_shape,
            aggregated_data,
        );

        Ok(output)
    }

    /// Aggregate partial activations using average
    pub fn all_reduce_mean(partials: Vec<PartialActivation>) -> Result<FragmentActivation, FragmentExecutorError> {
        let mut result = Self::all_reduce_sum(partials)?;
        
        // Divide by number of shards to get mean
        let num_shards = result.data.len();
        for val in &mut result.data {
            *val /= num_shards as f32;
        }
        
        // Recompute hash after averaging
        result.checkpoint_hash = FragmentActivation::compute_hash(&result.data);
        
        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct InferenceMetrics {
    pub total_inference_runs: AtomicU64,
    pub total_inference_failures: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub cache_evictions: AtomicU64,
    pub total_models_loaded: AtomicU64,
    pub total_model_load_failures: AtomicU64,
    pub total_bytes_loaded: AtomicU64,
    pub total_inference_time_ms: AtomicU64,
    pub average_inference_time_ms: AtomicU64,
    pub current_models_loaded: AtomicU64,
    pub current_memory_bytes: AtomicU64,
}

impl InferenceMetrics {
    pub fn inc_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_inference_runs(&self) {
        self.total_inference_runs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_inference_failures(&self) {
        self.total_inference_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cache_evictions(&self) {
        self.cache_evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_models_loaded(&self) {
        self.total_models_loaded.fetch_add(1, Ordering::Relaxed);
        self.current_models_loaded.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_load_failures(&self) {
        self.total_model_load_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_loaded_bytes(&self, bytes: u64) {
        self.total_bytes_loaded.fetch_add(bytes, Ordering::Relaxed);
        self.current_memory_bytes
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub_loaded_bytes(&self, bytes: u64) {
        self.current_memory_bytes
            .fetch_sub(bytes, Ordering::Relaxed);
        self.current_models_loaded.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn observe_inference_time_ms(&self, sample_ms: u64) {
        self.total_inference_time_ms
            .fetch_add(sample_ms, Ordering::Relaxed);
        let prev = self.average_inference_time_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            sample_ms
        } else {
            (prev + sample_ms) / 2
        };
        self.average_inference_time_ms
            .store(next, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> InferenceStats {
        InferenceStats {
            total_inference_runs: self.total_inference_runs.load(Ordering::Relaxed),
            total_inference_failures: self.total_inference_failures.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            cache_evictions: self.cache_evictions.load(Ordering::Relaxed),
            total_models_loaded: self.total_models_loaded.load(Ordering::Relaxed),
            total_model_load_failures: self.total_model_load_failures.load(Ordering::Relaxed),
            total_bytes_loaded: self.total_bytes_loaded.load(Ordering::Relaxed),
            total_inference_time_ms: self.total_inference_time_ms.load(Ordering::Relaxed),
            average_inference_time_ms: self.average_inference_time_ms.load(Ordering::Relaxed),
            current_models_loaded: self.current_models_loaded.load(Ordering::Relaxed),
            current_memory_bytes: self.current_memory_bytes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceStats {
    pub total_inference_runs: u64,
    pub total_inference_failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_evictions: u64,
    pub total_models_loaded: u64,
    pub total_model_load_failures: u64,
    pub total_bytes_loaded: u64,
    pub total_inference_time_ms: u64,
    pub average_inference_time_ms: u64,
    pub current_models_loaded: u64,
    pub current_memory_bytes: u64,
}

pub type SharedInferenceMetrics = Arc<InferenceMetrics>;

pub struct ModelCache {
    registry: Arc<ModelRegistry>,
    storage: Arc<dyn StorageBackend>,
    cache_dir: PathBuf,
    max_memory_bytes: usize,
    num_threads: usize,
    models: DashMap<String, Arc<LoadedModel>>,
    fragment_cache: DashMap<String, Arc<FragmentExecutor>>,
    core_model_id: Arc<RwLock<Option<String>>>,
    load_locks: DashMap<String, Arc<TokioMutex<()>>>,
    fragment_load_locks: DashMap<String, Arc<TokioMutex<()>>>,
    metrics: SharedInferenceMetrics,
}

impl ModelCache {
    pub fn new(
        registry: Arc<ModelRegistry>,
        storage: Arc<dyn StorageBackend>,
        cache_dir: PathBuf,
        max_memory_bytes: usize,
        num_threads: usize,
        metrics: SharedInferenceMetrics,
    ) -> Self {
        Self {
            registry,
            storage,
            cache_dir,
            max_memory_bytes,
            num_threads,
            models: DashMap::new(),
            fragment_cache: DashMap::new(),
            core_model_id: Arc::new(RwLock::new(None)),
            load_locks: DashMap::new(),
            fragment_load_locks: DashMap::new(),
            metrics,
        }
    }

    pub fn metrics(&self) -> SharedInferenceMetrics {
        self.metrics.clone()
    }

    pub fn registry(&self) -> Arc<ModelRegistry> {
        self.registry.clone()
    }

    pub fn model_exists(&self, model_id: &str) -> Result<bool, InferenceError> {
        ensure_running()?;
        self.registry
            .model_exists(model_id)
            .map_err(map_model_error)
    }

    pub async fn get_or_load(&self, model_id: &str) -> Result<Arc<LoadedModel>, InferenceError> {
        ensure_running()?;
        if let Some(entry) = self.models.get(model_id) {
            self.metrics.inc_cache_hit();
            return Ok(entry.value().clone());
        }
        self.metrics.inc_cache_miss();

        let lock = self
            .load_locks
            .entry(model_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        if let Some(entry) = self.models.get(model_id) {
            self.metrics.inc_cache_hit();
            return Ok(entry.value().clone());
        }

        let loaded = load_model_from_shards(
            self.registry.clone(),
            self.storage.clone(),
            &self.cache_dir,
            model_id,
            self.num_threads,
        )
        .await;

        match loaded {
            Ok(model) => {
                let model = Arc::new(model);
                let memory_size = model.memory_size as u64;
                let is_core = *model.is_core.read();
                self.models.insert(model_id.to_string(), model.clone());
                self.metrics.inc_models_loaded();
                self.metrics.add_loaded_bytes(memory_size);
                if is_core {
                    *self.core_model_id.write() = Some(model_id.to_string());
                }
                if let Err(err) = self.enforce_memory_limit().await {
                    self.models.remove(model_id);
                    self.metrics.sub_loaded_bytes(memory_size);
                    if is_core && self.core_model_id.read().as_deref() == Some(model_id) {
                        *self.core_model_id.write() = None;
                    }
                    let model_dir = model
                        .model_path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| self.cache_dir.join(model_id));
                    if fs::remove_dir_all(&model_dir).await.is_err() {
                        println!(
                            "failed to remove cached model directory: {}",
                            model_dir.display()
                        );
                    }
                    return Err(err);
                }
                Ok(model)
            }
            Err(err) => {
                println!("failed to load model: model_id={}, error={}", model_id, err);
                self.metrics.inc_model_load_failures();
                Err(err)
            }
        }
    }

    async fn enforce_memory_limit(&self) -> Result<(), InferenceError> {
        loop {
            let current = self.metrics.current_memory_bytes.load(Ordering::Relaxed);
            if current <= self.max_memory_bytes as u64 {
                return Ok(());
            }
            self.evict_lru().await?;
        }
    }

    pub async fn evict_lru(&self) -> Result<(), InferenceError> {
        ensure_running()?;
        
        // Track the oldest entry across both model and fragment caches
        let mut oldest_model: Option<(String, Arc<LoadedModel>, Instant)> = None;
        let mut oldest_fragment: Option<(String, Arc<FragmentExecutor>, Instant)> = None;
        
        // Find oldest model (excluding core models)
        for entry in self.models.iter() {
            let model = entry.value();
            if *model.is_core.read() {
                continue;
            }
            let last_used = *model.last_used.read();
            let replace = oldest_model
                .as_ref()
                .map(|(_, _, oldest_time)| last_used < *oldest_time)
                .unwrap_or(true);
            if replace {
                oldest_model = Some((entry.key().clone(), model.clone(), last_used));
            }
        }
        
        // Find oldest fragment
        for entry in self.fragment_cache.iter() {
            let fragment = entry.value();
            let last_used = *fragment.last_used.read();
            let replace = oldest_fragment
                .as_ref()
                .map(|(_, _, oldest_time)| last_used < *oldest_time)
                .unwrap_or(true);
            if replace {
                oldest_fragment = Some((entry.key().clone(), fragment.clone(), last_used));
            }
        }
        
        // Decide which to evict - choose the older one
        match (&oldest_model, &oldest_fragment) {
            (Some((model_id, model, model_time)), Some((fragment_key, fragment, fragment_time))) => {
                if model_time <= fragment_time {
                    // Model is older or same age, evict it
                    self.unload_model(model_id, model.clone()).await?;
                    self.metrics.inc_cache_evictions();
                    Ok(())
                } else {
                    // Fragment is older, evict it
                    self.unload_fragment_executor(fragment_key, fragment.clone()).await?;
                    self.metrics.inc_cache_evictions();
                    Ok(())
                }
            }
            (Some((model_id, model, _)), None) => {
                // Only model available, evict it
                self.unload_model(model_id, model.clone()).await?;
                self.metrics.inc_cache_evictions();
                Ok(())
            }
            (None, Some((fragment_key, fragment, _))) => {
                // Only fragment available, evict it
                self.unload_fragment_executor(fragment_key, fragment.clone()).await?;
                self.metrics.inc_cache_evictions();
                Ok(())
            }
            (None, None) => {
                // Nothing to evict
                Err(InferenceError::OutOfMemory)
            }
        }
    }

    async fn unload_fragment_executor(
        &self,
        cache_key: &str,
        executor: Arc<FragmentExecutor>,
    ) -> Result<(), InferenceError> {
        self.fragment_cache.remove(cache_key);
        let memory_size = executor.memory_size as u64;
        self.metrics.sub_loaded_bytes(memory_size);
        
        // Clean up fragment directory
        let model_id = &executor.model_id;
        let fragment_dir = self.cache_dir.join("fragments").join(model_id);
        if fs::remove_dir_all(&fragment_dir).await.is_err() {
            println!("failed to remove fragment directory: {}", fragment_dir.display());
        }
        
        Ok(())
    }

    pub async fn unload_model(
        &self,
        model_id: &str,
        model: Arc<LoadedModel>,
    ) -> Result<(), InferenceError> {
        if *model.is_core.read() || self.core_model_id.read().as_deref() == Some(model_id) {
            return Err(InferenceError::ModelLoadFailed(
                "refusing to unload core model".to_string(),
            ));
        }
        self.models.remove(model_id);
        let memory_size = model.memory_size as u64;
        self.metrics.sub_loaded_bytes(memory_size);

        let model_dir = self.cache_dir.join(model_id);
        if fs::remove_dir_all(&model_dir).await.is_err() {
            println!(
                "failed to remove cached model directory: {}",
                model_dir.display()
            );
        }
        Ok(())
    }

    pub fn set_core_model(&self, model_id: &str) -> Result<(), InferenceError> {
        self.registry
            .set_core_model(model_id)
            .map_err(map_model_error)?;
        *self.core_model_id.write() = Some(model_id.to_string());
        for entry in self.models.iter_mut() {
            let is_core = entry.key() == model_id;
            *entry.value().is_core.write() = is_core;
        }
        Ok(())
    }

    pub fn get_core_model(&self) -> Result<Option<String>, InferenceError> {
        ensure_running()?;
        Ok(self.core_model_id.read().clone())
    }

    pub async fn preload_core_model(&self) -> Result<Option<Arc<LoadedModel>>, InferenceError> {
        ensure_running()?;
        if let Some(core) = self.registry.get_core_model().map_err(map_model_error)? {
            let model = self.get_or_load(&core.model_id).await?;
            *model.is_core.write() = true;
            *self.core_model_id.write() = Some(core.model_id.clone());
            Ok(Some(model))
        } else {
            Ok(None)
        }
    }

    /// Get or load fragments for distributed inference
    pub async fn get_or_load_fragments(
        &self,
        model_id: &str,
        fragment_ids: Vec<String>,
        layer_range: (u32, u32),
    ) -> Result<Arc<FragmentExecutor>, FragmentExecutorError> {
        // Sort fragment IDs to ensure consistent cache key regardless of order
        let mut sorted_fragment_ids = fragment_ids.clone();
        sorted_fragment_ids.sort();
        let fragment_key = sorted_fragment_ids.join(",");
        
        // Create a composite key for fragment cache including model, layer range, and fragment IDs
        let cache_key = format!("{}:{:?}:{}", model_id, layer_range, fragment_key);
        
        if let Some(entry) = self.fragment_cache.get(&cache_key) {
            self.metrics.inc_cache_hit();
            return Ok(entry.value().clone());
        }
        self.metrics.inc_cache_miss();

        let lock = self
            .fragment_load_locks
            .entry(cache_key.clone())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        if let Some(entry) = self.fragment_cache.get(&cache_key) {
            self.metrics.inc_cache_hit();
            return Ok(entry.value().clone());
        }

        let executor = load_fragments_to_vram(
            self.registry.clone(),
            self.storage.clone(),
            &self.cache_dir,
            model_id,
            sorted_fragment_ids,
            layer_range,
            self.num_threads,
        )
        .await?;

        let executor = Arc::new(executor);
        let memory_size = executor.memory_size as u64;
        
        self.fragment_cache.insert(cache_key.clone(), executor.clone());
        self.metrics.inc_models_loaded();
        self.metrics.add_loaded_bytes(memory_size);

        if let Err(err) = self.enforce_memory_limit().await {
            self.fragment_cache.remove(&cache_key);
            self.metrics.sub_loaded_bytes(memory_size);
            return Err(FragmentExecutorError::FragmentLoadFailed(
                format!("memory limit enforcement failed: {}", err)
            ));
        }

        Ok(executor)
    }

    /// Execute a fragment task with input activation
    pub async fn execute_fragment_task(
        &self,
        model_id: &str,
        fragment_ids: Vec<String>,
        layer_range: (u32, u32),
        input: &FragmentActivation,
        tensor_shard_index: u32,
        total_tensor_shards: u32,
    ) -> Result<FragmentExecutionResult, FragmentExecutorError> {
        let executor = self.get_or_load_fragments(model_id, fragment_ids, layer_range).await?;
        
        let start = Instant::now();
        let output_activation = executor.forward(input)?;
        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(FragmentExecutionResult {
            output_activation,
            execution_time_ms,
            tensor_shard_index,
            total_tensor_shards,
        })
    }

    /// Unload a specific fragment executor
    pub async fn unload_fragment(&self, model_id: &str, layer_range: (u32, u32)) -> Result<(), InferenceError> {
        let cache_key = format!("{}:{:?}", model_id, layer_range);
        
        if let Some((_, executor)) = self.fragment_cache.remove(&cache_key) {
            let memory_size = executor.memory_size as u64;
            self.metrics.sub_loaded_bytes(memory_size);
            
            // Clean up fragment directory
            let fragment_dir = self.cache_dir.join("fragments").join(model_id);
            if fs::remove_dir_all(&fragment_dir).await.is_err() {
                println!("failed to remove fragment directory: {}", fragment_dir.display());
            }
        }
        
        Ok(())
    }

    /// Clear all fragment caches
    pub async fn clear_fragment_cache(&self) -> Result<(), InferenceError> {
        let mut keys_to_remove = Vec::new();
        
        for entry in self.fragment_cache.iter() {
            keys_to_remove.push(entry.key().clone());
        }
        
        for key in keys_to_remove {
            if let Some((_, executor)) = self.fragment_cache.remove(&key) {
                let memory_size = executor.memory_size as u64;
                self.metrics.sub_loaded_bytes(memory_size);
            }
        }
        
        Ok(())
    }

    /// Resume execution from checkpoint after failover
    pub async fn resume_from_checkpoint(
        &self,
        task_id: Uuid,
        failed_node: String,
        checkpoint_hash: [u8; 32],
    ) -> Result<FragmentExecutionResult, InferenceError> {
        println!(
            "resuming from checkpoint: task_id={}, failed_node={}, checkpoint_hash={:x?}",
            task_id, failed_node, &checkpoint_hash[..8]
        );

        // 1. Try to retrieve checkpoint data from distributed storage
        // For now, we'll re-execute from scratch since checkpoint retrieval is TODO in Phase 3
        
        // 2. Create a placeholder recovery result
        // The actual layer range and model would come from task metadata in a full implementation
        let output_activation = FragmentActivation::new(
            task_id,
            Uuid::new_v4(), // Would come from task metadata
            (0, 0), // Would come from task metadata
            vec![1, 768], // Standard embedding dimension
            vec![0.0f32; 768], // Would contain actual checkpoint data
        );

        // Note: In a full implementation, we would:
        // 1. Retrieve the checkpoint data from the network or local storage
        // 2. Verify the checkpoint integrity using the checkpoint_hash
        // 3. Resume execution from the checkpointed layer
        // 4. Return the result

        // For Phase 4, we return a placeholder result indicating recovery was attempted
        Ok(FragmentExecutionResult {
            output_activation,
            execution_time_ms: 0,
            tensor_shard_index: 0,
            total_tensor_shards: 1,
        })
    }
}

/// Combine shard files into a single model file
async fn combine_shards(
    shards: &[ModelShard],
    shard_dir: &Path,
    model_path: &Path,
) -> Result<(), InferenceError> {
    use tokio::io::AsyncWriteExt;
    
    let mut model_file = fs::File::create(model_path).await
        .map_err(|e| InferenceError::ModelLoadFailed(format!("failed to create model file: {}", e)))?;
    
    // Sort shards by index to ensure correct order
    let mut sorted_shards: Vec<_> = shards.to_vec();
    sorted_shards.sort_by_key(|s| s.shard_index);
    
    for shard in &sorted_shards {
        let shard_path = shard_dir.join(format!("{}_shard_{}.bin", shard.model_id, shard.shard_index));
        let mut shard_file = fs::File::open(&shard_path).await
            .map_err(|e| InferenceError::ModelLoadFailed(format!("failed to open shard {}: {}", shard.shard_index, e)))?;
        
        let mut shard_data = Vec::new();
        shard_file.read_to_end(&mut shard_data).await
            .map_err(|e| InferenceError::ModelLoadFailed(format!("failed to read shard {}: {}", shard.shard_index, e)))?;
        
        model_file.write_all(&shard_data).await
            .map_err(|e| InferenceError::ModelLoadFailed(format!("failed to write shard {} to model: {}", shard.shard_index, e)))?;
    }
    
    println!("combined {} shards into {}", shards.len(), model_path.display());
    Ok(())
}

async fn load_model_from_shards(
    registry: Arc<ModelRegistry>,
    storage: Arc<dyn StorageBackend>,
    cache_dir: &Path,
    model_id: &str,
    num_threads: usize,
) -> Result<LoadedModel, InferenceError> {
    ensure_running()?;
    let metadata = registry.get_model(model_id).map_err(map_model_error)?;
    let shards = registry.list_shards(model_id).map_err(map_model_error)?;
    if shards.is_empty() {
        return Err(InferenceError::ModelLoadFailed(
            "no shards registered".to_string(),
        ));
    }

    println!(
        "loading model from shards: model_id={}, shards={}",
        model_id,
        shards.len()
    );

    let model_dir = cache_dir.join(model_id);
    let shard_dir = model_dir.join("shards");
    let model_path = model_dir.join("model.onnx");
    fs::create_dir_all(&shard_dir).await?;

    for shard in shards.iter() {
        ensure_running()?;
        let shard_path = shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
        // Wrap storage call in spawn_blocking to make it Send-safe
        let storage_clone = storage.clone();
        let shard_clone = shard.clone();
        let path_clone = shard_path.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::try_current()
                .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
            rt.block_on(async move {
                storage_clone.download_shard(&shard_clone, &path_clone).await
            })
        })
        .await
        .map_err(|e| InferenceError::ModelLoadFailed(e.to_string()))??;
        let valid = verify_shard_integrity(&shard_path, &shard.hash).await?;
        if !valid {
            println!(
                "shard integrity check failed: model={}, shard={}",
                model_id, shard.shard_index
            );
            return Err(InferenceError::ShardError(ShardError::HashMismatch(
                shard.shard_index,
            )));
        }
    }

    if fs::metadata(&model_path).await.is_ok() && fs::remove_file(&model_path).await.is_err() {
        println!(
            "failed to remove cached model file: {}",
            model_path.display()
        );
    }
    combine_shards(&shards, &shard_dir, &model_path).await?;

    let session = create_onnx_session(&model_path, num_threads)?;
    let input_names = session
        .inputs()
        .iter()
        .map(|input| input.name().to_string())
        .collect();
    let output_names = session
        .outputs()
        .iter()
        .map(|output| output.name().to_string())
        .collect();

    let is_core = metadata.is_core_model;
    Ok(LoadedModel {
        model_id: metadata.model_id,
        session: Mutex::new(session),
        input_names,
        output_names,
        memory_size: metadata.total_size as usize,
        last_used: Arc::new(RwLock::new(Instant::now())),
        is_core: Arc::new(RwLock::new(is_core)),
        model_path,
        shard_dir,
    })
}

fn create_onnx_session(model_path: &Path, num_threads: usize) -> Result<Session, InferenceError> {
    init_onnx_runtime()?;
    let builder = Session::builder()
        .map_err(|err: ort::Error| InferenceError::ModelLoadFailed(err.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|err: ort::Error| InferenceError::ModelLoadFailed(err.to_string()))?;

    let builder = if num_threads > 0 {
        builder
            .with_intra_threads(num_threads)
            .map_err(|err: ort::Error| InferenceError::ModelLoadFailed(err.to_string()))?
    } else {
        builder
    };

    builder
        .commit_from_file(model_path)
        .map_err(|err: ort::Error| InferenceError::ModelLoadFailed(err.to_string()))
}

/// Load model fragments into VRAM for distributed inference
async fn load_fragments_to_vram(
    registry: Arc<ModelRegistry>,
    storage: Arc<dyn StorageBackend>,
    cache_dir: &Path,
    model_id: &str,
    fragment_ids: Vec<String>,
    layer_range: (u32, u32),
    num_threads: usize,
) -> Result<FragmentExecutor, FragmentExecutorError> {
    ensure_running().map_err(|_| FragmentExecutorError::Shutdown)?;

    // Get fragment metadata from registry
    let manifest = registry.get_fragment_manifest(model_id)
        .map_err(|e| FragmentExecutorError::FragmentNotFound(e.to_string()))?;

    // Find the fragments we need
    let mut fragments = Vec::new();
    for frag_id in &fragment_ids {
        let fragment = manifest.fragments.iter()
            .find(|f| &f.fragment_id == frag_id)
            .ok_or_else(|| FragmentExecutorError::FragmentNotFound(frag_id.clone()))?;
        fragments.push(fragment.clone());
    }

    if fragments.is_empty() {
        return Err(FragmentExecutorError::FragmentLoadFailed(
            "no fragments to load".to_string()
        ));
    }

    println!(
        "loading fragments to VRAM: model_id={}, fragments={}, layers={:?}",
        model_id,
        fragments.len(),
        layer_range
    );

    let fragment_dir = cache_dir.join("fragments").join(model_id);
    let combined_path = fragment_dir.join("combined.onnx");
    fs::create_dir_all(&fragment_dir).await
        .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;

    // Download and verify each fragment
    for fragment in &fragments {
        ensure_running().map_err(|_| FragmentExecutorError::Shutdown)?;
        
        let fragment_path = fragment_dir.join(format!("{}_frag_{}.bin", model_id, fragment.fragment_index));
        
        // Download from storage (first available location)
        if let Some(_location) = fragment.locations.first() {
            // Wrap storage call in spawn_blocking to make it Send-safe
            let storage_clone = storage.clone();
            let fragment_clone = fragment.clone();
            let path_clone = fragment_path.clone();
            tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::try_current()
                    .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
                rt.block_on(async move {
                    storage_clone.download_fragment(&fragment_clone, &path_clone).await
                })
            })
            .await
            .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))??;
        } else {
            return Err(FragmentExecutorError::FragmentLoadFailed(
                format!("no storage location for fragment {}", fragment.fragment_id)
            ));
        }

        // Verify integrity
        let mut file = fs::File::open(&fragment_path).await
            .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).await
            .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;

        // Decompress if needed
        let data_to_verify = if fragment.is_compressed {
            zstd::decode_all(data.as_slice())
                .map_err(|e| FragmentExecutorError::DecompressionFailed(e.to_string()))?
        } else {
            data
        };

        // Verify hash
        let mut hasher = Sha3_256::new();
        hasher.update(&data_to_verify);
        let actual_hash: [u8; 32] = hasher.finalize().into();
        
        if actual_hash != fragment.hash {
            return Err(FragmentExecutorError::IntegrityCheckFailed(
                format!("fragment {} hash mismatch", fragment.fragment_id)
            ));
        }

        println!("fragment {} verified", fragment.fragment_id);
    }

    // Combine fragments into partial model
    // For simplicity, we're combining all fragments into a single ONNX file
    // In a real implementation, you might extract only the layers needed
    let fragment_refs: Vec<WeightFragment> = fragments.clone();
    combine_fragments(&fragment_refs, &fragment_dir, &combined_path).await
        .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;

    // Create ONNX session
    let session = create_fragment_session(&combined_path, num_threads)?;
    
    let input_names = session
        .inputs()
        .iter()
        .map(|input| input.name().to_string())
        .collect();
    
    let output_names = session
        .outputs()
        .iter()
        .map(|output| output.name().to_string())
        .collect();

    // Calculate memory size
    let metadata = fs::metadata(&combined_path).await
        .map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;
    let memory_size = metadata.len() as usize;
    let vram_allocated_mb = (memory_size / (1024 * 1024)) as u32;

    println!(
        "fragment executor ready: model_id={}, memory={}MB, layers={:?}",
        model_id,
        vram_allocated_mb,
        layer_range
    );

    Ok(FragmentExecutor {
        model_id: model_id.to_string(),
        fragment_ids,
        layer_range,
        session: Mutex::new(session),
        input_names,
        output_names,
        memory_size,
        vram_allocated_mb,
        last_used: Arc::new(RwLock::new(Instant::now())),
        fragment_dir,
    })
}

fn create_fragment_session(model_path: &Path, num_threads: usize) -> Result<Session, FragmentExecutorError> {
    init_onnx_runtime().map_err(|e| FragmentExecutorError::FragmentLoadFailed(e.to_string()))?;
    
    let builder = Session::builder()
        .map_err(|err| FragmentExecutorError::FragmentLoadFailed(err.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|err| FragmentExecutorError::FragmentLoadFailed(err.to_string()))?;

    let builder = if num_threads > 0 {
        builder
            .with_intra_threads(num_threads)
            .map_err(|err| FragmentExecutorError::FragmentLoadFailed(err.to_string()))?
    } else {
        builder
    };

    builder
        .commit_from_file(model_path)
        .map_err(|err| FragmentExecutorError::FragmentLoadFailed(err.to_string()))
}

pub struct InferenceEngine {
    cache: Arc<ModelCache>,
    metrics: SharedInferenceMetrics,
    ad_manager: Option<Arc<AdManager>>,
    continual_learning: Option<Arc<crate::ContinualLearningManager>>,
    /// Training trigger callback - called when buffer reaches threshold
    training_trigger: Option<Arc<dyn Fn(String, Vec<f32>, Vec<f32>, f32) -> bool + Send + Sync>>,
}

impl InferenceEngine {
    pub fn new(
        registry: Arc<ModelRegistry>,
        storage: Arc<dyn StorageBackend>,
        cache_dir: PathBuf,
        max_memory_bytes: usize,
        num_threads: usize,
        ad_manager: Option<Arc<AdManager>>,
        continual_learning: Option<Arc<crate::ContinualLearningManager>>,
        training_trigger: Option<Arc<dyn Fn(String, Vec<f32>, Vec<f32>, f32) -> bool + Send + Sync>>,
    ) -> Self {
        let metrics = Arc::new(InferenceMetrics::default());
        let cache = Arc::new(ModelCache::new(
            registry,
            storage,
            cache_dir,
            max_memory_bytes,
            num_threads,
            metrics.clone(),
        ));
        Self {
            cache,
            metrics,
            ad_manager,
            continual_learning,
            training_trigger,
        }
    }

    /// Set the training trigger callback
    pub fn set_training_trigger(
        &mut self,
        trigger: Arc<dyn Fn(String, Vec<f32>, Vec<f32>, f32) -> bool + Send + Sync>,
    ) {
        self.training_trigger = Some(trigger);
    }

    pub fn cache(&self) -> Arc<ModelCache> {
        self.cache.clone()
    }

    pub fn model_exists(&self, model_id: &str) -> Result<bool, InferenceError> {
        self.cache.model_exists(model_id)
    }

    pub fn metrics(&self) -> SharedInferenceMetrics {
        self.metrics.clone()
    }

    pub fn registry(&self) -> Arc<ModelRegistry> {
        self.cache.registry()
    }

    pub fn get_metrics(&self) -> InferenceStats {
        self.metrics.snapshot()
    }

    pub async fn preload_core_model(&self) -> Result<Option<Arc<LoadedModel>>, InferenceError> {
        self.cache.preload_core_model().await
    }

    pub async fn run_inference(
        &self,
        model_id: &str,
        inputs: Vec<InferenceTensor>,
        user_address: &str,
    ) -> Result<Vec<InferenceOutput>, InferenceError> {
        ensure_running()?;
        let start = Instant::now();
        let model = self.cache.get_or_load(model_id).await?;
        
        // Clone inputs for continual learning before they're moved
        let input_flat: Vec<f32> = inputs.iter().flat_map(|t| t.data.clone()).collect();
        
        let result = model.run_inference(inputs);
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.metrics.observe_inference_time_ms(elapsed_ms);

        match result {
            Ok(outputs) => {
                self.metrics.inc_inference_runs();

                // Post-inference continual learning hook
                let mut should_trigger = false;
                if let Some(cl_manager) = self.continual_learning.as_ref() {
                    // Compute loss (placeholder - in production, use actual loss from model)
                    let loss = 0.1f32; // TODO: extract from model output or compute from ground truth

                    // Flatten outputs to Vec<f32>
                    let output_flat: Vec<f32> = outputs.iter().flat_map(|o| o.data.clone()).collect();

                    // Record inference for continual learning
                    if let Err(e) = cl_manager.record_inference(model_id, input_flat.clone(), output_flat.clone(), loss).await {
                        log::warn!("Failed to record inference for continual learning: {}", e);
                    }

                    // Check if buffer is ready for training trigger
                    if cl_manager.is_buffer_ready(model_id) {
                        should_trigger = true;
                        log::info!("Buffer ready for model {} - training should be triggered", model_id);
                    }

                    // Invoke training trigger callback if set
                    if should_trigger {
                        if let Some(trigger) = self.training_trigger.as_ref() {
                            let _ = trigger(model_id.to_string(), input_flat, output_flat, loss);
                        }
                    }
                }

                if let Some(ad_manager) = self.ad_manager.as_ref() {
                    match ad_manager.should_inject_ad(user_address) {
                        Ok(true) => match ad_manager.inject_ad_into_outputs(
                            outputs.clone(),
                            user_address,
                            model_id,
                        ) {
                            Ok((modified, template_id, tier)) => {
                                if !template_id.is_empty() {
                                    let _ = ad_manager.record_impression(
                                        user_address,
                                        &template_id,
                                        model_id,
                                        tier,
                                    );
                                    println!(
                                        "ad injected for user {}, template {}",
                                        user_address, template_id
                                    );
                                }
                                Ok(modified)
                            }
                            Err(err) => {
                                println!("ad injection failed: {}", err);
                                Ok(outputs)
                            }
                        },
                        Ok(false) => Ok(outputs),
                        Err(err) => {
                            println!("ad injection check failed: {}", err);
                            Ok(outputs)
                        }
                    }
                } else {
                    Ok(outputs)
                }
            }
            Err(err) => {
                self.metrics.inc_inference_failures();
                println!("inference failed: model_id={}, error={}", model_id, err);
                Err(err)
            }
        }
    }
}
