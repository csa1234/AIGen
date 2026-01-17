use ed25519_dalek::{Signature, Signer, SigningKey};
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::{emergency_shutdown, CeoSignature, GenesisConfig, ShutdownCommand};
use model::sharding::{compute_file_hash, SHARD_SIZE};
use model::{combine_shards, split_model_file, verify_shard_integrity, ShardError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

const CEO_SECRET_KEY_HEX: &str =
    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

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

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("aigen_{prefix}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
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

async fn write_sparse_file_with_markers(path: &Path, size: u64) {
    let mut file = File::create(path).await.expect("create sparse file");
    file.set_len(size).await.expect("set length");

    let total_shards = (size + SHARD_SIZE - 1) / SHARD_SIZE;
    for index in 0..total_shards {
        let offset = index * SHARD_SIZE;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .expect("seek");
        let marker = (index as u32).to_le_bytes();
        file.write_all(&marker).await.expect("write marker");
    }

    file.flush().await.expect("flush file");
}

#[tokio::test]
async fn test_split_and_combine_small_file() {
    reset_shutdown_for_tests();
    let dir = temp_dir("split_small");
    let model_path = dir.join("model.bin");
    let shard_dir = dir.join("shards");
    let output_path = dir.join("combined.bin");
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
    let model_path = dir.join("model-large.bin");
    let shard_dir = dir.join("shards");
    let size = 10u64 * 1024 * 1024 * 1024;

    write_sparse_file_with_markers(&model_path, size).await;

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
    let shard_path = dir.join("shard.bin");

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
    file.seek(std::io::SeekFrom::Start(0))
        .await
        .expect("seek");
    file.write_all(&[9u8; 4]).await.expect("write");
    file.flush().await.expect("flush");

    let valid = verify_shard_integrity(&shard_path, &hash)
        .await
        .expect("verify");
    assert!(!valid);
}

#[tokio::test]
async fn test_shutdown_during_split() {
    reset_shutdown_for_tests();
    let dir = temp_dir("shutdown_split");
    let model_path = dir.join("model.bin");
    let shard_dir = dir.join("shards");
    let size = 6u64 * 1024 * 1024 * 1024;

    write_sparse_file_with_markers(&model_path, size).await;

    let handle = tokio::spawn({
        let model_path = model_path.clone();
        let shard_dir = shard_dir.clone();
        async move { split_model_file(&model_path, &shard_dir, "shutdown-model").await }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let command = signed_shutdown_command(1);
    emergency_shutdown(command).expect("shutdown");

    let result = handle.await.expect("join");
    assert!(matches!(result, Err(ShardError::Shutdown)));

    reset_shutdown_for_tests();
    let _ = fs::remove_dir_all(&dir).await;
}
