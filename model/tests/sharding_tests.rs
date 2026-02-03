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
use model::sharding::{compute_file_hash, SHARD_SIZE};
use model::{combine_shards, split_model_file, verify_shard_integrity, ShardError};
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Notify;

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

fn temp_dir(prefix: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("aigen_{prefix}_"))
        .tempdir()
        .expect("create temp dir")
}

async fn write_test_file(path: &Path, size: u64) {
    let mut file = File::create(path).await.expect("create file");
    let chunk_size = 1024 * 1024;
    let chunks = size / chunk_size;
    let remainder = size % chunk_size;

    for index in 0..chunks {
        let value = (index % 255) as u8;
        let buffer = vec![value; chunk_size as usize];
        file.write_all(&buffer).await.expect("write chunk");
    }

    if remainder > 0 {
        let buffer = vec![42u8; remainder as usize];
        file.write_all(&buffer).await.expect("write remainder");
    }

    file.flush().await.expect("flush file");
}

async fn write_sparse_file_with_markers(path: &Path, size: u64) -> std::io::Result<()> {
    let mut file = File::create(path).await?;
    file.set_len(size).await?;

    let total_shards = (size + SHARD_SIZE - 1) / SHARD_SIZE;
    for index in 0..total_shards {
        let offset = index * SHARD_SIZE;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let marker = (index as u32).to_le_bytes();
        file.write_all(&marker).await?;
    }

    file.flush().await?;
    Ok(())
}

#[tokio::test]
async fn test_split_and_combine_small_file() {
    reset_shutdown_for_tests();
    let dir = temp_dir("split_small");
    let root = dir.path();
    let model_path = root.join("model.bin");
    let shard_dir = root.join("shards");
    let output_path = root.join("combined.bin");
    let size = 100 * 1024 * 1024;

    write_test_file(&model_path, size).await;

    let shards = split_model_file(&model_path, &shard_dir, "test-model")
        .await
        .expect("split");

    let expected = ((size + SHARD_SIZE - 1) / SHARD_SIZE) as usize;
    assert_eq!(shards.len(), expected);

    let original_hash = compute_file_hash(&model_path).await.expect("hash");
    if let Some(first) = shards.first() {
        if shards.len() == 1 {
            assert_eq!(first.hash, original_hash);
        }
    }

    combine_shards(&shards, &shard_dir, &output_path)
        .await
        .expect("combine");

    let combined_hash = compute_file_hash(&output_path).await.expect("hash");
    assert_eq!(original_hash, combined_hash);
}

#[tokio::test]
async fn test_split_large_file() {
    reset_shutdown_for_tests();
    let dir = temp_dir("split_large");
    let root = dir.path();
    let model_path = root.join("model-large.bin");
    let shard_dir = root.join("shards");
    let size = if std::env::var("AIGEN_RUN_LARGE_SHARD_TEST").is_ok() {
        512 * 1024 * 1024
    } else {
        64 * 1024 * 1024
    };

    if let Err(err) = write_sparse_file_with_markers(&model_path, size).await {
        if err.kind() == ErrorKind::StorageFull {
            eprintln!("skipping test_split_large_file: insufficient disk space");
            return;
        }
        panic!("failed to create sparse file: {err}");
    }

    let shards = split_model_file(&model_path, &shard_dir, "large-model")
        .await
        .expect("split");

    let expected = ((size + SHARD_SIZE - 1) / SHARD_SIZE) as usize;
    assert_eq!(shards.len(), expected);

    let mut hashes = std::collections::HashSet::new();
    for (index, shard) in shards.iter().enumerate() {
        assert!(shard.size > 0);
        assert!(shard.size <= SHARD_SIZE);
        if index == shards.len() - 1 {
            let expected_last = size - (SHARD_SIZE * (shards.len() as u64 - 1));
            assert_eq!(shard.size, expected_last);
        }
        hashes.insert(shard.hash);
    }
    assert_eq!(hashes.len(), shards.len());
}

#[tokio::test]
async fn test_shard_integrity_verification() {
    reset_shutdown_for_tests();
    let dir = temp_dir("integrity");
    let shard_path = dir.path().join("shard.bin");

    write_test_file(&shard_path, 5 * 1024 * 1024).await;
    let hash = compute_file_hash(&shard_path).await.expect("hash");
    let valid = verify_shard_integrity(&shard_path, &hash)
        .await
        .expect("verify");
    assert!(valid);

    let mut file = OpenOptions::new()
        .write(true)
        .open(&shard_path)
        .await
        .expect("open");
    file.seek(std::io::SeekFrom::Start(0)).await.expect("seek");
    file.write_all(&[9u8; 4]).await.expect("write");
    file.flush().await.expect("flush");

    let valid = verify_shard_integrity(&shard_path, &hash)
        .await
        .expect("verify");
    assert!(!valid);
}

#[tokio::test(flavor = "current_thread")]
async fn test_shutdown_during_split() {
    reset_shutdown_for_tests();
    let dir = temp_dir("shutdown_split");
    let root = dir.path();
    let model_path = root.join("model.bin");
    let shard_dir = root.join("shards");
    let size = 256 * 1024 * 1024;

    if let Err(err) = write_sparse_file_with_markers(&model_path, size).await {
        if err.kind() == ErrorKind::StorageFull {
            eprintln!("skipping test_shutdown_during_split: insufficient disk space");
            return;
        }
        panic!("failed to create sparse file: {err}");
    }

    let model_path = model_path.clone();
    let shard_dir_clone = shard_dir.clone();
    let started = std::sync::Arc::new(Notify::new());
    let proceed = std::sync::Arc::new(Notify::new());
    let started_task = started.clone();
    let proceed_task = proceed.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            let handle = tokio::task::spawn_local(async move {
                started_task.notify_one();
                proceed_task.notified().await;
                split_model_file(&model_path, &shard_dir_clone, "shutdown-model").await
            });

            let notified = tokio::time::timeout(Duration::from_secs(5), started.notified()).await;
            assert!(notified.is_ok());

            let command = signed_shutdown_command(1);
            emergency_shutdown(command).expect("shutdown");
            proceed.notify_one();

            handle.await.expect("join")
        })
        .await;
    assert!(matches!(result, Err(ShardError::Shutdown)));

    reset_shutdown_for_tests();
}
