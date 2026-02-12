// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

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

use crate::registry::{ModelShard, WeightFragment, FragmentManifest};
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;

pub const SHARD_SIZE: u64 = 4_294_967_296;
pub const BUFFER_SIZE: usize = 8_388_608;

pub const FRAGMENT_SIZE_MIN: u64 = 104_857_600;  // 100 MB
pub const FRAGMENT_SIZE_MAX: u64 = 524_288_000;  // 500 MB
pub const FRAGMENT_SIZE_DEFAULT: u64 = 209_715_200; // 200 MB

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FragmentConfig {
    pub fragment_size: u64,
    pub use_compression: bool,
    pub compression_level: i32, // zstd level 1-22
}

impl Default for FragmentConfig {
    fn default() -> Self {
        Self {
            fragment_size: FRAGMENT_SIZE_DEFAULT,
            use_compression: true,
            compression_level: 3,
        }
    }
}

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

/// Split a model file into weight fragments (100-500MB) with SHA3 verification and zstd compression.
///
/// Returns the generated fragment metadata and a FragmentManifest for registry registration.
pub async fn split_model_into_fragments(
    model_path: &Path,
    output_dir: &Path,
    model_id: &str,
    config: FragmentConfig,
) -> Result<(Vec<WeightFragment>, FragmentManifest), ShardError> {
    ensure_running()?;

    let metadata = fs::metadata(model_path).await?;
    let file_size = metadata.len();
    let total_size_bytes = file_size; // Store original model size
    if file_size == 0 {
        return Err(ShardError::InvalidSequence);
    }

    // Validate fragment size constraints
    let fragment_size = config.fragment_size.clamp(FRAGMENT_SIZE_MIN, FRAGMENT_SIZE_MAX);

    println!(
        "splitting model into fragments: path={}, size={}, fragment_size={}",
        model_path.display(),
        file_size,
        fragment_size
    );

    let total_fragments = file_size.div_ceil(fragment_size) as u32;
    fs::create_dir_all(output_dir).await?;

    let mut source = File::open(model_path).await?;
    let mut fragments = Vec::with_capacity(total_fragments as usize);
    let mut processed: u64 = 0;
    let mut next_log_percent = 10u32;
    let mut any_fragment_compressed = false;

    for fragment_index in 0..total_fragments {
        ensure_running()?;
        let fragment_path = output_dir.join(format!("{}_frag_{}.bin", model_id, fragment_index));
        let fragment_target = min(fragment_size, file_size - processed);

        // Read fragment data into memory
        let mut fragment_data = Vec::with_capacity(fragment_target as usize);
        let mut fragment_read = 0u64;
        let mut hasher = Sha3_256::new();

        while fragment_read < fragment_target {
            ensure_running()?;
            let to_read = min(BUFFER_SIZE as u64, fragment_target - fragment_read) as usize;
            let mut buffer = vec![0u8; to_read];
            let read: usize = source.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            fragment_data.extend_from_slice(&buffer[..read]);
            fragment_read += read as u64;
            processed += read as u64;
        }

        if fragment_read < fragment_target {
            return Err(ShardError::InvalidSequence);
        }

        let digest = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);

        let uncompressed_size = fragment_data.len() as u64;
        let mut compressed_size: Option<u64> = None;
        let mut final_data = fragment_data;
        let mut is_compressed = false;

        // Apply zstd compression if enabled
        if config.use_compression {
            let compression_level = config.compression_level.clamp(1, 22);
            match zstd::encode_all(final_data.as_slice(), compression_level) {
                Ok(compressed) => {
                    if compressed.len() < final_data.len() {
                        compressed_size = Some(compressed.len() as u64);
                        final_data = compressed;
                        is_compressed = true;
                        any_fragment_compressed = true;
                    }
                }
                Err(e) => {
                    eprintln!("compression failed for fragment {}: {}", fragment_index, e);
                }
            }
        }

        // Write the fragment file
        let mut fragment_file = File::create(&fragment_path).await?;
        fragment_file.write_all(&final_data).await?;
        fragment_file.flush().await?;
        fragment_file.sync_all().await?;

        println!(
            "created fragment {}/{}: hash={}, size={} bytes, compressed={}",
            fragment_index + 1,
            total_fragments,
            hex::encode(hash),
            final_data.len(),
            is_compressed
        );

        fragments.push(WeightFragment {
            model_id: model_id.to_string(),
            fragment_id: format!("{}_frag_{}", model_id, fragment_index),
            fragment_index,
            total_fragments,
            hash,
            size_bytes: uncompressed_size,
            compressed_size_bytes: compressed_size,
            is_compressed,
            layer_range: None,
            locations: Vec::new(),
        });

        let progress = ((fragment_index + 1) * 100) / total_fragments;
        if progress >= next_log_percent {
            println!(
                "fragment split progress: {}% ({}/{})",
                progress,
                fragment_index + 1,
                total_fragments
            );
            next_log_percent += 10;
        }
    }

    // Build the FragmentManifest
    let manifest = FragmentManifest {
        model_id: model_id.to_string(),
        total_fragments,
        total_size_bytes,
        fragment_size_bytes: fragment_size,
        is_compressed: any_fragment_compressed,
        fragments: fragments.clone(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| ShardError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid system time",
            )))?
            .as_secs() as i64,
    };

    println!(
        "successfully created {} fragments for model {} (manifest created)",
        fragments.len(),
        model_id
    );
    Ok((fragments, manifest))
}

