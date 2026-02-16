use std::path::PathBuf;
use std::sync::Arc;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ort::{session::Session, session::builder::GraphOptimizationLevel};
use tokio::time::{timeout, Duration};

/// Core types for benevolence model scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenevolenceScore {
    pub score: f32,
    pub confidence: f32,
    pub timestamp: i64,
    pub model_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenevolenceModelConfig {
    pub enabled: bool,
    pub model_path: PathBuf,
    pub ipfs_cid: Option<String>,
    pub threshold: f32,
    pub max_input_length: usize,
    pub timeout_ms: u64,
}

impl Default for BenevolenceModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model_path: PathBuf::from("aigen-data/models/benevolence/model.onnx"),
            ipfs_cid: None,
            threshold: 0.99,
            max_input_length: 10000,
            timeout_ms: 5000,
        }
    }
}

impl BenevolenceModelConfig {
    pub fn load() -> Self {
        // Try to load from data/benevolence_config.json
        if let Ok(config_str) = std::fs::read_to_string("data/benevolence_config.json") {
            if let Ok(config) = serde_json::from_str(&config_str) {
                return config;
            }
        }

        // Try environment variable AIGEN_BENEVOLENCE_CONFIG
        if let Ok(config_path) = std::env::var("AIGEN_BENEVOLENCE_CONFIG") {
            if let Ok(config_str) = std::fs::read_to_string(config_path) {
                if let Ok(config) = serde_json::from_str(&config_str) {
                    return config;
                }
            }
        }

        // Fall back to defaults
        Self::default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BenevolenceError {
    #[error("Model not loaded")]
    ModelNotLoaded,
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Timeout error")]
    TimeoutError,
    #[error("Threshold not met: {0}")]
    ThresholdNotMet(f32),
}

impl From<ort::Error> for BenevolenceError {
    fn from(err: ort::Error) -> Self {
        BenevolenceError::InferenceError(err.to_string())
    }
}

/// Benevolence model for constitutional compliance scoring
pub struct BenevolenceModel {
    session: Mutex<Option<Session>>,
    config: BenevolenceModelConfig,
    #[allow(dead_code)]
    input_name: String,
    #[allow(dead_code)]
    output_name: String,
    tokenizer_vocab_size: usize,
}

impl BenevolenceModel {
    pub fn new(config: BenevolenceModelConfig) -> Result<Self, BenevolenceError> {
        Ok(Self {
            session: Mutex::new(None),
            config,
            input_name: "input_ids".to_string(),
            output_name: "benevolence_score".to_string(),
            tokenizer_vocab_size: 30000, // Simple vocab size for initial implementation
        })
    }

    pub fn load_model(&mut self) -> Result<(), BenevolenceError> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(&self.config.model_path)
            .map_err(|e| BenevolenceError::InferenceError(e.to_string()))?;

        *self.session.lock() = Some(session);
        Ok(())
    }

    pub async fn score_output(&self, text: &str) -> Result<BenevolenceScore, BenevolenceError> {
        let input_ids = self.preprocess_text(text);

        // Run inference with timeout
        let score = timeout(
            Duration::from_millis(self.config.timeout_ms),
            self.run_inference(input_ids)
        )
        .await
        .map_err(|_| BenevolenceError::TimeoutError)?
        .map_err(|e| BenevolenceError::InferenceError(e.to_string()))?;

        Ok(BenevolenceScore {
            score,
            confidence: score, // For now, confidence equals score
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model_version: "benevolence_v1".to_string(), // TODO: Get from metadata
        })
    }

    fn preprocess_text(&self, text: &str) -> Vec<i64> {
        // Simple whitespace tokenization for Phase 1
        let mut tokens: Vec<String> = text
            .to_lowercase()
            .split_whitespace()
            .take(self.config.max_input_length)
            .map(|s| s.to_string())
            .collect();

        // Truncate to max length
        tokens.truncate(self.config.max_input_length);

        // Convert to integer IDs using simple hash
        tokens.into_iter()
            .map(|token| {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                token.hash(&mut hasher);
                (hasher.finish() % self.tokenizer_vocab_size as u64) as i64
            })
            .collect()
    }

    async fn run_inference(&self, input_ids: Vec<i64>) -> Result<f32, BenevolenceError> {
        let mut session_guard = self.session.lock();
        let session = session_guard.as_mut().ok_or(BenevolenceError::ModelNotLoaded)?;

        // Create Value from (shape, data)
        let seq_len = input_ids.len();
        let shape = vec![1usize, seq_len];
        let input_value = ort::value::Value::from_array((shape, input_ids))
             .map_err(|e| BenevolenceError::InferenceError(e.to_string()))?;

        // Run inference
        let inputs = ort::inputs![&self.input_name => input_value];

        let outputs = session.run(inputs)
            .map_err(|e| BenevolenceError::InferenceError(e.to_string()))?;

        // Extract score
        let output_tuple = outputs.get(&self.output_name)
            .ok_or_else(|| BenevolenceError::InferenceError(format!("Output {} not found", self.output_name)))?
            .try_extract_tensor::<f32>()
            .map_err(|e| BenevolenceError::InferenceError(e.to_string()))?;

        let score = *output_tuple.1.first()
            .ok_or_else(|| BenevolenceError::InferenceError("Empty output tensor".to_string()))?;

        // Validate range
        if !(0.0..=1.0).contains(&score) {
            return Err(BenevolenceError::InferenceError(format!("Score {} out of range 0.0-1.0", score)));
        }

        Ok(score)
    }

    pub async fn is_compliant(&self, text: &str) -> Result<bool, BenevolenceError> {
        let score = self.score_output(text).await?;
        Ok(score.score >= self.config.threshold)
    }

    pub fn threshold(&self) -> f32 {
        self.config.threshold
    }
}

/// Global singleton for benevolence model
static BENEVOLENCE_MODEL: Lazy<Mutex<Option<Arc<BenevolenceModel>>>> = Lazy::new(|| Mutex::new(None));

pub fn get_benevolence_model() -> Result<Arc<BenevolenceModel>, BenevolenceError> {
    BENEVOLENCE_MODEL.lock()
        .as_ref()
        .cloned()
        .ok_or(BenevolenceError::ModelNotLoaded)
}

pub fn initialize_benevolence_model(config: BenevolenceModelConfig) -> Result<(), BenevolenceError> {
    let mut model = BenevolenceModel::new(config)?;
    model.load_model()?;

    *BENEVOLENCE_MODEL.lock() = Some(Arc::new(model));
    Ok(())
}
