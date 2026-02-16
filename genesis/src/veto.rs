// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::authority::verify_ceo_signature;
use crate::benevolence::get_benevolence_model;
use crate::governance_config::GovernanceConfig;
use crate::shutdown::is_shutdown;
use crate::types::{CeoSignature, GenesisError, SipProposal};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::runtime::Handle;
use std::thread;

/// Trait for chain state operations needed by genesis
pub trait ChainStateProvider {
    /// Check if auto-approve threshold is met for a proposal
    fn check_auto_approve_threshold(
        &self,
        proposal_id: &str,
        min_approval_percentage: f64,
        min_participation_percentage: f64,
    ) -> bool;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SipStatus {
    Pending,
    Approved,
    Vetoed,
    Deployed,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SipRegistry {
    pub proposals: HashMap<String, SipProposal>,
    pub statuses: HashMap<String, SipStatus>,
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
    registry.statuses.insert(id.clone(), SipStatus::Pending);
    registry.proposals.insert(id.clone(), proposal);

    persist_registry(&registry)?;

    Ok(id)
}

pub fn veto_sip(proposal_id: &str, signature: CeoSignature) -> Result<(), GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    let proposal = registry
        .proposals
        .get(proposal_id)
        .cloned()
        .ok_or(GenesisError::ProposalNotFound)?;

    verify_ceo_signature(&proposal.message_to_sign_for_veto(), &signature)?;

    let mut updated = proposal.clone();
    updated.ceo_approved = false;

    registry
        .statuses
        .insert(proposal_id.to_string(), SipStatus::Vetoed);
    registry.proposals.insert(proposal_id.to_string(), updated);

    persist_registry(&registry)?;
    
    eprintln!("SIP {} vetoed by CEO (overrides auto-approval if any)", proposal_id); // NEW

    Ok(())
}

pub fn approve_sip(proposal_id: &str, signature: CeoSignature) -> Result<(), GenesisError> {
    let mut registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    let proposal = registry
        .proposals
        .get(proposal_id)
        .cloned()
        .ok_or(GenesisError::ProposalNotFound)?;

    verify_ceo_signature(&proposal.message_to_sign_for_approval(), &signature)?;

    let mut updated = proposal.clone();
    updated.ceo_approved = true;
    updated.approved_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    );

    registry
        .statuses
        .insert(proposal_id.to_string(), SipStatus::Approved);
    registry.proposals.insert(proposal_id.to_string(), updated);

    persist_registry(&registry)?;
    
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
            SipStatus::Approved | SipStatus::Vetoed | SipStatus::Deployed => {
                return Ok(false); // Already decided
            }
            _ => {}
        }
    }
    
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
            
            // Mark status as vetoed/rejected
            registry.statuses.insert(
                proposal_id.to_string(),
                SipStatus::Vetoed,
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
                    eprintln!("SIP {} benevolence score {} below threshold {}, requiring CEO review", proposal_id, score_data.score, threshold);
                    return Ok(false);
                }
            },
            Err(e) => {
                eprintln!("Benevolence scoring failed for SIP {}: {}, requiring CEO review", proposal_id, e);
                return Ok(false);
            }
        }
    } else {
        // Return or require CEO review if sample is missing
        eprintln!("SIP {} missing model output sample, requiring CEO review", proposal_id);
        return Ok(false);
    }
    
    // Check staker vote threshold using config values
    let meets_threshold = chain_state.check_auto_approve_threshold(
        proposal_id,
        config.min_approval_percentage,
        config.min_participation_percentage,
    );
    
    if meets_threshold {
        // Auto-approve
        let mut updated = current_proposal.clone(); // Use updated proposal with score
        updated.ceo_approved = true; // Mark as approved
        updated.auto_approved = true; // Mark as auto-approved
        updated.approved_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        );

        registry.statuses.insert(
            proposal_id.to_string(),
            SipStatus::Approved,
        );
        registry.proposals.insert(proposal_id.to_string(), updated);
        persist_registry(&registry)?;
        
        eprintln!(
            "SIP {} auto-approved by staker consensus (threshold met: {}% approval, {}% participation)",
            proposal_id,
            config.min_approval_percentage,
            config.min_participation_percentage
        );
        Ok(true)
    } else {
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

pub fn can_deploy_sip(proposal_id: &str) -> bool {
    if is_shutdown() {
        return false;
    }

    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    matches!(
        registry.statuses.get(proposal_id),
        Some(SipStatus::Approved) | Some(SipStatus::Deployed)
    )
}

pub fn get_sip_status(proposal_id: &str) -> Option<SipStatus> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    registry.statuses.get(proposal_id).cloned()
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
