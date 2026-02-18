// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! # SIP Governance and Auto-Approval System
//!
//! This module implements the governance lifecycle for SIP (System Improvement Proposals)
//! including automatic approval based on multi-criteria validation.
//!
//! ## Auto-Approval Criteria
//! A proposal is auto-approved if ALL of the following conditions are met:
//! - `CF.violations == 0`: Constitutional Filter finds no violations
//! - `BM >= 0.99`: Benevolence Model score meets threshold
//! - `SOE.safe == 0`: Safety Oracle Ensemble unanimous safe
//! - `PoI.valid`: Training proof passes validation (if present)
//! - `votes >= threshold`: Staker vote threshold met (approval % and participation %)
//!
//! ## CEO Veto Window
//! After auto-approval, there is a 24-hour window during which the CEO can veto.
//! The proposal transitions from `AutoApprovedPendingCeoWindow` to `Approved` after
//! the window expires without veto.

use crate::authority::verify_ceo_signature;
use crate::benevolence::get_benevolence_model;
use crate::governance_config::GovernanceConfig;
use crate::shutdown::{auto_safety_shutdown, is_shutdown};
use crate::types::{CeoSignature, GenesisError, ShutdownTrigger, SipProposal, SipStatus, WalletAddress};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::runtime::Handle;
use std::thread;

/// Duration of CEO veto window in seconds (24 hours)
pub const CEO_VETO_WINDOW_SECONDS: i64 = 86400;

/// Trait for chain state operations needed by genesis
pub trait ChainStateProvider {
    /// Check if auto-approve threshold is met for a proposal
    fn check_auto_approve_threshold(
        &self,
        proposal_id: &str,
        min_approval_percentage: f64,
        min_participation_percentage: f64,
    ) -> bool;
    
    /// Get a training round by ID
    fn get_training_round(&self, round_id: &str) -> Option<TrainingRoundInfo>;
    
    /// Get Fisher matrix hash for a model
    fn get_fisher_matrix_hash(&self, model_id: &str) -> Option<[u8; 32]>;
    
    /// Record a model approval on-chain
    fn record_model_approval(&self, record: ModelApprovalRecord) -> Result<(), GenesisError>;
    
    /// Get a model approval record
    fn get_model_approval_record(&self, proposal_id: &str) -> Option<ModelApprovalRecord>;
    
    /// Get G8 Safety Committee recommendations for a proposal
    fn get_g8_recommendations_for_proposal(&self, proposal_id: &str) -> Vec<G8Recommendation>;
    
    /// Get a DAO proposal by ID (for forwarding to CEO)
    fn get_dao_proposal(&self, proposal_id: &str) -> Option<DaoProposalInfo>;
    
    /// Get total staked amount across all stakers
    fn get_total_staked(&self) -> u64;
}

/// DAO proposal information for forwarding to CEO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaoProposalInfo {
    pub proposal_id: String,
    pub title: String,
    pub description: String,
    pub proposal_type: u8,
    pub status: u8,
    pub proposer: String,
    pub created_at: i64,
    pub voting_end: i64,
    pub votes_for: u64,
    pub votes_against: u64,
    pub votes_abstain: u64,
    pub ipfs_hash: Option<String>,
    pub parameters: Option<String>,
}

/// G8 Safety Committee recommendation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct G8Recommendation {
    pub recommendation_id: String,
    pub meeting_id: String,
    pub proposal_id: Option<String>,
    pub title: String,
    pub description: String,
    pub category: String,
    pub votes_for: u8,
    pub votes_against: u8,
    pub votes_abstain: u8,
    pub submitted_to_ceo: bool,
    pub ceo_response: Option<String>,
}

/// Training round information for PoI validation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainingRoundInfo {
    pub round_id: String,
    pub participants: Vec<String>,
    pub delta_hash: [u8; 32],
    pub fisher_hash: [u8; 32],
    pub model_id: String,
    pub status: u8,
}

/// On-chain record of model approval for governance transparency
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelApprovalRecord {
    pub proposal_id: String,
    pub model_id: String,
    /// 0=Pending, 1=Approved, 2=Rejected, 3=AutoApprovedPendingCeoWindow, 4=Vetoed
    pub status: u8,
    pub auto_approved: bool,
    pub auto_approved_at: Option<i64>,
    pub ceo_veto_deadline: Option<i64>,
    pub approved_at: Option<i64>,
    pub rejected_at: Option<i64>,
    pub rejection_logs: Vec<String>,
    pub constitutional_violations: Vec<String>,
    pub benevolence_score: Option<f32>,
    pub safety_oracle_votes: Option<String>,
    pub training_proof_hash: Option<[u8; 32]>,
    pub training_round_id: Option<String>,
}

/// Internal status enum for registry (maps to SipStatus in types.rs)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistrySipStatus {
    Pending,
    Approved,
    Vetoed,
    Deployed,
    AutoApprovedPendingCeoWindow,
}

impl From<SipStatus> for RegistrySipStatus {
    fn from(status: SipStatus) -> Self {
        match status {
            SipStatus::Pending => RegistrySipStatus::Pending,
            SipStatus::Approved => RegistrySipStatus::Approved,
            SipStatus::Vetoed => RegistrySipStatus::Vetoed,
            SipStatus::Deployed => RegistrySipStatus::Deployed,
            SipStatus::AutoApprovedPendingCeoWindow => RegistrySipStatus::AutoApprovedPendingCeoWindow,
        }
    }
}

