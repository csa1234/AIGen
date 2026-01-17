//! Storage backend abstraction for distributed model storage.
//!
//! Provides a unified interface for storing model shards across multiple
//! backends: local filesystem, IPFS, and HTTP mirrors. Supports redundancy
//! tracking and automatic failover between storage locations.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use genesis::check_shutdown;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient, TryFromUri};
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

use crate::registry::ModelShard;
use crate::sharding::{compute_file_hash, verify_shard_integrity, ShardError};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("IPFS error: {0}")]
    Ipfs(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("shard not found")]
    ShardNotFound,
    #[error("integrity check failed")]
    IntegrityFailed,
    #[error("all mirrors failed")]
    AllMirrorsFailed,
    #[error("shutdown active")]
    Shutdown,
}

fn ensure_running() -> Result<(), StorageError> {
    check_shutdown().map_err(|_| StorageError::Shutdown)
}

async fn hash_response_body(response: reqwest::Response) -> Result<[u8; 32], StorageError> {
    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        ensure_running()?;
        let chunk = chunk.map_err(|err| StorageError::Http(err.to_string()))?;
        hasher.update(&chunk);
    }
    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    Ok(hash)
}

impl From<ShardError> for StorageError {
    fn from(err: ShardError) -> Self {
        match err {
            ShardError::Io(err) => StorageError::Io(err),
            ShardError::Shutdown => StorageError::Shutdown,
            _ => StorageError::IntegrityFailed,
        }
    }
}

#[async_trait(?Send)]
pub trait StorageBackend: Send + Sync {
    /// Upload a shard file to the storage backend and return its location.
    async fn upload_shard(&self, shard: &ModelShard, data_path: &Path) -> Result<String, StorageError>;

    /// Download a shard file from the storage backend to the output path.
    async fn download_shard(&self, shard: &ModelShard, output_path: &Path) -> Result<(), StorageError>;

    /// Verify that a shard exists and matches its expected hash.
    async fn verify_shard(&self, shard: &ModelShard) -> Result<bool, StorageError>;

    /// Delete a shard file from the storage backend.
    async fn delete_shard(&self, shard: &ModelShard) -> Result<(), StorageError>;

    /// Return the storage backend identifier.
    fn backend_type(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct LocalStorage {
    root_dir: PathBuf,
    redundancy_factor: u8,
}

impl LocalStorage {
    /// Create a local storage backend rooted at the given directory.
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            redundancy_factor: 3,
        }
    }

    /// Build the primary shard path for local storage.
    pub fn shard_path(&self, shard: &ModelShard) -> PathBuf {
        self.root_dir
            .join("models")
            .join(&shard.model_id)
            .join("shards")
            .join(format!("shard_{}.bin", shard.shard_index))
    }

    fn replica_path(&self, shard: &ModelShard, replica_id: u8) -> PathBuf {
        self.root_dir
            .join("models")
            .join(&shard.model_id)
            .join("replicas")
            .join(format!("replica_{}", replica_id))
            .join(format!("shard_{}.bin", shard.shard_index))
    }
}

