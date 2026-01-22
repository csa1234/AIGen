//! Model sharding utilities for splitting and combining large AI models.
//!
//! This module provides functions to split large model files (potentially 100GB+)
//! into manageable 4GB shards for distributed storage. Each shard is verified
//! with SHA-256 hashing to ensure integrity during transfer and storage.

use std::cmp::min;
use std::path::Path;

use genesis::{check_shutdown, GenesisError};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::registry::ModelShard;

pub const SHARD_SIZE: u64 = 4_294_967_296;
pub const BUFFER_SIZE: usize = 8_388_608;

#[derive(Debug, Error)]
pub enum ShardError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid shard sequence")]
    InvalidSequence,
    #[error("hash mismatch for shard {0}")]
    HashMismatch(u32),
    #[error("shard file not found: {0}")]
    ShardNotFound(String),
    #[error("shutdown active")]
    Shutdown,
    #[error("genesis error: {0}")]
    Genesis(#[from] genesis::GenesisError),
}

fn ensure_running() -> Result<(), ShardError> {
    check_shutdown().map_err(|err| match err {
        GenesisError::ShutdownActive => ShardError::Shutdown,
        other => ShardError::Genesis(other),
    })
}

/// Split a model file into 4GB shards with SHA-256 verification hashes.
///
/// Returns the generated shard metadata for registry registration.
pub async fn split_model_file(
    model_path: &Path,
    output_dir: &Path,
    model_id: &str,
) -> Result<Vec<ModelShard>, ShardError> {
    ensure_running()?;

    let metadata = fs::metadata(model_path).await?;
    let file_size = metadata.len();
    if file_size == 0 {
        return Err(ShardError::InvalidSequence);
    }

    println!(
        "splitting model file: path={}, size={}",
        model_path.display(),
        file_size
    );

    let total_shards = file_size.div_ceil(SHARD_SIZE) as u32;
    fs::create_dir_all(output_dir).await?;

    let mut source = File::open(model_path).await?;
    let mut shards = Vec::with_capacity(total_shards as usize);
    let mut processed: u64 = 0;
    let mut next_log_percent = 10u32;

    for shard_index in 0..total_shards {
        ensure_running()?;
        let shard_path = output_dir.join(format!("{}_shard_{}.bin", model_id, shard_index));
        let mut shard_file = File::create(&shard_path).await?;
        let shard_target = min(SHARD_SIZE, file_size - processed);
        let mut shard_written = 0u64;
        let mut hasher = Sha256::new();

        while shard_written < shard_target {
            ensure_running()?;
            let to_read = min(BUFFER_SIZE as u64, shard_target - shard_written) as usize;
            let mut buffer = vec![0u8; to_read];
            let read: usize = source.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            shard_file.write_all(&buffer[..read]).await?;
            shard_written += read as u64;
            processed += read as u64;
        }

        if shard_written < shard_target {
            return Err(ShardError::InvalidSequence);
        }

        shard_file.flush().await?;
        shard_file.sync_all().await?;

        let digest = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);

        println!(
            "created shard {}/{}: hash={}",
            shard_index + 1,
            total_shards,
            hex::encode(hash)
        );

        shards.push(ModelShard {
            model_id: model_id.to_string(),
            shard_index,
            total_shards,
            hash,
            size: shard_written,
            ipfs_cid: None,
            http_urls: Vec::new(),
            locations: Vec::new(),
        });

        let progress = ((shard_index + 1) * 100) / total_shards;
        if progress >= next_log_percent {
            println!(
                "split progress: {}% ({}/{})",
                progress,
                shard_index + 1,
                total_shards
            );
            next_log_percent += 10;
        }
    }

    Ok(shards)
}

/// Combine model shards back into a single file with integrity validation.
pub async fn combine_shards(
    shards: &[ModelShard],
    shard_dir: &Path,
    output_path: &Path,
) -> Result<(), ShardError> {
    ensure_running()?;
    if shards.is_empty() {
        return Err(ShardError::InvalidSequence);
    }

    let model_id = shards[0].model_id.clone();
    let total_shards = shards[0].total_shards;
    if shards.len() as u32 != total_shards {
        return Err(ShardError::InvalidSequence);
    }

    for shard in shards {
        if shard.model_id != model_id || shard.total_shards != total_shards {
            return Err(ShardError::InvalidSequence);
        }
    }

    let mut ordered = shards.to_vec();
    ordered.sort_by_key(|shard| shard.shard_index);
    for (expected, shard) in ordered.iter().enumerate() {
        if shard.shard_index != expected as u32 {
            return Err(ShardError::InvalidSequence);
        }
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let mut output = File::create(output_path).await?;

    for shard in ordered.iter() {
        ensure_running()?;
        let shard_path = shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
        if fs::metadata(&shard_path).await.is_err() {
            return Err(ShardError::ShardNotFound(shard_path.display().to_string()));
        }

        let actual_hash = compute_file_hash(&shard_path).await?;
        if actual_hash != shard.hash {
            eprintln!(
                "shard integrity check failed: shard={}, expected={}, actual={}",
                shard.shard_index,
                hex::encode(shard.hash),
                hex::encode(actual_hash)
            );
            return Err(ShardError::HashMismatch(shard.shard_index));
        }

        let mut shard_file = File::open(&shard_path).await?;
        let mut buffer = vec![0u8; BUFFER_SIZE];
        loop {
            ensure_running()?;
            let read = shard_file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
        }
    }

    output.flush().await?;
    output.sync_all().await?;

    println!(
        "combined shards into model file: path={}",
        output_path.display()
    );
    Ok(())
}

/// Verify the integrity of a shard by comparing its hash with the expected value.
pub async fn verify_shard_integrity(
    shard_path: &Path,
    expected_hash: &[u8; 32],
) -> Result<bool, ShardError> {
    let actual = compute_file_hash(shard_path).await?;
    Ok(&actual == expected_hash)
}

/// Compute the SHA-256 hash for a file using streaming I/O.
pub async fn compute_file_hash(path: &Path) -> Result<[u8; 32], ShardError> {
    ensure_running()?;
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    loop {
        ensure_running()?;
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    Ok(hash)
}
