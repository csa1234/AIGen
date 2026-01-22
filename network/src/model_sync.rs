use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use genesis::check_shutdown;
use libp2p::PeerId;
use tokio::sync::mpsc;

use model::{LocalStorage, ModelError, ModelRegistry, ModelShard, ShardLocation, StorageBackend, StorageError};
use model::sharding::verify_shard_integrity;
use sha2::{Digest, Sha256};

use crate::events::NetworkEvent;
use crate::metrics::SharedNetworkMetrics;
use crate::model_stream::{ModelShardRequest, ModelShardResponse};
use crate::protocol::NetworkMessage;
use crate::reputation::{FailureReason, PeerReputationManager};

const MAX_CONCURRENT_DOWNLOADS: usize = 3;
const DEFAULT_TARGET_REDUNDANCY: u8 = 3;

#[derive(Debug, thiserror::Error)]
pub enum ModelSyncError {
    #[error("shutdown active")]
    Shutdown,
    #[error("registry error: {0}")]
    Registry(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("no available peers for shard {0}:{1}")]
    NoAvailablePeers(String, u32),
}

impl From<ModelError> for ModelSyncError {
    fn from(err: ModelError) -> Self {
        ModelSyncError::Registry(err.to_string())
    }
}

impl From<StorageError> for ModelSyncError {
    fn from(err: StorageError) -> Self {
        ModelSyncError::Storage(err.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct ShardPeerInfo {
    pub peer_id: PeerId,
    pub announced_at: Instant,
    pub reputation_score: f64,
    pub last_verified: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct ShardAvailability {
    pub model_id: String,
    pub shard_index: u32,
    pub available_peers: DashMap<PeerId, ShardPeerInfo>,
    pub last_updated: Arc<RwLock<Instant>>,
}

impl ShardAvailability {
    pub fn new(model_id: String, shard_index: u32) -> Self {
        Self {
            model_id,
            shard_index,
            available_peers: DashMap::new(),
            last_updated: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub fn touch(&self) {
        if let Ok(mut guard) = self.last_updated.write() {
            *guard = Instant::now();
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadTask {
    pub model_id: String,
    pub shard_index: u32,
    pub priority: u8,
    pub attempts: u8,
    pub enqueued_at: Instant,
}

#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub peer: PeerId,
    pub started_at: Instant,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ModelShardRequestEnvelope {
    pub peer: PeerId,
    pub request: ModelShardRequest,
}

#[derive(Clone)]
pub struct ModelSyncManager {
    pub registry: Arc<ModelRegistry>,
    pub local_storage: Arc<LocalStorage>,
    pub shard_availability: Arc<DashMap<(String, u32), ShardAvailability>>,
    pub download_queue: Arc<RwLock<VecDeque<DownloadTask>>>,
    pub active_downloads: Arc<DashMap<(String, u32), DownloadProgress>>,
    pub reputation_manager: Arc<PeerReputationManager>,
    pub metrics: SharedNetworkMetrics,
    pub publish_tx: mpsc::Sender<NetworkMessage>,
    pub request_tx: mpsc::Sender<ModelShardRequestEnvelope>,
    pub event_tx: mpsc::Sender<NetworkEvent>,
    pub local_node_id: String,
}

impl ModelSyncManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ModelRegistry>,
        local_storage: Arc<LocalStorage>,
        reputation_manager: Arc<PeerReputationManager>,
        metrics: SharedNetworkMetrics,
        publish_tx: mpsc::Sender<NetworkMessage>,
        request_tx: mpsc::Sender<ModelShardRequestEnvelope>,
        event_tx: mpsc::Sender<NetworkEvent>,
        local_node_id: String,
    ) -> Self {
        Self {
            registry,
            local_storage,
            shard_availability: Arc::new(DashMap::new()),
            download_queue: Arc::new(RwLock::new(VecDeque::new())),
            active_downloads: Arc::new(DashMap::new()),
            reputation_manager,
            metrics,
            publish_tx,
            request_tx,
            event_tx,
            local_node_id,
        }
    }

    pub async fn announce_local_shards(&self) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let models = self.registry.list_models()?;
        for model in models {
            let shards = self.registry.list_shards(&model.model_id)?;
            for shard in shards {
                let message = NetworkMessage::ModelAnnouncement {
                    model_id: shard.model_id.clone(),
                    shard_index: shard.shard_index,
                    node_id: self.local_node_id.clone(),
                };
                self.publish_tx
                    .send(message)
                    .await
                    .map_err(|_| ModelSyncError::ChannelClosed)?;
                self.metrics.inc_model_announcements_sent();
            }
        }
        Ok(())
    }

    pub async fn query_model_shards(&self, model_id: &str) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let message = NetworkMessage::ModelQuery {
            model_id: model_id.to_string(),
        };
        self.publish_tx
            .send(message)
            .await
            .map_err(|_| ModelSyncError::ChannelClosed)?;
        Ok(())
    }

    pub fn handle_model_announcement(&self, model_id: String, shard_index: u32, peer: PeerId) {
        let key = (model_id.clone(), shard_index);
        let availability = self
            .shard_availability
            .entry(key.clone())
            .or_insert_with(|| ShardAvailability::new(model_id.clone(), shard_index));

        let info = ShardPeerInfo {
            peer_id: peer,
            announced_at: Instant::now(),
            reputation_score: self.peer_score(&peer),
            last_verified: None,
        };
        availability.available_peers.insert(peer, info);
        availability.touch();
        self.metrics.inc_model_announcements_received();

        let _ = self
            .event_tx
            .try_send(NetworkEvent::ModelShardAnnounced {
                model_id,
                shard_index,
                peer: Some(peer),
            });
    }

    pub async fn handle_model_query(&self, model_id: &str) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let shards = self.registry.list_shards(model_id)?;
        for shard in shards {
            let message = NetworkMessage::ModelAnnouncement {
                model_id: shard.model_id.clone(),
                shard_index: shard.shard_index,
                node_id: self.local_node_id.clone(),
            };
            self.publish_tx
                .send(message)
                .await
                .map_err(|_| ModelSyncError::ChannelClosed)?;
            self.metrics.inc_model_announcements_sent();
        }
        Ok(())
    }

    pub fn request_shard_download(&self, model_id: String, shard_index: u32) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let priority = self
            .registry
            .get_model(&model_id)
            .map(|meta| if meta.is_core_model { 100 } else { 50 })
            .unwrap_or(50);
        let task = DownloadTask {
            model_id,
            shard_index,
            priority,
            attempts: 0,
            enqueued_at: Instant::now(),
        };
        if let Ok(mut queue) = self.download_queue.write() {
            queue.push_back(task);
        }
        Ok(())
    }

    pub async fn process_download_queue(&self) -> Result<(), ModelSyncError> {
        ensure_running()?;
        while self.active_downloads.len() < MAX_CONCURRENT_DOWNLOADS {
            let task = self.dequeue_next_task();
            let Some(task) = task else { break; };
            let peer = self
                .select_best_peer_for_shard(&task.model_id, task.shard_index)
                .ok_or_else(|| ModelSyncError::NoAvailablePeers(task.model_id.clone(), task.shard_index))?;
            self.active_downloads.insert(
                (task.model_id.clone(), task.shard_index),
                DownloadProgress {
                    peer,
                    started_at: Instant::now(),
                    bytes_received: 0,
                    total_bytes: None,
                },
            );
            self.download_shard_from_peer(peer, task.model_id.clone(), task.shard_index)
                .await?;
        }
        Ok(())
    }

    pub fn select_best_peer_for_shard(&self, model_id: &str, shard_index: u32) -> Option<PeerId> {
        let key = (model_id.to_string(), shard_index);
        let availability = self.shard_availability.get(&key)?;
        let mut best: Option<(PeerId, f64)> = None;
        for entry in availability.available_peers.iter() {
            let peer = *entry.key();
            if self.reputation_manager.is_banned(&peer) {
                continue;
            }
            let score = self.peer_score(&peer);
            let is_better = best
                .as_ref()
                .map(|(_, best_score)| score > *best_score)
                .unwrap_or(true);
            if is_better {
                best = Some((peer, score));
            }
        }
        best.map(|(peer, _)| peer)
    }

    pub async fn download_shard_from_peer(
        &self,
        peer: PeerId,
        model_id: String,
        shard_index: u32,
    ) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let envelope = ModelShardRequestEnvelope {
            peer,
            request: ModelShardRequest { model_id, shard_index },
        };
        self.request_tx
            .send(envelope)
            .await
            .map_err(|_| ModelSyncError::ChannelClosed)
    }

    pub async fn handle_received_shard(
        &self,
        peer: PeerId,
        response: ModelShardResponse,
    ) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let key = (response.model_id.clone(), response.shard_index);
        self.active_downloads.remove(&key);
        let hash = self.compute_hash(&response.data);
        if hash != response.hash {
            self.reputation_manager
                .record_failure(peer, FailureReason::ModelHashMismatch);
            self.metrics.inc_model_transfer_failures();
            return Err(ModelSyncError::Storage("hash mismatch".to_string()));
        }

        self.persist_downloaded_shard(&response).await?;
        self.reputation_manager.record_success(peer);
        self.metrics.inc_model_shards_received();
        self.metrics.add_model_bytes_received(response.data.len() as u64);
        Ok(())
    }

    pub async fn verify_downloaded_shard(&self, response: &ModelShardResponse) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let hash = self.compute_hash(&response.data);
        if hash != response.hash {
            return Err(ModelSyncError::Storage("hash mismatch".to_string()));
        }
        self.persist_downloaded_shard(response).await
    }

