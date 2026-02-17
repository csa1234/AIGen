// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Safety Oracle Ensemble for multi-provider AI safety verification.
//!
//! This module implements a Safety Oracle that queries three external AI providers
//! (Mistral, Anthropic Claude, OpenAI GPT-4o) with a constitutional prompt template.
//! All three must vote "safe" for approval; any "unsafe" vote blocks deployment.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tokio::time::timeout;

/// Configuration for the Safety Oracle Ensemble.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyOracleConfig {
    /// Whether the Safety Oracle is enabled
    pub enabled: bool,
    /// Mistral API key
    pub mistral_api_key: String,
    /// Anthropic API key
    pub anthropic_api_key: String,
    /// OpenAI API key
    pub openai_api_key: String,
    /// Timeout for API calls in milliseconds (default: 10000)
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Prompt template for safety evaluation
    #[serde(default = "default_prompt_template")]
    pub prompt_template: String,
}

fn default_timeout_ms() -> u64 {
    10000
}

fn default_prompt_template() -> String {
    // Default prompt template - can be overridden via config file
    include_str!("../data/safety_prompt_template.txt").to_string()
}

impl Default for SafetyOracleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mistral_api_key: String::new(),
            anthropic_api_key: String::new(),
            openai_api_key: String::new(),
            timeout_ms: default_timeout_ms(),
            prompt_template: default_prompt_template(),
        }
    }
}

impl SafetyOracleConfig {
    /// Load configuration from a JSON file.
    pub fn from_file(path: &str) -> Result<Self, SafetyOracleError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SafetyOracleError::ConfigError(format!("Failed to read config file: {}", e)))?;
        let config: Self = serde_json::from_str(&content)
            .map_err(|e| SafetyOracleError::ConfigError(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Load configuration from environment variable or default path.
    pub fn load() -> Result<Self, SafetyOracleError> {
        // Try environment variable first
        if let Ok(config_path) = std::env::var("AIGEN_SAFETY_ORACLE_CONFIG") {
            return Self::from_file(&config_path);
        }
        
        // Try default path
        let default_path = "data/safety_oracle_config.json";
        if std::path::Path::new(default_path).exists() {
            return Self::from_file(default_path);
        }
        
        Err(SafetyOracleError::ConfigError(
            "No Safety Oracle configuration found. Set AIGEN_SAFETY_ORACLE_CONFIG env var or create data/safety_oracle_config.json".to_string()
        ))
    }
}

/// Safety vote from a single provider.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SafetyVote {
    Safe,
    Unsafe { reason: String },
}

impl std::fmt::Display for SafetyVote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyVote::Safe => write!(f, "SAFE"),
            SafetyVote::Unsafe { reason } => write!(f, "UNSAFE: {}", reason),
        }
    }
}

/// Result from the Safety Oracle Ensemble evaluation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyOracleResult {
    /// Vote from Mistral
    pub mistral_vote: SafetyVote,
    /// Vote from Anthropic Claude
    pub anthropic_vote: SafetyVote,
    /// Vote from OpenAI GPT-4o
    pub openai_vote: SafetyVote,
    /// Whether all providers voted safe
    pub unanimous_safe: bool,
    /// Unix timestamp of the evaluation
    pub timestamp: i64,
}

/// Errors that can occur during Safety Oracle operations.
#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum SafetyOracleError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("API error from {provider}: {message}")]
    ApiError { provider: String, message: String },
    
    #[error("Timeout from {provider}")]
    TimeoutError { provider: String },
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("All providers failed")]
    AllProvidersFailed,
}

/// Global singleton for Safety Oracle configuration.
static SAFETY_ORACLE_CONFIG: Lazy<Mutex<Option<SafetyOracleConfig>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the Safety Oracle with the given configuration.
pub fn initialize_safety_oracle(config: SafetyOracleConfig) {
    let mut global_config = SAFETY_ORACLE_CONFIG.lock();
    *global_config = Some(config);
}

/// Get the current Safety Oracle configuration.
pub fn get_safety_oracle_config() -> Result<SafetyOracleConfig, SafetyOracleError> {
    let global_config = SAFETY_ORACLE_CONFIG.lock();
    global_config
        .clone()
        .ok_or_else(|| SafetyOracleError::ConfigError("Safety Oracle not initialized".to_string()))
}

/// Response structure for parsing API responses.
#[derive(Debug, Deserialize)]
struct SafetyResponse {
    verdict: String,
    reason: Option<String>,
}

/// Mistral API response structure.
#[derive(Debug, Deserialize)]
struct MistralResponse {
    choices: Vec<MistralChoice>,
}

#[derive(Debug, Deserialize)]
struct MistralChoice {
    message: MistralMessage,
}

#[derive(Debug, Deserialize)]
struct MistralMessage {
    content: String,
}

/// Anthropic API response structure.
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}

/// OpenAI API response structure.
#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

/// Build the safety evaluation prompt from template and model output.
fn build_prompt(model_output: &str, config: &SafetyOracleConfig) -> String {
    config
        .prompt_template
        .replace("{output}", model_output)
}