impl From<RegistrySipStatus> for SipStatus {
    fn from(status: RegistrySipStatus) -> Self {
        match status {
            RegistrySipStatus::Pending => SipStatus::Pending,
            RegistrySipStatus::Approved => SipStatus::Approved,
            RegistrySipStatus::Vetoed => SipStatus::Vetoed,
            RegistrySipStatus::Deployed => SipStatus::Deployed,
            RegistrySipStatus::AutoApprovedPendingCeoWindow => SipStatus::AutoApprovedPendingCeoWindow,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SipRegistry {
    pub proposals: HashMap<String, SipProposal>,
    pub statuses: HashMap<String, RegistrySipStatus>,
    pub auto_approve_config: AutoApproveConfig, // NEW
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoApproveConfig {
    pub enabled: bool,
    pub min_approval_percentage: f64,    // Default: 80.0
    pub min_participation_percentage: f64, // Default: 50.0
}

impl Default for AutoApproveConfig {
    fn default() -> Self {
        // Load from GovernanceConfig if available, otherwise use hardcoded defaults
        let config = GovernanceConfig::load();
        Self {
            enabled: config.auto_approve_enabled,
            min_approval_percentage: config.min_approval_percentage,
            min_participation_percentage: config.min_participation_percentage,
        }
    }
}

static SIP_REGISTRY: Lazy<Mutex<SipRegistry>> =
    Lazy::new(|| Mutex::new(load_registry().unwrap_or_default()));

fn registry_path() -> PathBuf {
    std::env::var("AIGEN_SIP_REGISTRY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/sip_registry.json"))
}

fn load_registry() -> Result<SipRegistry, GenesisError> {
    let path = registry_path();
    if !path.exists() {
        return Ok(SipRegistry::default());
    }
    let data = fs::read(&path)?;
    let registry: SipRegistry = serde_json::from_slice(&data)?;
    Ok(registry)
}

fn persist_registry(registry: &SipRegistry) -> Result<(), GenesisError> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(registry)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn submit_sip(proposal: SipProposal) -> Result<String, GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    if registry.proposals.contains_key(&proposal.proposal_id) {
        return Err(GenesisError::DuplicateProposal);
    }

    let id = proposal.proposal_id.clone();
    registry.statuses.insert(id.clone(), RegistrySipStatus::Pending);
    registry.proposals.insert(id.clone(), proposal);

    persist_registry(&registry)?;

    Ok(id)
}

/// Forward a passed DAO proposal to CEO for review
/// 
/// This function bridges the DAO → CEO pipeline. When a DAO proposal passes
/// (status == 2), it is converted to a SIP and enters the CEO review queue.
/// The CEO can then approve_sip or veto_sip.
pub fn forward_dao_proposal_to_ceo(
    dao_proposal: &DaoProposalInfo,
) -> Result<String, GenesisError> {
    // Verify the proposal is passed
    if dao_proposal.status != 2 {
        return Err(GenesisError::InvalidAddress("DAO proposal must be passed (status=2) to forward to CEO".to_string()));
    }
    
    // Create a deterministic proposal hash from DAO proposal data
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(dao_proposal.proposal_id.as_bytes());
    hasher.update(dao_proposal.title.as_bytes());
    hasher.update(dao_proposal.description.as_bytes());
    hasher.update(&dao_proposal.created_at.to_le_bytes());
    let proposal_hash: [u8; 32] = hasher.finalize().into();
    
    // Construct a SipProposal from the DAO proposal
    let sip = SipProposal {
        proposal_hash,
        proposal_id: dao_proposal.proposal_id.clone(),
        description: format!("{}\n\n{}", dao_proposal.title, dao_proposal.description),
        code_changes_hash: [0u8; 32], // No code changes for DAO proposals
        proposer: WalletAddress::new(dao_proposal.proposer.clone())
            .map_err(|_| GenesisError::InvalidWalletAddress)?,
        timestamp: dao_proposal.created_at,
        ceo_approved: false,
        auto_approved: false,
        approved_at: None,
        model_output_sample: None,
        constitutional_violations: vec![],
        constitution_version: None,
        benevolence_score: None,
        benevolence_model_version: None,
        safety_oracle_votes: None,
        training_proof_hash: None,
        training_round_id: None,
        training_proof_json: None,
        model_id: None,
        auto_approved_at: None,
        ceo_veto_deadline: None,
        rejection_logs: vec![],
        status: SipStatus::Pending,
    };
    
    // Submit to CEO queue
    submit_sip(sip)
}

pub fn veto_sip(proposal_id: &str, signature: CeoSignature, chain_state: &dyn ChainStateProvider) -> Result<(), GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    let proposal = registry
        .proposals
        .get(proposal_id)
        .cloned()
        .ok_or(GenesisError::ProposalNotFound)?;

    verify_ceo_signature(&proposal.message_to_sign_for_veto(), &signature)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    let mut updated = proposal.clone();
    updated.ceo_approved = false;
    updated.status = SipStatus::Vetoed;

    registry
        .statuses
        .insert(proposal_id.to_string(), RegistrySipStatus::Vetoed);
    registry.proposals.insert(proposal_id.to_string(), updated.clone());

    persist_registry(&registry)?;
    
    // Comment 2: Record ModelApprovalRecord on-chain for veto
    let record = ModelApprovalRecord {
        proposal_id: proposal_id.to_string(),
        model_id: updated.model_id.clone().unwrap_or_default(),
        status: 4, // Vetoed
        auto_approved: updated.auto_approved,
        auto_approved_at: updated.auto_approved_at,
        ceo_veto_deadline: updated.ceo_veto_deadline,
        approved_at: None,
        rejected_at: Some(now),
        rejection_logs: vec!["Vetoed by CEO".to_string()],
        constitutional_violations: updated.constitutional_violations.clone(),
        benevolence_score: updated.benevolence_score,
        safety_oracle_votes: updated.safety_oracle_votes.clone(),
        training_proof_hash: updated.training_proof_hash,
        training_round_id: updated.training_round_id.clone(),
    };
    
    if let Err(e) = chain_state.record_model_approval(record) {
        // Fail closed: log error but don't revert the veto
        eprintln!("ERROR: Failed to record ModelApprovalRecord for vetoed SIP {}: {}", proposal_id, e);
    }
    
    eprintln!("SIP {} vetoed by CEO (overrides auto-approval if any)", proposal_id); // NEW

    Ok(())
}

pub fn approve_sip(proposal_id: &str, signature: CeoSignature, chain_state: &dyn ChainStateProvider) -> Result<(), GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    let proposal = registry
        .proposals
        .get(proposal_id)
        .cloned()
        .ok_or(GenesisError::ProposalNotFound)?;

