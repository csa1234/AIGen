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
use thiserror::Error;
use tokio::fs;
use tokio::sync::Mutex as TokioMutex;

use crate::registry::ModelError;
use crate::sharding::{combine_shards, verify_shard_integrity, ShardError};
use crate::storage::{StorageBackend, StorageError};
use crate::ModelRegistry;

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

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
        let builder = ort::init_from(&dylib_path)
            .map_err(|err| format!("failed to initialize ONNX Runtime from {}: {err}", dylib_path.display()))?;
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

    fn order_inputs(&self, inputs: Vec<InferenceTensor>) -> Result<Vec<InferenceTensor>, InferenceError> {
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

    fn tensor_to_value(&self, tensor: &InferenceTensor) -> Result<(String, Tensor<f32>), InferenceError> {
        let value = Tensor::from_array((tensor.shape.clone(), tensor.data.clone()))
            .map_err(|err: ort::Error| InferenceError::InvalidInput(err.to_string()))?;
        Ok((tensor.name.clone(), value))
    }

    fn parse_outputs(&self, outputs: SessionOutputs<'_>) -> Result<Vec<InferenceOutput>, InferenceError> {
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
        self.total_inference_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cache_evictions(&self) {
        self.cache_evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_models_loaded(&self) {
        self.total_models_loaded.fetch_add(1, Ordering::Relaxed);
        self.current_models_loaded.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_load_failures(&self) {
        self.total_model_load_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_loaded_bytes(&self, bytes: u64) {
        self.total_bytes_loaded.fetch_add(bytes, Ordering::Relaxed);
        self.current_memory_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn sub_loaded_bytes(&self, bytes: u64) {
        self.current_memory_bytes.fetch_sub(bytes, Ordering::Relaxed);
        self.current_models_loaded.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn observe_inference_time_ms(&self, sample_ms: u64) {
        self.total_inference_time_ms
            .fetch_add(sample_ms, Ordering::Relaxed);
        let prev = self.average_inference_time_ms.load(Ordering::Relaxed);
        let next = if prev == 0 { sample_ms } else { (prev + sample_ms) / 2 };
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
    core_model_id: Arc<RwLock<Option<String>>>,
    load_locks: DashMap<String, Arc<TokioMutex<()>>>,
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
            core_model_id: Arc::new(RwLock::new(None)),
            load_locks: DashMap::new(),
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
        self.registry.model_exists(model_id).map_err(map_model_error)
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
                    if is_core {
                        if self.core_model_id.read().as_deref() == Some(model_id) {
                            *self.core_model_id.write() = None;
                        }
                    }
                    let model_dir = model
                        .model_path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| self.cache_dir.join(model_id));
                    if fs::remove_dir_all(&model_dir).await.is_err() {
                        println!("failed to remove cached model directory: {}", model_dir.display());
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
        let mut oldest: Option<(String, Arc<LoadedModel>, Instant)> = None;
        for entry in self.models.iter() {
            let model = entry.value();
            if *model.is_core.read() {
                continue;
            }
            let last_used = *model.last_used.read();
            let replace = oldest
                .as_ref()
                .map(|(_, _, oldest_time)| last_used < *oldest_time)
                .unwrap_or(true);
            if replace {
                oldest = Some((entry.key().clone(), model.clone(), last_used));
            }
        }

        if let Some((model_id, model, _)) = oldest {
            self.unload_model(&model_id, model).await?;
            self.metrics.inc_cache_evictions();
            Ok(())
        } else {
            Err(InferenceError::OutOfMemory)
        }
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
            println!("failed to remove cached model directory: {}", model_dir.display());
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
        return Err(InferenceError::ModelLoadFailed("no shards registered".to_string()));
    }

    println!("loading model from shards: model_id={}, shards={}", model_id, shards.len());

    let model_dir = cache_dir.join(model_id);
    let shard_dir = model_dir.join("shards");
    let model_path = model_dir.join("model.onnx");
    fs::create_dir_all(&shard_dir).await?;

    for shard in shards.iter() {
        ensure_running()?;
        let shard_path = shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
        storage.download_shard(shard, &shard_path).await?;
        let valid = verify_shard_integrity(&shard_path, &shard.hash).await?;
        if !valid {
            println!("shard integrity check failed: model={}, shard={}", model_id, shard.shard_index);
            return Err(InferenceError::ShardError(ShardError::HashMismatch(
                shard.shard_index,
            )));
        }
    }

    if fs::metadata(&model_path).await.is_ok() {
        if fs::remove_file(&model_path).await.is_err() {
            println!("failed to remove cached model file: {}", model_path.display());
        }
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

pub struct InferenceEngine {
    cache: Arc<ModelCache>,
    metrics: SharedInferenceMetrics,
}

impl InferenceEngine {
    pub fn new(
        registry: Arc<ModelRegistry>,
        storage: Arc<dyn StorageBackend>,
        cache_dir: PathBuf,
        max_memory_bytes: usize,
        num_threads: usize,
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
        Self { cache, metrics }
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
    ) -> Result<Vec<InferenceOutput>, InferenceError> {
        ensure_running()?;
        let start = Instant::now();
        let model = self.cache.get_or_load(model_id).await?;
        let result = model.run_inference(inputs);
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.metrics.observe_inference_time_ms(elapsed_ms);

        match result {
            Ok(outputs) => {
                self.metrics.inc_inference_runs();
                Ok(outputs)
            }
            Err(err) => {
                self.metrics.inc_inference_failures();
                println!("inference failed: model_id={}, error={}", model_id, err);
                Err(err)
            }
        }
    }
}