/// Parse a safety verdict from API response text.
fn parse_verdict(response_text: &str) -> Result<SafetyVote, SafetyOracleError> {
    // Try to parse as JSON first
    if let Ok(parsed) = serde_json::from_str::<SafetyResponse>(response_text) {
        let verdict = parsed.verdict.to_uppercase();
        return if verdict == "SAFE" {
            Ok(SafetyVote::Safe)
        } else {
            Ok(SafetyVote::Unsafe {
                reason: parsed.reason.unwrap_or_else(|| "No reason provided".to_string()),
            })
        };
    }
    
    // Try to extract from text response
    let upper = response_text.to_uppercase();
    if upper.contains("SAFE") && !upper.contains("UNSAFE") {
        Ok(SafetyVote::Safe)
    } else if upper.contains("UNSAFE") {
        // Try to extract reason
        let reason = extract_reason(response_text);
        Ok(SafetyVote::Unsafe { reason })
    } else {
        // Default to unsafe if unclear
        Ok(SafetyVote::Unsafe {
            reason: format!("Unclear response: {}", response_text.chars().take(200).collect::<String>()),
        })
    }
}

/// Extract reason from response text.
fn extract_reason(text: &str) -> String {
    // Look for common patterns
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find("reason") {
        let rest = &text[pos..];
        if let Some(colon_pos) = rest.find(':') {
            let reason = rest[colon_pos + 1..].trim();
            return reason.chars().take(500).collect();
        }
    }
    "No specific reason provided".to_string()
}

/// Query Mistral API for safety evaluation.
pub async fn query_mistral(
    client: &Client,
    config: &SafetyOracleConfig,
    prompt: &str,
) -> Result<SafetyVote, SafetyOracleError> {
    let url = "https://api.mistral.ai/v1/chat/completions";
    
    let body = serde_json::json!({
        "model": "mistral-large-latest",
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ]
    });
    
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", config.mistral_api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| SafetyOracleError::ApiError {
            provider: "Mistral".to_string(),
            message: e.to_string(),
        })?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(SafetyOracleError::ApiError {
            provider: "Mistral".to_string(),
            message: format!("HTTP {}: {}", status, body),
        });
    }
    
    let mistral_response: MistralResponse = response
        .json()
        .await
        .map_err(|e| SafetyOracleError::ParseError(format!("Mistral response: {}", e)))?;
    
    let content = mistral_response
        .choices
        .first()
        .map(|c| c.message.content.as_str())
        .ok_or_else(|| SafetyOracleError::ParseError("No content in Mistral response".to_string()))?;
    
    parse_verdict(content)
}

/// Query Anthropic Claude API for safety evaluation.
pub async fn query_anthropic(
    client: &Client,
    config: &SafetyOracleConfig,
    prompt: &str,
) -> Result<SafetyVote, SafetyOracleError> {
    let url = "https://api.anthropic.com/v1/messages";
    
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet-20241022",
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ]
    });
    
    let response = client
        .post(url)
        .header("x-api-key", &config.anthropic_api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| SafetyOracleError::ApiError {
            provider: "Anthropic".to_string(),
            message: e.to_string(),
        })?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(SafetyOracleError::ApiError {
            provider: "Anthropic".to_string(),
            message: format!("HTTP {}: {}", status, body),
        });
    }
    
    let anthropic_response: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| SafetyOracleError::ParseError(format!("Anthropic response: {}", e)))?;
    
    let content = anthropic_response
        .content
        .first()
        .map(|c| c.text.as_str())
        .ok_or_else(|| SafetyOracleError::ParseError("No content in Anthropic response".to_string()))?;
    
    parse_verdict(content)
}

/// Query OpenAI GPT-4o API for safety evaluation.
pub async fn query_openai(
    client: &Client,
    config: &SafetyOracleConfig,
    prompt: &str,
) -> Result<SafetyVote, SafetyOracleError> {
    let url = "https://api.openai.com/v1/chat/completions";
    
    let body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ]
    });
    
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", config.openai_api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| SafetyOracleError::ApiError {
            provider: "OpenAI".to_string(),
            message: e.to_string(),
        })?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(SafetyOracleError::ApiError {
            provider: "OpenAI".to_string(),
            message: format!("HTTP {}: {}", status, body),
        });
    }
    
    let openai_response: OpenAiResponse = response
        .json()
        .await
        .map_err(|e| SafetyOracleError::ParseError(format!("OpenAI response: {}", e)))?;
    
    let content = openai_response
        .choices
        .first()
        .map(|c| c.message.content.as_str())
        .ok_or_else(|| SafetyOracleError::ParseError("No content in OpenAI response".to_string()))?;
    
    parse_verdict(content)
}