#[async_trait(?Send)]
impl StorageBackend for LocalStorage {
    async fn upload_shard(&self, shard: &ModelShard, data_path: &Path) -> Result<String, StorageError> {
        ensure_running()?;
        info!(
            "uploading shard to {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );

        let primary_path = self.shard_path(shard);
        if let Some(parent) = primary_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::copy(data_path, &primary_path).await?;

        if !verify_shard_integrity(&primary_path, &shard.hash).await? {
            let actual_hash = compute_file_hash(&primary_path).await?;
            warn!(
                "shard integrity check failed: shard={}, expected={}, actual={}",
                shard.shard_index,
                hex::encode(shard.hash),
                hex::encode(actual_hash)
            );
            return Err(StorageError::IntegrityFailed);
        }

        for replica_id in 1..self.redundancy_factor {
            ensure_running()?;
            let replica_path = self.replica_path(shard, replica_id);
            if let Some(parent) = replica_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::copy(&primary_path, &replica_path).await?;
        }

        Ok(primary_path.display().to_string())
    }

    async fn download_shard(&self, shard: &ModelShard, output_path: &Path) -> Result<(), StorageError> {
        ensure_running()?;
        info!(
            "downloading shard from {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );

        let mut source = self.shard_path(shard);
        if fs::metadata(&source).await.is_err() {
            let mut found = None;
            for replica_id in 1..self.redundancy_factor {
                let candidate = self.replica_path(shard, replica_id);
                if fs::metadata(&candidate).await.is_ok() {
                    found = Some(candidate);
                    break;
                }
            }
            if let Some(candidate) = found {
                source = candidate;
            } else {
                return Err(StorageError::ShardNotFound);
            }
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let bytes = fs::copy(&source, output_path).await?;
        debug!("download progress: {}/{} bytes", bytes, bytes);

        if !verify_shard_integrity(output_path, &shard.hash).await? {
            let actual_hash = compute_file_hash(output_path).await?;
            warn!(
                "shard integrity check failed: shard={}, expected={}, actual={}",
                shard.shard_index,
                hex::encode(shard.hash),
                hex::encode(actual_hash)
            );
            return Err(StorageError::IntegrityFailed);
        }

        Ok(())
    }

    async fn verify_shard(&self, shard: &ModelShard) -> Result<bool, StorageError> {
        ensure_running()?;
        let primary = self.shard_path(shard);
        if fs::metadata(&primary).await.is_ok() {
            return verify_shard_integrity(&primary, &shard.hash).await.map_err(StorageError::from);
        }

        for replica_id in 1..self.redundancy_factor {
            let candidate = self.replica_path(shard, replica_id);
            if fs::metadata(&candidate).await.is_ok() {
                if verify_shard_integrity(&candidate, &shard.hash).await? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    async fn delete_shard(&self, shard: &ModelShard) -> Result<(), StorageError> {
        ensure_running()?;
        let mut removed = false;
        let primary = self.shard_path(shard);
        if fs::remove_file(&primary).await.is_ok() {
            removed = true;
        }

        for replica_id in 1..self.redundancy_factor {
            let path = self.replica_path(shard, replica_id);
            if fs::remove_file(&path).await.is_ok() {
                removed = true;
            }
        }

        if removed {
            Ok(())
        } else {
            Err(StorageError::ShardNotFound)
        }
    }

    fn backend_type(&self) -> &str {
        "local"
    }
}

#[derive(Clone)]
pub struct IpfsStorage {
    client: IpfsClient,
    gateway_url: String,
    pin_shards: bool,
}

impl IpfsStorage {
    /// Create an IPFS storage backend using the provided API and gateway URLs.
    pub fn new(api_url: &str, gateway_url: &str) -> Result<Self, StorageError> {
        let client = IpfsClient::from_str(api_url)
            .map_err(|err| StorageError::Ipfs(err.to_string()))?;
        Ok(Self {
            client,
            gateway_url: gateway_url.trim_end_matches('/').to_string(),
            pin_shards: true,
        })
    }

    fn gateway_shard_url(&self, cid: &str) -> String {
        format!("{}/ipfs/{}", self.gateway_url, cid)
    }
}

#[async_trait(?Send)]
impl StorageBackend for IpfsStorage {
    async fn upload_shard(&self, shard: &ModelShard, data_path: &Path) -> Result<String, StorageError> {
        ensure_running()?;
        info!(
            "uploading shard to {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );

        let response = self
            .client
            .add_path(data_path)
            .await
            .map_err(|err| StorageError::Ipfs(err.to_string()))?;
        let cid = response
            .first()
            .map(|item| item.hash.clone())
            .ok_or_else(|| StorageError::Ipfs("empty ipfs add response".to_string()))?;

        if self.pin_shards {
            self.client
                .pin_add(&cid, true)
                .await
                .map_err(|err| StorageError::Ipfs(err.to_string()))?;
        }

        Ok(cid)
    }

    async fn download_shard(&self, shard: &ModelShard, output_path: &Path) -> Result<(), StorageError> {
        ensure_running()?;
        let cid = shard.ipfs_cid.as_ref().ok_or(StorageError::ShardNotFound)?;
        let url = self.gateway_shard_url(cid);
        info!(
            "downloading shard from {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );

        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|err| StorageError::Http(err.to_string()))?;
        let response = client.get(&url).send().await.map_err(|err| StorageError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Err(StorageError::ShardNotFound);
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut output = fs::File::create(output_path).await?;
        let mut downloaded = 0u64;
        let total = response.content_length().unwrap_or(0);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            ensure_running()?;
            let chunk = chunk.map_err(|err| StorageError::Http(err.to_string()))?;
            output.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            debug!("download progress: {}/{} bytes", downloaded, total);
        }
        output.flush().await?;
        output.sync_all().await?;

        if !verify_shard_integrity(output_path, &shard.hash).await? {
            let actual_hash = compute_file_hash(output_path).await?;
            warn!(
                "shard integrity check failed: shard={}, expected={}, actual={}",
                shard.shard_index,
                hex::encode(shard.hash),
                hex::encode(actual_hash)
            );
            return Err(StorageError::IntegrityFailed);
        }

        Ok(())
    }

    async fn verify_shard(&self, shard: &ModelShard) -> Result<bool, StorageError> {
        ensure_running()?;
        let cid = match shard.ipfs_cid.as_ref() {
            Some(cid) => cid,
            None => return Ok(false),
        };
        let url = self.gateway_shard_url(cid);
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|err| StorageError::Http(err.to_string()))?;
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|err| StorageError::Http(err.to_string()))?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let actual_hash = hash_response_body(response).await?;
        if actual_hash != shard.hash {
            return Err(StorageError::IntegrityFailed);
        }
        Ok(true)
    }

    async fn delete_shard(&self, shard: &ModelShard) -> Result<(), StorageError> {
        ensure_running()?;
        let cid = shard.ipfs_cid.as_ref().ok_or(StorageError::ShardNotFound)?;
        if self.pin_shards {
            self.client
                .pin_rm(cid, true)
                .await
                .map_err(|err| StorageError::Ipfs(err.to_string()))?;
        }
        Ok(())
    }

    fn backend_type(&self) -> &str {
        "ipfs"
    }
}

#[derive(Clone)]
pub struct HttpStorage {
    client: Client,
    mirror_urls: Vec<String>,
    timeout: Duration,
}

impl HttpStorage {
    /// Create an HTTP storage backend with the given mirror base URLs.
    pub fn new(mirror_urls: Vec<String>) -> Result<Self, StorageError> {
        let timeout = Duration::from_secs(300);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| StorageError::Http(err.to_string()))?;
        Ok(Self {
            client,
            mirror_urls,
            timeout,
        })
    }

    /// Construct a mirror shard URL for a given index.
    pub fn shard_url(&self, shard: &ModelShard, mirror_index: usize) -> String {
        let base = self
            .mirror_urls
            .get(mirror_index)
            .cloned()
            .unwrap_or_default();
        format!(
            "{}/models/{}/shards/shard_{}.bin",
            base.trim_end_matches('/'),
            shard.model_id,
            shard.shard_index
        )
    }
}

#[async_trait(?Send)]
impl StorageBackend for HttpStorage {
    async fn upload_shard(&self, shard: &ModelShard, data_path: &Path) -> Result<String, StorageError> {
        ensure_running()?;
        info!(
            "uploading shard to {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );

        let payload = fs::read(data_path).await?;
        let file_name = data_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("shard.bin")
            .to_string();
        let mut success_url = None;
        for (index, _) in self.mirror_urls.iter().enumerate() {
            ensure_running()?;
            let url = self.shard_url(shard, index);
            let part = Part::bytes(payload.clone()).file_name(file_name.clone());
            let form = Form::new().part("file", part);
            match self.client.post(&url).multipart(form).send().await {
                Ok(response) if response.status().is_success() => {
                    success_url = Some(url);
                }
                Ok(response) => {
                    error!(
                        "storage operation failed: backend={}, error={}",
                        self.backend_type(),
                        response.status()
                    );
                }
                Err(err) => {
                    error!(
                        "storage operation failed: backend={}, error={}",
                        self.backend_type(),
                        err
                    );
                }
            }
        }

        success_url.ok_or(StorageError::AllMirrorsFailed)
    }

    async fn download_shard(&self, shard: &ModelShard, output_path: &Path) -> Result<(), StorageError> {
        ensure_running()?;
        info!(
            "downloading shard from {}: model={}, shard={}",
            self.backend_type(),
            shard.model_id,
            shard.shard_index
        );
        debug!("http storage timeout: {}s", self.timeout.as_secs());

        for (index, _) in self.mirror_urls.iter().enumerate() {
            ensure_running()?;
            let url = self.shard_url(shard, index);
            let response = match self.client.get(&url).send().await {
                Ok(response) => response,
                Err(err) => {
                    error!(
                        "storage operation failed: backend={}, error={}",
                        self.backend_type(),
                        err
                    );
                    continue;
                }
            };

            if !response.status().is_success() {
                continue;
            }

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            let mut output = fs::File::create(output_path).await?;
            let mut downloaded = 0u64;
            let total = response.content_length().unwrap_or(0);
            let mut stream = response.bytes_stream();
            let mut stream_failed = false;
            while let Some(chunk) = stream.next().await {
                ensure_running()?;
                match chunk {
                    Ok(chunk) => {
                        output.write_all(&chunk).await?;
                        downloaded += chunk.len() as u64;
                        debug!("download progress: {}/{} bytes", downloaded, total);
                    }
                    Err(err) => {
                        error!(
                            "storage operation failed: backend={}, error={}",
                            self.backend_type(),
                            err
                        );
                        stream_failed = true;
                        break;
                    }
                }
            }

            if stream_failed {
                let _ = fs::remove_file(output_path).await;
                continue;
            }

            output.flush().await?;
            output.sync_all().await?;

            if !verify_shard_integrity(output_path, &shard.hash).await? {
                let actual_hash = compute_file_hash(output_path).await?;
                warn!(
                    "shard integrity check failed: shard={}, expected={}, actual={}",
                    shard.shard_index,
                    hex::encode(shard.hash),
                    hex::encode(actual_hash)
                );
                let _ = fs::remove_file(output_path).await;
                continue;
            }

            return Ok(());
        }

        Err(StorageError::AllMirrorsFailed)
    }

    async fn verify_shard(&self, shard: &ModelShard) -> Result<bool, StorageError> {
        ensure_running()?;
        for (index, _) in self.mirror_urls.iter().enumerate() {
            let url = self.shard_url(shard, index);
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|err| StorageError::Http(err.to_string()))?;
            if !response.status().is_success() {
                continue;
            }
            let actual_hash = hash_response_body(response).await?;
            if actual_hash != shard.hash {
                return Err(StorageError::IntegrityFailed);
            }
            return Ok(true);
        }
        Ok(false)
    }

    async fn delete_shard(&self, shard: &ModelShard) -> Result<(), StorageError> {
        ensure_running()?;
        let mut success = false;
        for (index, _) in self.mirror_urls.iter().enumerate() {
            let url = self.shard_url(shard, index);
            match self.client.delete(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    success = true;
                }
                Ok(response) => {
                    error!(
                        "storage operation failed: backend={}, error={}",
                        self.backend_type(),
                        response.status()
                    );
                }
                Err(err) => {
                    error!(
                        "storage operation failed: backend={}, error={}",
                        self.backend_type(),
                        err
                    );
                }
            }
        }

        if success {
            Ok(())
        } else {
            Err(StorageError::AllMirrorsFailed)
        }
    }

    fn backend_type(&self) -> &str {
        "http"
    }
}
