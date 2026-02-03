// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Inference verification helpers for deterministic PoI checks.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use genesis::{check_shutdown, CEO_WALLET};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::inference::{InferenceEngine, InferenceError, InferenceOutput, InferenceTensor};
use crate::registry::ModelError;

pub const DEFAULT_VERIFICATION_CACHE_CAPACITY: usize = 256;
pub const DEFAULT_VERIFICATION_EPSILON: f32 = 1e-5;

static GLOBAL_ENGINE: OnceLock<Arc<InferenceEngine>> = OnceLock::new();

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("inference engine unavailable")]
    EngineUnavailable,
    #[error("inference engine already initialized")]
    EngineAlreadyInitialized,
    #[error("shutdown active")]
    Shutdown,
    #[error("inference error: {0}")]
    Inference(#[from] InferenceError),
    #[error("model error: {0}")]
    Model(#[from] ModelError),
    #[error("serialization error: {0}")]
    Serialization(String),
}

fn ensure_running() -> Result<(), VerificationError> {
    check_shutdown().map_err(|_| VerificationError::Shutdown)
}

#[derive(Debug)]
pub struct VerificationCache {
    capacity: usize,
    inner: Mutex<CacheInner>,
}

#[derive(Debug)]
struct CacheInner {
    map: HashMap<[u8; 32], CacheEntry>,
    lru: VecDeque<[u8; 32]>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    outputs: Vec<InferenceOutput>,
    last_used: Instant,
}

impl VerificationCache {
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity == 0 {
            DEFAULT_VERIFICATION_CACHE_CAPACITY
        } else {
            capacity
        };
        Self {
            capacity,
            inner: Mutex::new(CacheInner {
                map: HashMap::new(),
                lru: VecDeque::new(),
            }),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<Vec<InferenceOutput>> {
        let mut inner = self.inner.lock();
        let outputs = inner.map.get(key).map(|entry| entry.outputs.clone());
        if outputs.is_some() {
            inner.touch(*key);
        }
        outputs
    }

    pub fn insert(&self, key: [u8; 32], outputs: Vec<InferenceOutput>) {
        let mut inner = self.inner.lock();
        let now = Instant::now();
        let entry = CacheEntry {
            outputs,
            last_used: now,
        };

        if let std::collections::hash_map::Entry::Occupied(mut existing) = inner.map.entry(key) {
            existing.insert(entry);
            inner.touch(key);
            return;
        }

        if inner.map.len() >= self.capacity {
            inner.evict_lru();
        }

        inner.map.insert(key, entry);
        inner.lru.push_front(key);
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.map.clear();
        inner.lru.clear();
    }
}

impl CacheInner {
    fn touch(&mut self, key: [u8; 32]) {
        if let Some(entry) = self.map.get_mut(&key) {
            entry.last_used = Instant::now();
        }
        if let Some(pos) = self.lru.iter().position(|existing| existing == &key) {
            self.lru.remove(pos);
        }
        self.lru.push_front(key);
    }

    fn evict_lru(&mut self) {
        if let Some(key) = self.lru.pop_back() {
            self.map.remove(&key);
        }
    }
}

pub fn set_global_inference_engine(engine: Arc<InferenceEngine>) -> Result<(), VerificationError> {
    GLOBAL_ENGINE
        .set(engine)
        .map_err(|_| VerificationError::EngineAlreadyInitialized)
}

pub fn global_inference_engine() -> Option<Arc<InferenceEngine>> {
    GLOBAL_ENGINE.get().cloned()
}

pub fn model_exists(model_id: &str) -> Result<bool, VerificationError> {
    let engine = GLOBAL_ENGINE
        .get()
        .ok_or(VerificationError::EngineUnavailable)?;
    match engine.model_exists(model_id) {
        Ok(exists) => Ok(exists),
        Err(InferenceError::Shutdown) => Err(VerificationError::Shutdown),
        Err(err) => Err(VerificationError::Inference(err)),
    }
}

pub fn hash_inference_key(model_id: &str, inputs: &[InferenceTensor]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(model_id.as_bytes());

    let mut ordered: Vec<&InferenceTensor> = inputs.iter().collect();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));

    for tensor in ordered {
        hasher.update(tensor.name.as_bytes());
        hasher.update((tensor.shape.len() as u64).to_le_bytes());
        for dim in &tensor.shape {
            hasher.update(dim.to_le_bytes());
        }
        hasher.update((tensor.data.len() as u64).to_le_bytes());
        for value in &tensor.data {
            hasher.update(value.to_le_bytes());
        }
    }

    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

pub async fn deterministic_inference(
    model_id: &str,
    inputs: Vec<InferenceTensor>,
    cache: Option<&VerificationCache>,
) -> Result<Vec<InferenceOutput>, VerificationError> {
    let engine = GLOBAL_ENGINE
        .get()
        .ok_or(VerificationError::EngineUnavailable)?;
    deterministic_inference_with_engine(engine, model_id, inputs, cache).await
}

pub async fn deterministic_inference_with_engine(
    engine: &InferenceEngine,
    model_id: &str,
    inputs: Vec<InferenceTensor>,
    cache: Option<&VerificationCache>,
) -> Result<Vec<InferenceOutput>, VerificationError> {
    ensure_running()?;

    let cache_key = cache.map(|_| hash_inference_key(model_id, &inputs));
    if let (Some(cache), Some(key)) = (cache, cache_key) {
        if let Some(outputs) = cache.get(&key) {
            println!("verification cache hit");
            return Ok(outputs);
        }
    }

    let outputs = engine.run_inference(model_id, inputs, CEO_WALLET).await?;

    if let (Some(cache), Some(key)) = (cache, cache_key) {
        cache.insert(key, outputs.clone());
    }

    Ok(outputs)
}

pub fn outputs_match(
    expected: &[InferenceOutput],
    actual: &[InferenceOutput],
    epsilon: f32,
) -> bool {
    if expected.len() != actual.len() {
        return false;
    }

    let mut expected_sorted: Vec<&InferenceOutput> = expected.iter().collect();
    let mut actual_sorted: Vec<&InferenceOutput> = actual.iter().collect();
    expected_sorted.sort_by(|a, b| a.name.cmp(&b.name));
    actual_sorted.sort_by(|a, b| a.name.cmp(&b.name));

    for (expected_output, actual_output) in expected_sorted.into_iter().zip(actual_sorted) {
        if expected_output.name != actual_output.name {
            return false;
        }
        if expected_output.shape != actual_output.shape {
            return false;
        }
        if expected_output.data.len() != actual_output.data.len() {
            return false;
        }
        for (expected_value, actual_value) in
            expected_output.data.iter().zip(actual_output.data.iter())
        {
            if (expected_value - actual_value).abs() > epsilon {
                return false;
            }
        }
    }

    true
}