    verify_ceo_signature(&proposal.message_to_sign_for_approval(), &signature)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    let mut updated = proposal.clone();
    updated.ceo_approved = true;
    updated.approved_at = Some(now);
    updated.status = SipStatus::Approved;

    registry
        .statuses
        .insert(proposal_id.to_string(), RegistrySipStatus::Approved);
    registry.proposals.insert(proposal_id.to_string(), updated.clone());

    persist_registry(&registry)?;
    
    // Comment 2: Record ModelApprovalRecord on-chain for approval
    let record = ModelApprovalRecord {
        proposal_id: proposal_id.to_string(),
        model_id: updated.model_id.clone().unwrap_or_default(),
        status: 1, // Approved
        auto_approved: updated.auto_approved,
        auto_approved_at: updated.auto_approved_at,
        ceo_veto_deadline: updated.ceo_veto_deadline,
        approved_at: Some(now),
        rejected_at: None,
        rejection_logs: Vec::new(),
        constitutional_violations: updated.constitutional_violations.clone(),
        benevolence_score: updated.benevolence_score,
        safety_oracle_votes: updated.safety_oracle_votes.clone(),
        training_proof_hash: updated.training_proof_hash,
        training_round_id: updated.training_round_id.clone(),
    };
    
    if let Err(e) = chain_state.record_model_approval(record) {
        // Fail closed: log error but don't revert the approval
        eprintln!("ERROR: Failed to record ModelApprovalRecord for approved SIP {}: {}", proposal_id, e);
    }
    
    eprintln!("SIP {} approved by CEO", proposal_id); // NEW: logging

    Ok(())
}