/// Combine weight fragments back into a single file with integrity validation.
pub async fn combine_fragments(
    fragments: &[WeightFragment],
    fragment_dir: &Path,
    output_path: &Path,
) -> Result<(), ShardError> {
    ensure_running()?;
    if fragments.is_empty() {
        return Err(ShardError::InvalidSequence);
    }

    let model_id = fragments[0].model_id.clone();
    let total_fragments = fragments[0].total_fragments;
    if fragments.len() as u32 != total_fragments {
        return Err(ShardError::InvalidSequence);
    }

    for fragment in fragments {
        if fragment.model_id != model_id || fragment.total_fragments != total_fragments {
            return Err(ShardError::InvalidSequence);
        }
    }

    let mut ordered = fragments.to_vec();
    ordered.sort_by_key(|fragment| fragment.fragment_index);
    for (expected, fragment) in ordered.iter().enumerate() {
        if fragment.fragment_index != expected as u32 {
            return Err(ShardError::InvalidSequence);
        }
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let mut output = File::create(output_path).await?;

    for fragment in ordered.iter() {
        ensure_running()?;
        let fragment_path = fragment_dir.join(format!("{}_frag_{}.bin", model_id, fragment.fragment_index));
        if fs::metadata(&fragment_path).await.is_err() {
            return Err(ShardError::ShardNotFound(fragment_path.display().to_string()));
        }

        // Read fragment data
        let mut fragment_file = File::open(&fragment_path).await?;
        let mut fragment_data = Vec::new();
        fragment_file.read_to_end(&mut fragment_data).await?;

        // Decompress first if needed (hash was computed on uncompressed data during split)
        let data_to_verify = if fragment.is_compressed {
            match zstd::decode_all(fragment_data.as_slice()) {
                Ok(decompressed) => decompressed,
                Err(e) => {
                    eprintln!("decompression failed for fragment {}: {}", fragment.fragment_index, e);
                    return Err(ShardError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("decompression failed: {}", e),
                    )));
                }
            }
        } else {
            fragment_data
        };

        // Verify SHA3 hash on decompressed data
        let mut hasher = Sha3_256::new();
        hasher.update(&data_to_verify);
        let actual_hash = hasher.finalize();
        let mut computed_hash = [0u8; 32];
        computed_hash.copy_from_slice(&actual_hash);

        if computed_hash != fragment.hash {
            eprintln!(
                "fragment integrity check failed: fragment={}, expected={}, actual={}",
                fragment.fragment_index,
                hex::encode(fragment.hash),
                hex::encode(computed_hash)
            );
            return Err(ShardError::HashMismatch(fragment.fragment_index));
        }

        // Write the verified decompressed data to output
        output.write_all(&data_to_verify).await?;

        println!(
            "combined fragment {}/{} into model file",
            fragment.fragment_index + 1,
            total_fragments
        );
    }

    output.flush().await?;
    output.sync_all().await?;

    println!(
        "combined fragments into model file: path={}",
        output_path.display()
    );
    Ok(())
}
