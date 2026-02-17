// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use ed25519_dalek::{Signature, Signer, SigningKey};
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::{emergency_shutdown, CeoSignature, GenesisConfig, ShutdownCommand};
use model::{ModelError, ModelMetadata, ModelRegistry, ModelShard};
use model::registry::DeploymentStatus;
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const CEO_SECRET_KEY_HEX: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

fn signing_key() -> SigningKey {
    let bytes = hex::decode(CEO_SECRET_KEY_HEX).expect("valid hex");
    let seed: [u8; 32] = bytes.as_slice().try_into().expect("valid seed length");
    SigningKey::from_bytes(&seed)
}

fn signed_shutdown_command(nonce: u64) -> ShutdownCommand {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("valid time")
        .as_secs() as i64;
    let network_magic = GenesisConfig::default().network_magic;
    let placeholder = Signature::from_bytes(&[0u8; 64]);
    let mut command = ShutdownCommand {
        timestamp,
        reason: "test".to_string(),
        ceo_signature: CeoSignature(placeholder),
        nonce,
        network_magic,
    };
    let sig = signing_key().sign(&command.message_to_sign());
    command.ceo_signature = CeoSignature(sig);
    command
}

fn sample_metadata(model_id: &str, is_core_model: bool, created_at: i64) -> ModelMetadata {
    ModelMetadata {
        model_id: model_id.to_string(),
        name: "Test Model".to_string(),
        version: "v1.0".to_string(),
        total_size: 1024,
        shard_count: 2,
        verification_hashes: vec![[1u8; 32], [2u8; 32]],
        is_core_model,
        minimum_tier: None,
        is_experimental: false,
        created_at,
        deployment_status: DeploymentStatus::Stable,
        traffic_percentage: 100.0,
    }
}

fn sample_shard(metadata: &ModelMetadata, shard_index: u32) -> ModelShard {
    ModelShard {
        model_id: metadata.model_id.clone(),
        shard_index,
        total_shards: metadata.shard_count,
        hash: metadata.verification_hashes[shard_index as usize],
        size: 512,
        ipfs_cid: None,
        http_urls: Vec::new(),
        locations: Vec::new(),
    }
}

#[test]
fn test_model_registration() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let metadata = sample_metadata("mistral-7b", false, 1);

    registry.register_model(metadata.clone()).expect("register");

    let stored = registry.get_model("mistral-7b").expect("get");
    assert_eq!(stored.model_id, metadata.model_id);
}

#[test]
fn test_shard_registration() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let metadata = sample_metadata("mistral-7b", false, 1);
    registry.register_model(metadata.clone()).expect("register");

    let shard = sample_shard(&metadata, 0);
    registry
        .register_shard(shard.clone())
        .expect("register shard");

    let stored = registry.get_shard("mistral-7b", 0).expect("get shard");
    assert_eq!(stored.hash, shard.hash);
}

#[test]
fn test_core_model_designation() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let first = sample_metadata("core-model", true, 1);
    let second = sample_metadata("secondary-model", false, 2);

    registry.register_model(first).expect("register first");
    registry.register_model(second).expect("register second");

    let core = registry
        .get_core_model()
        .expect("core model")
        .expect("core model");
    assert_eq!(core.model_id, "core-model");

    registry
        .set_core_model("secondary-model")
        .expect("set core");
    let core = registry
        .get_core_model()
        .expect("core model")
        .expect("core model");
    assert_eq!(core.model_id, "secondary-model");

    let core_count = registry
        .list_models()
        .expect("list models")
        .iter()
        .filter(|model| model.is_core_model)
        .count();
    assert_eq!(core_count, 1);
}

#[test]
fn test_validation_failures() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let invalid = sample_metadata("", false, 1);
    let err = registry
        .register_model(invalid)
        .expect_err("invalid model id");
    assert!(matches!(err, ModelError::InvalidModelId));

    let metadata = sample_metadata("valid-model", false, 2);
    registry.register_model(metadata.clone()).expect("register");

    let mut shard = sample_shard(&metadata, 0);
    shard.hash = [9u8; 32];
    let err = registry.register_shard(shard).expect_err("mismatched hash");
    assert!(matches!(err, ModelError::InvalidShard));

    let invalid_shard = ModelShard {
        model_id: metadata.model_id.clone(),
        shard_index: 3,
        total_shards: metadata.shard_count,
        hash: metadata.verification_hashes[0],
        size: 512,
        ipfs_cid: None,
        http_urls: Vec::new(),
        locations: Vec::new(),
    };
    let err = registry
        .register_shard(invalid_shard)
        .expect_err("invalid shard index");
    assert!(matches!(err, ModelError::InvalidShard));
}

#[test]
fn test_concurrent_access() {
    reset_shutdown_for_tests();
    let registry = Arc::new(ModelRegistry::new());
    let mut handles = Vec::new();

    for idx in 0..8 {
        let registry = Arc::clone(&registry);
        handles.push(thread::spawn(move || {
            let model_id = format!("model-{}", idx);
            let metadata = sample_metadata(&model_id, false, idx as i64);
            registry.register_model(metadata).expect("register");
        }));
    }

    for handle in handles {
        handle.join().expect("thread");
    }

    assert_eq!(registry.list_models().expect("list models").len(), 8);
}

#[test]
fn test_shutdown_blocks_registry() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();

    let command = signed_shutdown_command(1);
    emergency_shutdown(command).expect("shutdown");

    let metadata = sample_metadata("shutdown-model", false, 1);
    let err = registry
        .register_model(metadata)
        .expect_err("shutdown error");
    assert!(matches!(err, ModelError::Shutdown));

    reset_shutdown_for_tests();
}