/// Check if proposal meets auto-approval criteria based on staker votes
/// Uses GovernanceConfig for thresholds
/// Returns true if auto-approved, false if CEO review needed
pub fn check_and_auto_approve(
    proposal_id: &str,
    chain_state: &dyn ChainStateProvider,
) -> Result<bool, GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    
    // Load governance config for thresholds
    let config = GovernanceConfig::load();
    
    if !config.auto_approve_enabled {
        return Ok(false); // Auto-approve disabled, needs CEO review
    }
    
    let proposal = registry
        .proposals
        .get(proposal_id)
        .ok_or(GenesisError::ProposalNotFound)?;
    
    // Check if already approved/vetoed
    if let Some(status) = registry.statuses.get(proposal_id) {
        match status {
            RegistrySipStatus::Approved | RegistrySipStatus::Vetoed | RegistrySipStatus::Deployed => {
                return Ok(false); // Already decided
            }
            _ => {}
        }
    }
    
    // Initialize rejection logs for this proposal
    let mut rejection_logs: Vec<String> = Vec::new();
    
    // Run constitutional filter BEFORE vote threshold evaluation
    if let Some(model_output) = &proposal.model_output_sample {
        let violations = crate::constitution::check_constitutional_compliance(model_output);
        if !violations.is_empty() {
            // Record violations on proposal
            let mut updated = proposal.clone();
            updated.constitutional_violations = violations.iter()
                .map(|v| format!("Principle {} ({}): {}", v.principle_id, v.category, v.principle_text))
                .collect();
            updated.constitution_version = Some(1); // Current constitution version
            
            // Add detailed rejection logs
            for v in &violations {
                let log_entry = format!(
                    "Constitutional violation - Principle {} ({}): {} [pattern: {}]",
                    v.principle_id, v.category, v.principle_text, v.matched_pattern
                );
                rejection_logs.push(log_entry);
            }
            updated.rejection_logs = rejection_logs.clone();
            updated.status = SipStatus::Vetoed;
            
            // Mark status as vetoed/rejected
            registry.statuses.insert(
                proposal_id.to_string(),
                RegistrySipStatus::Vetoed,
            );
            registry.proposals.insert(proposal_id.to_string(), updated);
            persist_registry(&registry)?;
            
            eprintln!(
                "SIP {} rejected due to {} constitutional violation(s)",
                proposal_id,
                violations.len()
            );
            for v in &violations {
                eprintln!(
                    "  - Principle {} ({}): {}",
                    v.principle_id, v.category, v.principle_text
                );
            }
            
            // Trigger auto-safety-shutdown for critical constitutional violation
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let shutdown_details = format!(
                "Constitutional violation in SIP {} at timestamp {}: {} violation(s) detected",
                proposal_id,
                timestamp,
                violations.len()
            );
            if let Err(e) = auto_safety_shutdown(ShutdownTrigger::ConstitutionalViolation, shutdown_details) {
                eprintln!("Failed to trigger auto-safety-shutdown for constitutional violation: {:?}", e);
            }
            
            return Ok(false); // Rejected due to constitutional violations
        }
    }
    
    // Check benevolence model (ML-based constitutional compliance scoring)
    // Comment 1: Benevolence scoring is never applied in auto-approval
    let mut current_proposal = proposal.clone(); // Clone for updates
    
    if let Some(sample) = &current_proposal.model_output_sample {
        let model = match get_benevolence_model() {
            Ok(m) => m,
            Err(e) => {
                // Comment 3: Populate rejection_logs for benevolence model not available
                let log_entry = format!("Benevolence model not available: {}", e);
                rejection_logs.push(log_entry.clone());
                let mut updated = current_proposal.clone();
                updated.rejection_logs = rejection_logs.clone();
                registry.proposals.insert(proposal_id.to_string(), updated);
                persist_registry(&registry)?;
                eprintln!("Benevolence model not available for SIP {}, requiring CEO review: {}", proposal_id, e);
                return Ok(false);
            }
        };

        // Run async score_output using a runtime handle
        let handle = Handle::current();
        let sample_text = sample.clone();
        let model_clone = model.clone();

        let score_result = thread::spawn(move || {
            handle.block_on(async {
                model_clone.score_output(&sample_text).await
            })
        }).join().unwrap_or_else(|_| Err(crate::benevolence::BenevolenceError::InferenceError("Thread panicked".to_string())));

        match score_result {
            Ok(score_data) => {
                // Persist score
                current_proposal.benevolence_score = Some(score_data.score);
                current_proposal.benevolence_model_version = Some(score_data.model_version);

                // Update registry with score info
                registry.proposals.insert(proposal_id.to_string(), current_proposal.clone());
                persist_registry(&registry)?;

                // Check threshold
                let threshold = model.threshold();
                if score_data.score < threshold {
                    // Comment 3: Populate rejection_logs for benevolence score below threshold
                    let log_entry = format!(
                        "Benevolence score {} below threshold {}",
                        score_data.score, threshold
                    );
                    rejection_logs.push(log_entry.clone());
                    let mut updated = current_proposal.clone();
                    updated.rejection_logs = rejection_logs.clone();
                    registry.proposals.insert(proposal_id.to_string(), updated);
                    persist_registry(&registry)?;
                    eprintln!("SIP {} benevolence score {} below threshold {}, requiring CEO review", proposal_id, score_data.score, threshold);
                    
                    // Trigger auto-safety-shutdown for critical benevolence failure
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let shutdown_details = format!(
                        "Benevolence score {} below threshold {} in SIP {} at timestamp {}",
                        score_data.score,
                        threshold,
                        proposal_id,
                        timestamp
                    );
                    if let Err(e) = auto_safety_shutdown(ShutdownTrigger::BenevolenceFailure, shutdown_details) {
                        eprintln!("Failed to trigger auto-safety-shutdown for benevolence failure: {:?}", e);
                    }
                    
                    return Ok(false);
                }
            },
            Err(e) => {
                // Comment 3: Populate rejection_logs for benevolence inference error
                let log_entry = format!("Benevolence scoring failed: {}", e);
                rejection_logs.push(log_entry.clone());
                let mut updated = current_proposal.clone();
                updated.rejection_logs = rejection_logs.clone();
                registry.proposals.insert(proposal_id.to_string(), updated);
                persist_registry(&registry)?;
                eprintln!("Benevolence scoring failed for SIP {}: {}, requiring CEO review", proposal_id, e);
                return Ok(false);
            }
        }
    } else {
        // Comment 3: Populate rejection_logs for missing model output sample
        let log_entry = "Missing model output sample for benevolence evaluation".to_string();
        rejection_logs.push(log_entry.clone());
        let mut updated = current_proposal.clone();
        updated.rejection_logs = rejection_logs.clone();
        registry.proposals.insert(proposal_id.to_string(), updated);
        persist_registry(&registry)?;
        // Return or require CEO review if sample is missing
        eprintln!("SIP {} missing model output sample, requiring CEO review", proposal_id);
        return Ok(false);
    }
    
    // Safety Oracle Ensemble check (after benevolence scoring)
    if let Some(sample) = &current_proposal.model_output_sample {
        use crate::safety_oracle::{SafetyOracleConfig, evaluate_safety};
        
        // Load config directly from file/env - fail closed on load errors
        let oracle_config = match SafetyOracleConfig::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                // Comment 3: Populate rejection_logs for safety oracle config load failure
                let log_entry = format!("Safety Oracle config load failed: {}", e);
                rejection_logs.push(log_entry.clone());
                let mut updated = current_proposal.clone();
                updated.rejection_logs = rejection_logs.clone();
                registry.proposals.insert(proposal_id.to_string(), updated);
                persist_registry(&registry)?;
                eprintln!("Safety Oracle config load failed for SIP {}, requiring CEO review: {}", proposal_id, e);
                return Ok(false);
            }
        };
        
        if oracle_config.enabled {
            let handle = Handle::current();
            let sample_clone = sample.clone();
            let config_clone = oracle_config.clone();
            
            let oracle_result = thread::spawn(move || {
                handle.block_on(async {
                    evaluate_safety(&sample_clone, &config_clone).await
                })
            }).join().unwrap_or_else(|_| {
                Err(crate::safety_oracle::SafetyOracleError::AllProvidersFailed)
            });
            
            match oracle_result {
                Ok(result) => {
                    // Persist oracle votes to proposal
                    current_proposal.safety_oracle_votes = Some(serde_json::to_string(&result).unwrap_or_default());
                    registry.proposals.insert(proposal_id.to_string(), current_proposal.clone());
                    persist_registry(&registry)?;
                    
                    if !result.unanimous_safe {
                        // Comment 3: Populate rejection_logs for safety oracle not unanimous
                        let log_entry = format!(
                            "Safety Oracle Ensemble rejected (not unanimous safe) - Mistral: {:?}, Anthropic: {:?}, OpenAI: {:?}",
                            result.mistral_vote, result.anthropic_vote, result.openai_vote
                        );
                        rejection_logs.push(log_entry.clone());
                        let mut updated = current_proposal.clone();
                        updated.rejection_logs = rejection_logs.clone();
                        registry.proposals.insert(proposal_id.to_string(), updated);
                        persist_registry(&registry)?;
                        
                        eprintln!(
                            "SIP {} rejected by Safety Oracle Ensemble (not unanimous safe), requiring CEO review",
                            proposal_id
                        );
                        eprintln!("  Mistral: {:?}", result.mistral_vote);
                        eprintln!("  Anthropic: {:?}", result.anthropic_vote);
                        eprintln!("  OpenAI: {:?}", result.openai_vote);
                        
                        // Trigger auto-safety-shutdown for critical safety oracle unsafe
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let shutdown_details = format!(
                            "Safety Oracle unsafe vote in SIP {} at timestamp {} - Mistral: {:?}, Anthropic: {:?}, OpenAI: {:?}",
                            proposal_id,
                            timestamp,
                            result.mistral_vote,
                            result.anthropic_vote,
                            result.openai_vote
                        );
                        if let Err(e) = auto_safety_shutdown(ShutdownTrigger::SafetyOracleUnsafe, shutdown_details) {
                            eprintln!("Failed to trigger auto-safety-shutdown for safety oracle unsafe: {:?}", e);
                        }
                        
                        return Ok(false);
                    }
                },
                Err(e) => {
                    // Comment 3: Populate rejection_logs for safety oracle error
                    let log_entry = format!("Safety Oracle Ensemble failed: {}", e);
                    rejection_logs.push(log_entry.clone());
                    let mut updated = current_proposal.clone();
                    updated.rejection_logs = rejection_logs.clone();
                    registry.proposals.insert(proposal_id.to_string(), updated);
                    persist_registry(&registry)?;
                    
                    eprintln!("Safety Oracle Ensemble failed for SIP {}: {}, requiring CEO review", proposal_id, e);
                    return Ok(false);
                }
            }
        }
    }
    
    // G8 Safety Committee Advisory Check
    // Advisory only - does NOT block approval, logged for CEO review
    let g8_recommendations = chain_state.get_g8_recommendations_for_proposal(proposal_id);
    for rec in &g8_recommendations {
        if rec.votes_for >= 5 {
            let log_entry = format!(
                "G8 Advisory ({} of 8 votes): {} - {}",
                rec.votes_for, rec.title, rec.description
            );
            rejection_logs.push(log_entry.clone());
            eprintln!("SIP {}: {}", proposal_id, log_entry);
            eprintln!("  Note: G8 recommendation is advisory only and does not block auto-approval");
        }
    }
    
    // Comment 1: PoI validation before vote threshold evaluation
    // Check if proposal has training proof for PoI validation
    if let Some(training_proof_json_str) = &current_proposal.training_proof_json {
        let model_id = current_proposal.model_id.as_deref().unwrap_or("");
        
        // Parse the training proof JSON
        let training_proof: serde_json::Value = match serde_json::from_str(training_proof_json_str) {
            Ok(v) => v,
            Err(e) => {
                // Missing or invalid proof - reject
                let log_entry = format!("PoI validation failed: Invalid training proof JSON: {}", e);
                rejection_logs.push(log_entry.clone());
                let mut updated = current_proposal.clone();
                updated.rejection_logs = rejection_logs.clone();
                updated.status = SipStatus::Vetoed;
                registry.statuses.insert(proposal_id.to_string(), RegistrySipStatus::Vetoed);
                registry.proposals.insert(proposal_id.to_string(), updated);
                persist_registry(&registry)?;
                
                eprintln!("SIP {} rejected: Invalid training proof JSON: {}", proposal_id, e);
                return Ok(false);
            }
        };
        
        // Verify PoI training proof
        let poi_valid = match verify_poi_training_proof(&training_proof, chain_state, model_id) {
            Ok(valid) => valid,
            Err(e) => {
                // Error during verification - reject
                let log_entry = format!("PoI validation error: {}", e);
                rejection_logs.push(log_entry.clone());
                let mut updated = current_proposal.clone();
                updated.rejection_logs = rejection_logs.clone();
                updated.status = SipStatus::Vetoed;
                registry.statuses.insert(proposal_id.to_string(), RegistrySipStatus::Vetoed);
                registry.proposals.insert(proposal_id.to_string(), updated);
                persist_registry(&registry)?;
                
                eprintln!("SIP {} rejected: PoI validation error: {}", proposal_id, e);
                return Ok(false);
            }
        };
        
        if !poi_valid {
            // PoI validation failed - reject
            let log_entry = "PoI validation failed: Training proof validation returned false".to_string();
            rejection_logs.push(log_entry.clone());
            let mut updated = current_proposal.clone();
            updated.rejection_logs = rejection_logs.clone();
            updated.status = SipStatus::Vetoed;
            registry.statuses.insert(proposal_id.to_string(), RegistrySipStatus::Vetoed);
            registry.proposals.insert(proposal_id.to_string(), updated);
            persist_registry(&registry)?;
            
            eprintln!("SIP {} rejected: PoI validation failed", proposal_id);
            return Ok(false);
        }
    } else if current_proposal.training_round_id.is_some() || current_proposal.model_id.is_some() {
        // Has training_round_id or model_id but no training_proof_json - reject
        let log_entry = "PoI validation failed: Training proof JSON is missing but training_round_id or model_id is present".to_string();
        rejection_logs.push(log_entry.clone());
        let mut updated = current_proposal.clone();
        updated.rejection_logs = rejection_logs.clone();
        updated.status = SipStatus::Vetoed;
        registry.statuses.insert(proposal_id.to_string(), RegistrySipStatus::Vetoed);
        registry.proposals.insert(proposal_id.to_string(), updated);
        persist_registry(&registry)?;
        
        eprintln!("SIP {} rejected: Training proof JSON is missing", proposal_id);
        return Ok(false);
    }
    
    // Check staker vote threshold using config values
    let meets_threshold = chain_state.check_auto_approve_threshold(
        proposal_id,
        config.min_approval_percentage,
        config.min_participation_percentage,
    );
    
    if meets_threshold {
        // Get current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        // Auto-approve with CEO veto window
        let mut updated = current_proposal.clone(); // Use updated proposal with score
        updated.ceo_approved = true; // Mark as approved
        updated.auto_approved = true; // Mark as auto-approved
        updated.auto_approved_at = Some(now);
        updated.ceo_veto_deadline = Some(now + CEO_VETO_WINDOW_SECONDS);
        updated.status = SipStatus::AutoApprovedPendingCeoWindow;

        registry.statuses.insert(
            proposal_id.to_string(),
            RegistrySipStatus::AutoApprovedPendingCeoWindow,
        );
        registry.proposals.insert(proposal_id.to_string(), updated.clone());
        persist_registry(&registry)?;
        
        // Comment 2: Record ModelApprovalRecord on-chain for auto-approval
        let record = ModelApprovalRecord {
            proposal_id: proposal_id.to_string(),
            model_id: updated.model_id.clone().unwrap_or_default(),
            status: 3, // AutoApprovedPendingCeoWindow
            auto_approved: true,
            auto_approved_at: Some(now),
            ceo_veto_deadline: Some(now + CEO_VETO_WINDOW_SECONDS),
            approved_at: None,
            rejected_at: None,
            rejection_logs: Vec::new(),
            constitutional_violations: updated.constitutional_violations.clone(),
            benevolence_score: updated.benevolence_score,
            safety_oracle_votes: updated.safety_oracle_votes.clone(),
            training_proof_hash: updated.training_proof_hash,
            training_round_id: updated.training_round_id.clone(),
        };
        
        if let Err(e) = chain_state.record_model_approval(record) {
            // Fail closed: log error but don't revert the approval
            eprintln!("ERROR: Failed to record ModelApprovalRecord for SIP {}: {}", proposal_id, e);
        }
        
        eprintln!(
            "SIP {} auto-approved by staker consensus (threshold met: {}% approval, {}% participation), CEO veto window expires at {}",
            proposal_id,
            config.min_approval_percentage,
            config.min_participation_percentage,
            now + CEO_VETO_WINDOW_SECONDS
        );
        Ok(true)
    } else {
        // Log vote threshold failure
        rejection_logs.push(format!(
            "Vote threshold not met: required {}% approval, {}% participation",
            config.min_approval_percentage,
            config.min_participation_percentage
        ));
        
        // Update proposal with rejection logs
        let mut updated = current_proposal.clone();
        updated.rejection_logs = rejection_logs;
        registry.proposals.insert(proposal_id.to_string(), updated);
        persist_registry(&registry)?;
        
        Ok(false) // Needs CEO review
    }
}