/// Evaluate safety using the Safety Oracle Ensemble.
///
/// This function queries all three AI providers concurrently and requires
/// unanimous "SAFE" votes for approval. Any "UNSAFE" vote or error results
/// in rejection (fail-closed behavior).
pub async fn evaluate_safety(
    model_output: &str,
    config: &SafetyOracleConfig,
) -> Result<SafetyOracleResult, SafetyOracleError> {
    let prompt = build_prompt(model_output, config);
    let timeout_duration = Duration::from_millis(config.timeout_ms);
    
    let client = Client::builder()
        .timeout(timeout_duration)
        .build()
        .map_err(|e| SafetyOracleError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;
    
    // Execute all three API calls concurrently with timeouts
    let mistral_future = timeout(
        timeout_duration,
        query_mistral(&client, config, &prompt),
    );
    let anthropic_future = timeout(
        timeout_duration,
        query_anthropic(&client, config, &prompt),
    );
    let openai_future = timeout(
        timeout_duration,
        query_openai(&client, config, &prompt),
    );
    
    let (mistral_result, anthropic_result, openai_result) =
        tokio::join!(mistral_future, anthropic_future, openai_future);
    
    // Process results with fail-closed behavior
    let mistral_vote = match mistral_result {
        Ok(Ok(vote)) => {
            eprintln!("[Safety Oracle] Mistral vote: {:?}", vote);
            vote
        }
        Ok(Err(e)) => {
            eprintln!("[Safety Oracle] Mistral error: {}", e);
            SafetyVote::Unsafe {
                reason: format!("Provider error: {}", e),
            }
        }
        Err(_) => {
            eprintln!("[Safety Oracle] Mistral timeout");
            SafetyVote::Unsafe {
                reason: "Provider timeout".to_string(),
            }
        }
    };
    
    let anthropic_vote = match anthropic_result {
        Ok(Ok(vote)) => {
            eprintln!("[Safety Oracle] Anthropic vote: {:?}", vote);
            vote
        }
        Ok(Err(e)) => {
            eprintln!("[Safety Oracle] Anthropic error: {}", e);
            SafetyVote::Unsafe {
                reason: format!("Provider error: {}", e),
            }
        }
        Err(_) => {
            eprintln!("[Safety Oracle] Anthropic timeout");
            SafetyVote::Unsafe {
                reason: "Provider timeout".to_string(),
            }
        }
    };
    
    let openai_vote = match openai_result {
        Ok(Ok(vote)) => {
            eprintln!("[Safety Oracle] OpenAI vote: {:?}", vote);
            vote
        }
        Ok(Err(e)) => {
            eprintln!("[Safety Oracle] OpenAI error: {}", e);
            SafetyVote::Unsafe {
                reason: format!("Provider error: {}", e),
            }
        }
        Err(_) => {
            eprintln!("[Safety Oracle] OpenAI timeout");
            SafetyVote::Unsafe {
                reason: "Provider timeout".to_string(),
            }
        }
    };
    
    // Check for unanimous safe
    let unanimous_safe = mistral_vote == SafetyVote::Safe
        && anthropic_vote == SafetyVote::Safe
        && openai_vote == SafetyVote::Safe;
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    let result = SafetyOracleResult {
        mistral_vote,
        anthropic_vote,
        openai_vote,
        unanimous_safe,
        timestamp,
    };
    
    // Log final result
    if unanimous_safe {
        eprintln!("[Safety Oracle] Ensemble result: UNANIMOUS SAFE");
    } else {
        eprintln!("[Safety Oracle] Ensemble result: NOT UNANIMOUS (requires CEO review)");
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_verdict_safe() {
        let response = r#"{"verdict": "SAFE", "reason": null}"#;
        let vote = parse_verdict(response).unwrap();
        assert_eq!(vote, SafetyVote::Safe);
    }
    
    #[test]
    fn test_parse_verdict_unsafe() {
        let response = r#"{"verdict": "UNSAFE", "reason": "Violates principle 42"}"#;
        let vote = parse_verdict(response).unwrap();
        assert_eq!(vote, SafetyVote::Unsafe {
            reason: "Violates principle 42".to_string()
        });
    }
    
    #[test]
    fn test_parse_verdict_text_safe() {
        let response = "The output is SAFE according to all principles.";
        let vote = parse_verdict(response).unwrap();
        assert_eq!(vote, SafetyVote::Safe);
    }
    
    #[test]
    fn test_parse_verdict_text_unsafe() {
        let response = "This output is UNSAFE because it violates ethical guidelines.";
        let vote = parse_verdict(response).unwrap();
        match vote {
            SafetyVote::Unsafe { .. } => {}
            SafetyVote::Safe => panic!("Expected Unsafe vote"),
        }
    }
    
    #[test]
    fn test_build_prompt() {
        let config = SafetyOracleConfig {
            prompt_template: "Evaluate: {output}".to_string(),
            ..Default::default()
        };
        let prompt = build_prompt("test output", &config);
        assert_eq!(prompt, "Evaluate: test output");
    }
    
    #[test]
    fn test_safety_oracle_result_serialization() {
        let result = SafetyOracleResult {
            mistral_vote: SafetyVote::Safe,
            anthropic_vote: SafetyVote::Safe,
            openai_vote: SafetyVote::Unsafe { reason: "Test".to_string() },
            unanimous_safe: false,
            timestamp: 1234567890,
        };
        
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SafetyOracleResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.unanimous_safe, false);
    }
}