    pub fn handle_download_failure(
        &self,
        peer: PeerId,
        model_id: &str,
        shard_index: u32,
        reason: FailureReason,
    ) {
        self.active_downloads.remove(&(model_id.to_string(), shard_index));
        self.reputation_manager.record_failure(peer, reason);
        self.metrics.inc_model_transfer_failures();
        let mut task = DownloadTask {
            model_id: model_id.to_string(),
            shard_index,
            priority: 50,
            attempts: 1,
            enqueued_at: Instant::now(),
        };
        if let Ok(meta) = self.registry.get_model(model_id) {
            task.priority = if meta.is_core_model { 100 } else { 50 };
        }
        if let Ok(mut queue) = self.download_queue.write() {
            queue.push_back(task);
        }
    }

    pub async fn check_redundancy_levels(&self) -> Result<Vec<(String, u32)>, ModelSyncError> {
        ensure_running()?;
        let results = self
            .registry
            .get_shards_needing_replication(DEFAULT_TARGET_REDUNDANCY)?;
        Ok(results)
    }

    pub async fn replicate_shard(&self, model_id: &str, shard_index: u32) -> Result<(), ModelSyncError> {
        ensure_running()?;
        let message = NetworkMessage::ModelAnnouncement {
            model_id: model_id.to_string(),
            shard_index,
            node_id: self.local_node_id.clone(),
        };
        self.publish_tx
            .send(message)
            .await
            .map_err(|_| ModelSyncError::ChannelClosed)?;
        self.metrics.inc_model_announcements_sent();
        Ok(())
    }

