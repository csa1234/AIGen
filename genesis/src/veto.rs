use crate::shutdown::is_shutdown;
use crate::types::{CeoSignature, GenesisError, SipProposal};
use crate::authority::verify_ceo_signature;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

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
    registry
        .statuses
        .insert(id.clone(), SipStatus::Pending);
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
    registry
        .proposals
        .insert(proposal_id.to_string(), updated);

    persist_registry(&registry)?;

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

    registry
        .statuses
        .insert(proposal_id.to_string(), SipStatus::Approved);
    registry
        .proposals
        .insert(proposal_id.to_string(), updated);

    persist_registry(&registry)?;

    Ok(())
}

pub fn can_deploy_sip(proposal_id: &str) -> bool {
    if is_shutdown() {
        return false;
    }

    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");

    match registry.statuses.get(proposal_id) {
        Some(SipStatus::Approved) | Some(SipStatus::Deployed) => true,
        _ => false,
    }
}

pub fn get_sip_status(proposal_id: &str) -> Option<SipStatus> {
    let registry = SIP_REGISTRY.lock().expect("sip registry mutex poisoned");
    registry.statuses.get(proposal_id).cloned()
}