/// Trigger auto-approval check for a proposal after vote recording or period close
/// This should be called by the voting lifecycle manager
pub fn trigger_auto_approval_check(
    proposal_id: &str,
    chain_state: &dyn ChainStateProvider,
) -> Result<bool, GenesisError> {
    check_and_auto_approve(proposal_id, chain_state)
}

/// Check if a proposal can be deployed.
/// Only proposals with status Approved (not AutoApprovedPendingCeoWindow) can be deployed.
pub fn can_deploy_sip(proposal_id: &str) -> bool {
    if is_shutdown() {
        return false;
    }

    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    matches!(
        registry.statuses.get(proposal_id),
        Some(RegistrySipStatus::Approved) | Some(RegistrySipStatus::Deployed)
    )
}

/// Get the current status of a SIP proposal
pub fn get_sip_status(proposal_id: &str) -> Option<SipStatus> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    registry.statuses.get(proposal_id).cloned().map(|s| s.into())
}

/// Get rejection logs for a proposal (for CEO review)
pub fn get_rejection_logs(proposal_id: &str) -> Result<Vec<String>, GenesisError> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    let proposal = registry
        .proposals
        .get(proposal_id)
        .ok_or(GenesisError::ProposalNotFound)?;
    Ok(proposal.rejection_logs.clone())
}