    pub async fn balance_shard_distribution(&self) -> Result<Vec<(String, u32)>, ModelSyncError> {
        ensure_running()?;
        let under_replicated = self.check_redundancy_levels().await?;
        for (model_id, shard_index) in &under_replicated {
            let _ = self.replicate_shard(model_id, *shard_index).await;
        }
        Ok(under_replicated)
    }

    fn dequeue_next_task(&self) -> Option<DownloadTask> {
        let mut selected_index = None;
        if let Ok(queue) = self.download_queue.read() {
            let mut best_priority = None;
            for (idx, task) in queue.iter().enumerate() {
                if best_priority.map(|p| task.priority > p).unwrap_or(true) {
                    best_priority = Some(task.priority);
                    selected_index = Some(idx);
                }
            }
        }
        if let (Some(index), Ok(mut queue)) = (selected_index, self.download_queue.write()) {
            return queue.remove(index);
        }
        None
    }

    fn compute_hash(&self, data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);
        hash
    }

    async fn persist_downloaded_shard(&self, response: &ModelShardResponse) -> Result<(), ModelSyncError> {
        let metadata = self.registry.get_model(&response.model_id)?;
        let shard = ModelShard {
            model_id: response.model_id.clone(),
            shard_index: response.shard_index,
            total_shards: metadata.shard_count,
            hash: response.hash,
            size: response.size,
            ipfs_cid: None,
            http_urls: Vec::new(),
            locations: Vec::new(),
        };
        let path = self.local_storage.shard_path(&shard);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(StorageError::from)?;
        }
        tokio::fs::write(&path, &response.data).await.map_err(StorageError::from)?;
        let verified = verify_shard_integrity(&path, &shard.hash)
            .await
            .map_err(StorageError::from)?;
        if !verified {
            return Err(ModelSyncError::Storage("integrity check failed".to_string()));
        }
        self.registry.register_shard(shard.clone())?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ModelSyncError::Storage("invalid system time".to_string()))?
            .as_secs() as i64;
        let location = ShardLocation {
            node_id: self.local_node_id.clone(),
            backend_type: self.local_storage.backend_type().to_string(),
            location_uri: path.display().to_string(),
            last_verified: timestamp,
            is_healthy: true,
        };
        let _ = self
            .registry
            .register_shard_location(&response.model_id, response.shard_index, location);
        Ok(())
    }

    fn peer_score(&self, peer: &PeerId) -> f64 {
        self.reputation_manager
            .peers
            .get(peer)
            .map(|entry| entry.score)
            .unwrap_or(0.5)
    }
}

fn ensure_running() -> Result<(), ModelSyncError> {
    check_shutdown().map_err(|_| ModelSyncError::Shutdown)
}
