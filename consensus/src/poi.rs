use blockchain_core::hash_data;
use blockchain_core::types::Amount;
use blockchain_core::{Block, BlockHash, Transaction};
use base64::Engine;
use genesis::{check_shutdown, is_shutdown, GenesisError};
use model::{
    deterministic_inference,
    model_exists,
    outputs_match,
    InferenceOutput,
    InferenceTensor,
    VerificationCache,
    VerificationError,
    DEFAULT_VERIFICATION_CACHE_CAPACITY,
    DEFAULT_VERIFICATION_EPSILON,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::OnceLock;
use thiserror::Error;
use tokio::runtime::Builder;
use tokio::task;
use tracing::warn;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("shutdown active")]
    ShutdownActive,
    #[error("genesis error: {0}")]
    Genesis(#[from] GenesisError),
    #[error("invalid proof")]
    InvalidProof,
    #[error("proof rejected")]
    ProofRejected,
    #[error("invalid vote")]
    InvalidVote,
    #[error("registry error")]
    RegistryError,
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("verification error: {0}")]
    VerificationError(String),
    #[error("timeout")]
    Timeout,
}

impl From<bincode::Error> for ConsensusError {
    fn from(err: bincode::Error) -> Self {
        ConsensusError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for ConsensusError {
    fn from(err: serde_json::Error) -> Self {
        ConsensusError::Serialization(err.to_string())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkType {
    MatrixMultiplication,
    GradientDescent,
    Inference,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionMethod {
    None,
    Quantize8Bit,
    Quantize4Bit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComputationMetadata {
    pub rows: u32,
    pub cols: u32,
    pub inner: u32,
    pub iterations: u32,
    pub model_id: String,
    pub compression_method: CompressionMethod,
    pub original_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceVerificationConfig {
    pub cache_capacity: usize,
    pub epsilon: f32,
}

impl Default for InferenceVerificationConfig {
    fn default() -> Self {
        Self {
            cache_capacity: DEFAULT_VERIFICATION_CACHE_CAPACITY,
            epsilon: DEFAULT_VERIFICATION_EPSILON,
        }
    }
}

static INFERENCE_VERIFICATION_CONFIG: OnceLock<InferenceVerificationConfig> = OnceLock::new();
static INFERENCE_VERIFICATION_CACHE: OnceLock<VerificationCache> = OnceLock::new();

pub fn set_inference_verification_config(
    config: InferenceVerificationConfig,
) -> Result<(), ConsensusError> {
    INFERENCE_VERIFICATION_CONFIG
        .set(config)
        .map_err(|_| ConsensusError::VerificationError("verification config already set".to_string()))
}

fn inference_verification_config() -> &'static InferenceVerificationConfig {
    INFERENCE_VERIFICATION_CONFIG.get_or_init(InferenceVerificationConfig::default)
}

fn inference_verification_cache() -> &'static VerificationCache {
    INFERENCE_VERIFICATION_CACHE.get_or_init(|| {
        VerificationCache::new(inference_verification_config().cache_capacity)
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PoIProof {
    pub work_hash: [u8; 32],
    pub miner_address: String,
    pub timestamp: i64,
    pub verification_data: serde_json::Value,
    pub work_type: WorkType,
    pub input_hash: [u8; 32],
    pub output_data: Vec<u8>,
    pub computation_metadata: ComputationMetadata,
    pub difficulty: u64,
    pub nonce: u64,
}

impl PoIProof {
    pub fn new(
        miner_address: String,
        work_type: WorkType,
        input_hash: [u8; 32],
        output_data: Vec<u8>,
        computation_metadata: ComputationMetadata,
        difficulty: u64,
        timestamp: i64,
        nonce: u64,
        verification_data: serde_json::Value,
    ) -> Self {
        let work_hash = hash_data(&Self::work_payload_bytes(
            &miner_address,
            work_type,
            input_hash,
            &output_data,
            &computation_metadata,
            difficulty,
            timestamp,
            nonce,
        ));

        Self {
            work_hash,
            miner_address,
            timestamp,
            verification_data,
            work_type,
            input_hash,
            output_data,
            computation_metadata,
            difficulty,
            nonce,
        }
    }

    fn work_payload_bytes(
        miner_address: &str,
        work_type: WorkType,
        input_hash: [u8; 32],
        output_data: &[u8],
        computation_metadata: &ComputationMetadata,
        difficulty: u64,
        timestamp: i64,
        nonce: u64,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(miner_address.as_bytes());
        buf.push(work_type as u8);
        buf.extend_from_slice(&input_hash);
        buf.extend_from_slice(&(output_data.len() as u64).to_le_bytes());
        buf.extend_from_slice(output_data);
        let meta_bytes = bincode::serialize(computation_metadata).unwrap_or_default();
        buf.extend_from_slice(&(meta_bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(&meta_bytes);
        buf.extend_from_slice(&difficulty.to_le_bytes());
        buf.extend_from_slice(&timestamp.to_le_bytes());
        buf.extend_from_slice(&nonce.to_le_bytes());
        buf
    }

    pub fn verify(&self) -> Result<bool, ConsensusError> {
        check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;

        let expected = hash_data(&Self::work_payload_bytes(
            &self.miner_address,
            self.work_type,
            self.input_hash,
            &self.output_data,
            &self.computation_metadata,
            self.difficulty,
            self.timestamp,
            self.nonce,
        ));
        if expected != self.work_hash {
            return Err(ConsensusError::InvalidProof);
        }

        let now = chrono::Utc::now().timestamp();
        if self.timestamp <= 0 {
            return Err(ConsensusError::InvalidProof);
        }
        if self.timestamp > now + 300 || self.timestamp < now - (86_400 * 365) {
            return Err(ConsensusError::InvalidProof);
        }

        match self.work_type {
            WorkType::MatrixMultiplication => {
                let input = self
                    .verification_data
                    .get("input")
                    .and_then(|v| v.as_str())
                    .ok_or(ConsensusError::InvalidProof)?;
                let input_bytes = base64::engine::general_purpose::STANDARD
                    .decode(input)
                    .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
                let ok = verify_matrix_multiplication(&input_bytes, &self.output_data, &self.computation_metadata)?;
                if !ok {
                    return Err(ConsensusError::InvalidProof);
                }
            }
            WorkType::GradientDescent => {
                let input = self
                    .verification_data
                    .get("input")
                    .and_then(|v| v.as_str())
                    .ok_or(ConsensusError::InvalidProof)?;
                let input_bytes = base64::engine::general_purpose::STANDARD
                    .decode(input)
                    .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
                let ok = verify_gradient_descent(&input_bytes, &self.output_data, &self.computation_metadata)?;
                if !ok {
                    return Err(ConsensusError::InvalidProof);
                }
            }
            WorkType::Inference => {
                let input = self
                    .verification_data
                    .get("input")
                    .and_then(|v| v.as_str())
                    .ok_or(ConsensusError::InvalidProof)?;
                let input_bytes = base64::engine::general_purpose::STANDARD
                    .decode(input)
                    .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
                let ok = verify_inference(&input_bytes, &self.output_data, &self.computation_metadata)?;
                if !ok {
                    return Err(ConsensusError::InvalidProof);
                }
            }
        }

        Ok(true)
    }
}

pub fn verify_matrix_multiplication(
    input: &[u8],
    output: &[u8],
    metadata: &ComputationMetadata,
) -> Result<bool, ConsensusError> {
    check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;

    let rows = metadata.rows as usize;
    let cols = metadata.cols as usize;
    let inner = metadata.inner as usize;
    if rows == 0 || cols == 0 || inner == 0 {
        return Ok(false);
    }

    let (a, b): (Vec<f32>, Vec<f32>) = bincode::deserialize(input)?;
    if a.len() != rows * inner || b.len() != inner * cols {
        return Ok(false);
    }

    let out: Vec<f32> = bincode::deserialize(output)?;
    if out.len() != rows * cols {
        return Ok(false);
    }

    let mut rng = rand::thread_rng();
    let checks = 8usize.min(rows * cols);
    for _ in 0..checks {
        let r = rng.gen_range(0..rows);
        let c = rng.gen_range(0..cols);
        let mut acc = 0.0f32;
        for k in 0..inner {
            acc += a[r * inner + k] * b[k * cols + c];
        }
        let got = out[r * cols + c];
        if (acc - got).abs() > 1e-2 {
            return Ok(false);
        }
    }

    Ok(true)
}

pub fn verify_gradient_descent(
    input: &[u8],
    output: &[u8],
    metadata: &ComputationMetadata,
) -> Result<bool, ConsensusError> {
    check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;

    let (loss_before, loss_after, grad_len): (f32, f32, u32) = bincode::deserialize(input)?;
    if !(loss_after.is_finite() && loss_before.is_finite()) {
        return Ok(false);
    }
    if loss_after > loss_before {
        return Ok(false);
    }

    let grad_len_usize = grad_len as usize;
    let decompressed = match metadata.compression_method {
        CompressionMethod::None => {
            let g: Vec<f32> = bincode::deserialize(output)?;
            g
        }
        CompressionMethod::Quantize8Bit => decompress_gradients(output),
        CompressionMethod::Quantize4Bit => decompress_gradients_4bit(output, grad_len_usize),
    };

    if decompressed.len() != grad_len_usize {
        return Ok(false);
    }

    if metadata.original_size == 0 {
        return Ok(false);
    }
    let compression_ratio = (metadata.original_size as f64) / (output.len().max(1) as f64);
    if compression_ratio <= 4.0 {
        return Ok(false);
    }

    Ok(true)
}

pub fn verify_inference(
    input: &[u8],
    output: &[u8],
    metadata: &ComputationMetadata,
) -> Result<bool, ConsensusError> {
    check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;

    if metadata.model_id.trim().is_empty() {
        return Ok(false);
    }

    match model_exists(&metadata.model_id) {
        Ok(false) => return Ok(false),
        Ok(true) => {}
        Err(VerificationError::Shutdown) => return Err(ConsensusError::ShutdownActive),
        Err(err) => {
            eprintln!("model existence check failed: {}", err);
            return Ok(false);
        }
    }

    let inputs: Vec<InferenceTensor> = bincode::deserialize(input)?;
    if inputs.is_empty() {
        return Ok(false);
    }
    let expected: Vec<InferenceOutput> = bincode::deserialize(output)?;
    if expected.is_empty() {
        return Ok(false);
    }

    let inputs_for_inference = inputs.clone();

    let cache = inference_verification_cache();
    let epsilon = inference_verification_config().epsilon;
    let model_id = metadata.model_id.clone();
    let outputs = block_on_verification(move || async move {
        deterministic_inference(&model_id, inputs_for_inference, Some(cache)).await
    });

    match outputs {
        Ok(outputs) => Ok(outputs_match(&expected, &outputs, epsilon)),
        Err(VerificationError::Shutdown) => Err(ConsensusError::ShutdownActive),
        Err(VerificationError::EngineUnavailable)
        | Err(VerificationError::Inference(_))
        | Err(VerificationError::Model(_)) => {
            warn!("falling back to structural inference verification: {outputs:?}");
            Ok(fallback_verify_inference(&inputs, &expected, metadata))
        }
        Err(err) => Err(ConsensusError::VerificationError(err.to_string())),
    }
}

fn fallback_verify_inference(
    inputs: &[InferenceTensor],
    expected: &[InferenceOutput],
    metadata: &ComputationMetadata,
) -> bool {
    if inputs.is_empty() || expected.is_empty() {
        return false;
    }

    for input in inputs {
        if input.name.trim().is_empty() {
            return false;
        }
        if input.shape.is_empty() {
            return false;
        }
        let mut elem_count = 1i64;
        for dim in &input.shape {
            if *dim <= 0 {
                return false;
            }
            elem_count = match elem_count.checked_mul(*dim) {
                Some(value) => value,
                None => return false,
            };
        }
        if input.data.len() as i64 != elem_count {
            return false;
        }
        if input.data.iter().any(|value| !value.is_finite()) {
            return false;
        }
    }

    for output in expected {
        if output.name.trim().is_empty() {
            return false;
        }
        if output.shape.is_empty() {
            return false;
        }
        let mut elem_count = 1i64;
        for dim in &output.shape {
            if *dim <= 0 {
                return false;
            }
            elem_count = match elem_count.checked_mul(*dim) {
                Some(value) => value,
                None => return false,
            };
        }
        if elem_count <= 0 {
            return false;
        }
        if output.data.len() as i64 != elem_count {
            return false;
        }
        if output.data.iter().any(|value| !value.is_finite()) {
            return false;
        }
    }

    if metadata.model_id.trim().is_empty() {
        return false;
    }

    true
}

fn block_on_verification<Fut, MakeFut>(make_future: MakeFut) -> Result<Vec<InferenceOutput>, VerificationError>
where
    MakeFut: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<Vec<InferenceOutput>, VerificationError>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let can_block_in_place = std::panic::catch_unwind(|| {
            task::block_in_place(|| {});
        })
        .is_ok();

        if can_block_in_place {
            return task::block_in_place(|| handle.block_on(make_future()));
        }

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = match Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(make_future()),
                Err(err) => Err(VerificationError::Serialization(err.to_string())),
            };
            let _ = tx.send(result);
        });

        return rx.recv().map_err(|err| VerificationError::Serialization(err.to_string()))?;
    }

    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| VerificationError::Serialization(err.to_string()))?;
    runtime.block_on(make_future())
}

pub fn compress_gradients(gradients: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(gradients.len());
    for &g in gradients {
        let clamped = g.max(-1.0).min(1.0);
        let q = ((clamped + 1.0) * 127.5).round() as i32;
        out.push(q.max(0).min(255) as u8);
    }
    out
}

pub fn decompress_gradients(compressed: &[u8]) -> Vec<f32> {
    compressed
        .iter()
        .map(|&b| (b as f32 / 127.5) - 1.0)
        .collect()
}

pub fn compress_gradients_4bit(gradients: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity((gradients.len() + 1) / 2);
    let mut i = 0;
    while i < gradients.len() {
        let g0 = gradients[i].max(-1.0).min(1.0);
        let q0 = (((g0 + 1.0) * 7.5).round() as i32).max(0).min(15) as u8;
        let q1 = if i + 1 < gradients.len() {
            let g1 = gradients[i + 1].max(-1.0).min(1.0);
            (((g1 + 1.0) * 7.5).round() as i32).max(0).min(15) as u8
        } else {
            0
        };
        out.push((q0 << 4) | (q1 & 0x0f));
        i += 2;
    }
    out
}

pub fn decompress_gradients_4bit(compressed: &[u8], target_len: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(compressed.len() * 2);
    for &b in compressed {
        let hi = (b >> 4) & 0x0f;
        let lo = b & 0x0f;
        let f0 = (hi as f32 / 7.5) - 1.0;
        let f1 = (lo as f32 / 7.5) - 1.0;
        out.push(f0);
        out.push(f1);
    }

    if target_len > 0 && out.len() > target_len {
        out.truncate(target_len);
    }
    out
}

pub fn calculate_poi_reward(proof: &PoIProof) -> Amount {
    let base = 100u64;
    let factor = (proof.difficulty / 1000).max(1);
    let mut reward = base.saturating_mul(factor);

    match proof.work_type {
        WorkType::GradientDescent => {
            reward = (reward as u128 * 120 / 100) as u64;
        }
        WorkType::Inference => {
            reward = (reward as u128 * 110 / 100) as u64;
        }
        WorkType::MatrixMultiplication => {}
    }

    Amount::new(reward)
}

pub struct PoIBlockProducer {
    pub committee_size: usize,
}

impl Default for PoIBlockProducer {
    fn default() -> Self {
        Self { committee_size: 21 }
    }
}

impl PoIBlockProducer {
    pub fn produce_block(
        &self,
        previous_hash: BlockHash,
        transactions: Vec<Transaction>,
        block_height: u64,
        poi_proof: PoIProof,
        miner_address: String,
        committee_addrs: Vec<String>,
    ) -> Result<Block, ConsensusError> {
        check_shutdown_and_halt()?;

        if block_height == 0 {
            return Err(ConsensusError::InvalidProof);
        }

        if miner_address != poi_proof.miner_address {
            return Err(ConsensusError::InvalidProof);
        }

        poi_proof.verify()?;

        let mut proof_for_block = poi_proof.clone();
        proof_for_block.verification_data["committee"] = serde_json::json!(committee_addrs);

        let proof_bytes = serde_json::to_vec(&proof_for_block)?;

        Ok(Block::new(
            previous_hash.0,
            transactions,
            block_height,
            1,
            Some(proof_bytes),
        ))
    }
}

pub fn check_shutdown_and_halt() -> Result<(), ConsensusError> {
    check_shutdown().map_err(|_| ConsensusError::ShutdownActive)?;
    Ok(())
}

pub fn is_shutdown_active() -> bool {
    is_shutdown()
}
