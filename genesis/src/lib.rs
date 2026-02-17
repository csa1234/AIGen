// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Genesis CEO control layer for the AIGEN blockchain.
//!
//! This crate hardcodes the CEO wallet and public key, verifies CEO signatures,
//! exposes an irreversible global shutdown flag, and manages SIP approval/veto
//! as described in the CRITICAL PRIME DIRECTIVE of `spec.md`.

pub mod authority;
pub mod benevolence;
pub mod config;
pub mod constitution;
pub mod governance_config;
pub mod safety_oracle;
pub mod shutdown;
pub mod types;
pub mod veto;

pub use crate::authority::{
    is_ceo_wallet, verify_ceo_signature, verify_ceo_transaction, CeoAuthority, CeoTransactable,
};
pub use crate::benevolence::{
    BenevolenceModel,
    BenevolenceScore,
    BenevolenceModelConfig,
    BenevolenceError,
    get_benevolence_model,
    initialize_benevolence_model,
};
pub use crate::config::{GenesisConfig, CEO_PUBLIC_KEY_HEX, CEO_WALLET};
pub use crate::constitution::{
    check_constitutional_compliance,
    compute_constitution_hash,
    serialize_constitution,
    get_principles_by_category,
    Principle,
    PrincipleCategory,
    Violation,
    ViolationAction,
    CONSTITUTION,
    CONSTITUTIONAL_FILTER,
    ConstitutionalFilter,
    ConstitutionState,
    MAX_SCAN_LENGTH,
};
pub use crate::governance_config::GovernanceConfig;
pub use crate::safety_oracle::{
    SafetyOracleConfig,
    SafetyVote,
    SafetyOracleResult,
    SafetyOracleError,
    evaluate_safety,
    initialize_safety_oracle,
    get_safety_oracle_config,
};
pub use crate::shutdown::{
    check_shutdown, emergency_shutdown, is_shutdown, shutdown_registry, ShutdownRegistry,
};
pub use crate::types::{CeoSignature, GenesisError, ShutdownCommand, SipProposal, WalletAddress, SipStatus};
pub use crate::veto::{
    approve_sip, can_deploy_sip, check_and_auto_approve, get_sip_status, submit_sip, veto_sip,
    trigger_auto_approval_check, AutoApproveConfig, SipRegistry, ChainStateProvider,
    check_model_output_compliance, CEO_VETO_WINDOW_SECONDS, check_ceo_veto_window,
    process_expired_veto_windows, verify_poi_training_proof, get_rejection_logs,
    list_pending_ceo_window, TrainingRoundInfo, ModelApprovalRecord, G8Recommendation,
};