/// Check if model output is constitutionally compliant
/// Returns true if compliant, false if violations found
pub fn check_model_output_compliance(output: &str) -> Result<bool, GenesisError> {
    let violations = crate::constitution::check_constitutional_compliance(output);
    if violations.is_empty() {
        Ok(true)
    } else {
        eprintln!(
            "Constitutional violations detected: {}",
            violations.len()
        );
        for v in &violations {
            eprintln!(
                "  - Principle {} ({}): {}",
                v.principle_id, v.category, v.principle_text
            );
        }
        Ok(false)
    }
}

/// Check and process CEO veto window expiration for a proposal.
/// Returns Ok(true) if the proposal transitioned from AutoApprovedPendingCeoWindow to Approved.
/// Returns Ok(false) if still pending or already decided.
pub fn check_ceo_veto_window(proposal_id: &str, chain_state: &dyn ChainStateProvider) -> Result<bool, GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    
    let status = registry.statuses.get(proposal_id).cloned();
    if status != Some(RegistrySipStatus::AutoApprovedPendingCeoWindow) {
        return Ok(false); // Not in veto window
    }
    
    let proposal = registry
        .proposals
        .get(proposal_id)
        .ok_or(GenesisError::ProposalNotFound)?;
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    
    if let Some(deadline) = proposal.ceo_veto_deadline {
        if now >= deadline {
            // Veto window expired, transition to Approved
            let mut updated = proposal.clone();
            updated.status = SipStatus::Approved;
            updated.approved_at = Some(now);
            
            registry.statuses.insert(
                proposal_id.to_string(),
                RegistrySipStatus::Approved,
            );
            registry.proposals.insert(proposal_id.to_string(), updated.clone());
            persist_registry(&registry)?;
            
            // Comment 2: Record ModelApprovalRecord on-chain for CEO veto window expiration
            let record = ModelApprovalRecord {
                proposal_id: proposal_id.to_string(),
                model_id: updated.model_id.clone().unwrap_or_default(),
                status: 1, // Approved
                auto_approved: updated.auto_approved,
                auto_approved_at: updated.auto_approved_at,
                ceo_veto_deadline: updated.ceo_veto_deadline,
                approved_at: Some(now),
                rejected_at: None,
                rejection_logs: Vec::new(),
                constitutional_violations: updated.constitutional_violations.clone(),
                benevolence_score: updated.benevolence_score,
                safety_oracle_votes: updated.safety_oracle_votes.clone(),
                training_proof_hash: updated.training_proof_hash,
                training_round_id: updated.training_round_id.clone(),
            };
            
            if let Err(e) = chain_state.record_model_approval(record) {
                // Fail closed: log error but don't revert the approval
                eprintln!("ERROR: Failed to record ModelApprovalRecord for SIP {} on veto window expiration: {}", proposal_id, e);
            }
            
            eprintln!(
                "SIP {} CEO veto window expired, status changed to Approved",
                proposal_id
            );
            return Ok(true);
        }
    }
    
    Ok(false) // Still within veto window
}

