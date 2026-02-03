// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use blockchain_core::{Blockchain, Transaction};
use blockchain_core::ChainId;
use consensus::{ConsensusState, PoIProof};
use genesis::{
    emergency_shutdown,
    is_shutdown,
    shutdown_registry,
    submit_sip,
    veto_sip,
    approve_sip,
    can_deploy_sip,
    CEO_WALLET,
    CeoSignature,
    GenesisConfig,
    GenesisError,
    ShutdownCommand,
    SipProposal,
    WalletAddress,
};
use network::{broadcast_shutdown, NetworkNode, NetworkMessage, ShutdownPropagationConfig};
use std::time::{SystemTime, UNIX_EPOCH};

fn dummy_signature() -> CeoSignature {
    use ed25519_dalek::Signature;
    CeoSignature(Signature::from_bytes(&[0u8; 64]).unwrap())
}

fn dummy_shutdown_command() -> ShutdownCommand {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let network_magic = GenesisConfig::default().network_magic;
    ShutdownCommand {
        timestamp: ts,
        reason: "test".to_string(),
        ceo_signature: dummy_signature(),
        nonce: 1,
        network_magic,
    }
}

#[test]
fn test_ceo_signature_verification_fails_for_invalid() {
    let cmd = dummy_shutdown_command();
    let res = genesis::verify_ceo_signature(&cmd.message_to_sign(), &cmd.ceo_signature);
    assert!(matches!(res, Err(GenesisError::InvalidSignature)));
}

#[test]
fn test_emergency_shutdown_sets_flag_and_registry() {
    let cmd = dummy_shutdown_command();
    let _ = emergency_shutdown(cmd.clone());

    assert!(is_shutdown());
    let registry = shutdown_registry();
    assert!(!registry.commands.is_empty());
}

#[test]
fn test_shutdown_blocks_transactions_and_consensus() {
    let _ = emergency_shutdown(dummy_shutdown_command());

    let res = genesis::check_shutdown();
    assert!(matches!(res, Err(GenesisError::ShutdownActive)));

    let state = ConsensusState::Active;
    let new_state = state.check_and_update();
    assert_eq!(new_state, ConsensusState::Shutdown);
    assert!(!new_state.can_process_blocks());
}

#[test]
fn test_sip_veto_power_flow() {
    let proposal = SipProposal {
        proposal_hash: [0u8; 32],
        proposal_id: "sip-1".to_string(),
        description: "Test".to_string(),
        code_changes_hash: [0u8; 32],
        proposer: WalletAddress::new(CEO_WALLET.to_string()).unwrap(),
        timestamp: 0,
        ceo_approved: false,
    };

    let id = submit_sip(proposal).unwrap();

    // With no valid CEO signatures, deployment should not be allowed.
    assert!(!can_deploy_sip(&id));

    // Veto with invalid signature still records veto status due to verification failure.
    let _ = veto_sip(&id, dummy_signature());

    assert!(!can_deploy_sip(&id));

    // Approve with invalid signature will be rejected.
    let res = approve_sip(&id, dummy_signature());
    assert!(res.is_err());
}

#[test]
fn test_ceo_wallet_constant_address_format() {
    let wallet = WalletAddress::new(CEO_WALLET.to_string()).unwrap();
    assert_eq!(wallet.as_str(), CEO_WALLET);
}

#[test]
fn test_shutdown_propagation_message_wraps_command() {
    let cmd = dummy_shutdown_command();
    let cfg = ShutdownPropagationConfig {
        retry_attempts: 3,
        timeout_ms: 1000,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _ = rt.block_on(broadcast_shutdown(cmd, cfg, None));

    assert!(is_shutdown());
}

#[test]
fn test_network_node_respects_shutdown() {
    let mut node = NetworkNode::new("peer-1".to_string());

    let mut chain = Blockchain::new();
    let chain_id = ChainId::from_str_id(&GenesisConfig::default().chain_id);
    let tx = Transaction::new(
        CEO_WALLET.to_string(),
        "0x0000000000000000000000000000000000000002".to_string(),
        1,
        0,
        0,
        true,
        chain_id,
        None,
    )
    .unwrap();

    let block = blockchain_core::Block::new([0u8; 32], vec![tx], 1, 1, None);
    let _ = chain.add_block(block);

    let _ = emergency_shutdown(dummy_shutdown_command());

    node.check_shutdown();
    assert!(!node.should_accept_connections());

    let msg = NetworkMessage::Ping;
    assert!(msg.priority() < NetworkMessage::ShutdownSignal(
        crate::network::shutdown_propagation::ShutdownMessage {
            command: dummy_shutdown_command(),
            signature: dummy_signature(),
            timestamp: 0,
            network_magic: GenesisConfig::default().network_magic,
        }
    )
    .priority());
}