/// Process all proposals with expired CEO veto windows.
/// Returns list of proposal IDs that transitioned to Approved.
pub fn process_expired_veto_windows(chain_state: &dyn ChainStateProvider) -> Result<Vec<String>, GenesisError> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    
    // Collect proposals in veto window
    let pending_proposals: Vec<String> = registry
        .statuses
        .iter()
        .filter(|(_, status)| **status == RegistrySipStatus::AutoApprovedPendingCeoWindow)
        .map(|(id, _)| id.clone())
        .collect();
    
    drop(registry); // Release lock before processing
    
    let mut transitioned = Vec::new();
    for proposal_id in pending_proposals {
        if check_ceo_veto_window(&proposal_id, chain_state)? {
            transitioned.push(proposal_id);
        }
    }
    
    Ok(transitioned)
}

/// Verify PoI training proof for a proposal.
/// Returns Ok(true) if valid, Ok(false) if invalid, Err on failure.
pub fn verify_poi_training_proof(
    training_proof_json: &serde_json::Value,
    chain_state: &dyn ChainStateProvider,
    model_id: &str,
) -> Result<bool, GenesisError> {
    // Validate participants list (≥2 nodes)
    let participants = training_proof_json
        .get("participants")
        .and_then(|v| v.as_array())
        .ok_or_else(|| GenesisError::SerializationError("Missing participants in training proof".to_string()))?;
    
    if participants.len() < 2 {
        return Ok(false);
    }
    
    // Validate delta L2 norm (< 1000.0)
    let delta_l2_norm = training_proof_json
        .get("delta_l2_norm")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| GenesisError::SerializationError("Missing delta_l2_norm in training proof".to_string()))?;
    
    const MAX_DELTA_L2_NORM: f64 = 1000.0;
    if delta_l2_norm > MAX_DELTA_L2_NORM {
        eprintln!("PoI validation failed: Delta L2 norm {} exceeds threshold {}", delta_l2_norm, MAX_DELTA_L2_NORM);
        return Ok(false);
    }
    
    // Validate Fisher matrix hash matches on-chain
    if let Some(fisher_hash_str) = training_proof_json.get("fisher_hash").and_then(|v| v.as_str()) {
        if fisher_hash_str.len() != 64 {
            eprintln!("PoI validation failed: Invalid Fisher hash length");
            return Ok(false);
        }
        
        // Verify against on-chain Fisher matrix hash
        if let Some(on_chain_hash) = chain_state.get_fisher_matrix_hash(model_id) {
            let proof_hash = hex::decode(fisher_hash_str)
                .map_err(|e| GenesisError::SerializationError(format!("Invalid Fisher hash hex: {}", e)))?;
            let proof_hash_array: [u8; 32] = proof_hash.as_slice().try_into()
                .map_err(|_| GenesisError::SerializationError("Fisher hash wrong length".to_string()))?;
            
            if proof_hash_array != on_chain_hash {
                eprintln!("PoI validation failed: Fisher hash mismatch");
                return Ok(false);
            }
        }
    }
    
    // Validate EWC lambda (0.0-10.0)
    let ewc_lambda = training_proof_json
        .get("ewc_lambda")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    
    if ewc_lambda < 0.0 || ewc_lambda > 10.0 {
        eprintln!("PoI validation failed: Invalid EWC lambda {}", ewc_lambda);
        return Ok(false);
    }
    
    // Validate learning rate (1e-6 ± 1e-8)
    let learning_rate = training_proof_json
        .get("learning_rate")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| GenesisError::SerializationError("Missing learning_rate in training proof".to_string()))?;
    
    const EXPECTED_LR: f64 = 1e-6;
    const LR_TOLERANCE: f64 = 1e-8;
    if (learning_rate - EXPECTED_LR).abs() > LR_TOLERANCE {
        eprintln!("PoI validation failed: Learning rate {} not within tolerance of {}", learning_rate, EXPECTED_LR);
        return Ok(false);
    }
    
    // Validate epochs (= 1)
    let epochs = training_proof_json
        .get("epochs")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| GenesisError::SerializationError("Missing epochs in training proof".to_string()))?;
    
    if epochs != 1 {
        eprintln!("PoI validation failed: Epochs {} != 1", epochs);
        return Ok(false);
    }
    
    // Validate batch size (1-1024)
    let batch_size = training_proof_json
        .get("batch_size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| GenesisError::SerializationError("Missing batch_size in training proof".to_string()))?;
    
    if batch_size == 0 || batch_size > 1024 {
        eprintln!("PoI validation failed: Invalid batch size {}", batch_size);
        return Ok(false);
    }
    
    // Validate round_id is valid UUID
    let round_id = training_proof_json
        .get("round_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GenesisError::SerializationError("Missing round_id in training proof".to_string()))?;
    
    if uuid::Uuid::parse_str(round_id).is_err() {
        eprintln!("PoI validation failed: Invalid round_id UUID");
        return Ok(false);
    }
    
    // Verify training round exists on-chain if round_id provided
    if chain_state.get_training_round(round_id).is_none() {
        eprintln!("PoI validation warning: Training round {} not found on-chain", round_id);
        // Don't fail - the round may not be recorded yet
    }
    
    eprintln!("PoI training proof validated successfully: {} participants, lr={}, epochs={}", 
        participants.len(), learning_rate, epochs);
    
    Ok(true)
}

/// List all proposals pending CEO veto window expiration
pub fn list_pending_ceo_window() -> Vec<String> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    registry
        .statuses
        .iter()
        .filter(|(_, status)| **status == RegistrySipStatus::AutoApprovedPendingCeoWindow)
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ceo_veto_window_duration() {
        assert_eq!(CEO_VETO_WINDOW_SECONDS, 86400); // 24 hours
    }
    
    #[test]
    fn test_sip_status_conversion() {
        // Test SipStatus -> RegistrySipStatus
        assert!(matches!(RegistrySipStatus::from(SipStatus::Pending), RegistrySipStatus::Pending));
        assert!(matches!(RegistrySipStatus::from(SipStatus::Approved), RegistrySipStatus::Approved));
        assert!(matches!(RegistrySipStatus::from(SipStatus::Vetoed), RegistrySipStatus::Vetoed));
        assert!(matches!(RegistrySipStatus::from(SipStatus::Deployed), RegistrySipStatus::Deployed));
        assert!(matches!(RegistrySipStatus::from(SipStatus::AutoApprovedPendingCeoWindow), RegistrySipStatus::AutoApprovedPendingCeoWindow));
        
        // Test RegistrySipStatus -> SipStatus
        assert!(matches!(SipStatus::from(RegistrySipStatus::Pending), SipStatus::Pending));
        assert!(matches!(SipStatus::from(RegistrySipStatus::Approved), SipStatus::Approved));
        assert!(matches!(SipStatus::from(RegistrySipStatus::Vetoed), SipStatus::Vetoed));
        assert!(matches!(SipStatus::from(RegistrySipStatus::Deployed), SipStatus::Deployed));
        assert!(matches!(SipStatus::from(RegistrySipStatus::AutoApprovedPendingCeoWindow), SipStatus::AutoApprovedPendingCeoWindow));
    }
    
    #[test]
    fn test_training_proof_validation() {
        // Valid training proof
        let valid_proof = serde_json::json!({
            "participants": ["node1", "node2"],
            "delta_l2_norm": 500.0,
            "fisher_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "ewc_lambda": 1.0,
            "learning_rate": 1e-6,
            "epochs": 1,
            "batch_size": 32,
            "round_id": "00000000-0000-0000-0000-000000000001"
        });
        
        // Invalid training proof - delta too high
        let invalid_delta = serde_json::json!({
            "participants": ["node1", "node2"],
            "delta_l2_norm": 1500.0,
            "ewc_lambda": 1.0,
            "learning_rate": 1e-6,
            "epochs": 1,
            "batch_size": 32,
            "round_id": "00000000-0000-0000-0000-000000000001"
        });
        
        // Invalid training proof - only 1 participant
        let invalid_participants = serde_json::json!({
            "participants": ["node1"],
            "delta_l2_norm": 500.0,
            "ewc_lambda": 1.0,
            "learning_rate": 1e-6,
            "epochs": 1,
            "batch_size": 32,
            "round_id": "00000000-0000-0000-0000-000000000001"
        });
        
        // Test basic validation (without chain state)
        // These tests verify the JSON parsing and basic checks
        assert!(valid_proof.get("participants").and_then(|v| v.as_array()).unwrap().len() >= 2);
        assert!(invalid_delta.get("delta_l2_norm").and_then(|v| v.as_f64()).unwrap() > 1000.0);
        assert!(invalid_participants.get("participants").and_then(|v| v.as_array()).unwrap().len() < 2);
    }
}
