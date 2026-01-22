use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use libp2p::PeerId;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use genesis::config::GenesisConfig;
use genesis::shutdown::{emergency_shutdown, reset_shutdown_for_tests};
use genesis::types::{CeoSignature, ShutdownCommand};
use model::{LocalStorage, ModelMetadata, ModelRegistry, ModelShard};
use network::events::NetworkEvent;
use network::metrics::NetworkMetrics;
use network::model_sync::{ModelShardRequestEnvelope, ModelSyncError};
use network::reputation::{FailureReason, PeerReputationManager};
use network::{ModelShardResponse, ModelSyncManager};

const CEO_SECRET_KEY_BYTES: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

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

fn hash_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

fn register_model(
    registry: &ModelRegistry,
    model_id: &str,
    shard_data: &[Vec<u8>],
    is_core: bool,
) -> Vec<[u8; 32]> {
    let hashes: Vec<[u8; 32]> = shard_data.iter().map(|data| hash_bytes(data)).collect();
    let total_size = shard_data.iter().map(|data| data.len() as u64).sum();
    let metadata = ModelMetadata {
        model_id: model_id.to_string(),
        name: "test-model".to_string(),
        version: "0.1".to_string(),
        total_size,
        shard_count: shard_data.len() as u32,
        verification_hashes: hashes.clone(),
        is_core_model: is_core,
        minimum_tier: None,
        is_experimental: false,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs() as i64,
    };
    registry.register_model(metadata).expect("register model");
    hashes
}

fn build_shard(model_id: &str, shard_index: u32, total_shards: u32, hash: [u8; 32], size: u64) -> ModelShard {
    ModelShard {
        model_id: model_id.to_string(),
        shard_index,
        total_shards,
        hash,
        size,
        ipfs_cid: None,
        http_urls: Vec::new(),
        locations: Vec::new(),
    }
}

fn setup_manager() -> (
    ModelSyncManager,
    mpsc::Receiver<network::NetworkMessage>,
    mpsc::Receiver<ModelShardRequestEnvelope>,
    mpsc::Receiver<NetworkEvent>,
    Arc<ModelRegistry>,
    Arc<LocalStorage>,
    Arc<PeerReputationManager>,
    Arc<NetworkMetrics>,
) {
    reset_shutdown_for_tests();
    let registry = Arc::new(ModelRegistry::new());
    let local_storage = Arc::new(LocalStorage::new(temp_dir("model_sync")));
    let reputation_manager = Arc::new(PeerReputationManager::new(0.2, Duration::from_secs(60)));
    let metrics = Arc::new(NetworkMetrics::default());
    let (publish_tx, publish_rx) = mpsc::channel(32);
    let (request_tx, request_rx) = mpsc::channel(32);
    let (event_tx, event_rx) = mpsc::channel(32);

    let manager = ModelSyncManager::new(
        registry.clone(),
        local_storage.clone(),
        reputation_manager.clone(),
        metrics.clone(),
        publish_tx,
        request_tx,
        event_tx,
        "node-1".to_string(),
    );

    (
        manager,
        publish_rx,
        request_rx,
        event_rx,
        registry,
        local_storage,
        reputation_manager,
        metrics,
    )
}

fn build_shutdown_command(reason: &str) -> ShutdownCommand {
    let timestamp = 42;
    let nonce = 99;
    let network_magic = GenesisConfig::default().network_magic;
    let message = format!("shutdown:{}:{}:{}:{}", network_magic, timestamp, nonce, reason);
    let signing_key = SigningKey::from_bytes(&CEO_SECRET_KEY_BYTES);
    let signature = signing_key.sign(message.as_bytes());
    ShutdownCommand {
        timestamp,
        reason: reason.to_string(),
        ceo_signature: CeoSignature(signature),
        nonce,
        network_magic,
    }
}

#[tokio::test]
async fn shard_announcement_propagation() {
    let (manager, _publish_rx, _request_rx, mut event_rx, _registry, _storage, _reputation, metrics) =
        setup_manager();
    let peer = PeerId::random();

    manager.handle_model_announcement("alpha".to_string(), 2, peer);

    let availability = manager
        .shard_availability
        .get(&("alpha".to_string(), 2))
        .expect("availability");
    assert!(availability.available_peers.contains_key(&peer));

    let event = event_rx.recv().await.expect("event");
    assert!(matches!(
        event,
        NetworkEvent::ModelShardAnnounced { model_id, shard_index, peer: Some(p) }
            if model_id == "alpha" && shard_index == 2 && p == peer
    ));
    assert_eq!(metrics.model_announcements_received.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn shard_request_response_persists_data() {
    let (manager, _publish_rx, _request_rx, _event_rx, registry, storage, reputation, metrics) =
        setup_manager();
    let data = vec![1u8, 2, 3, 4, 5];
    let hashes = register_model(&registry, "model-a", &[data.clone()], true);
    let peer = PeerId::random();

    let response = ModelShardResponse {
        model_id: "model-a".to_string(),
        shard_index: 0,
        total_shards: 1,
        data: data.clone(),
        hash: hashes[0],
        size: data.len() as u64,
    };

    manager
        .handle_received_shard(peer, response)
        .await
        .expect("receive shard");

    let shard = registry.get_shard("model-a", 0).expect("registered shard");
    let path = storage.shard_path(&shard);
    assert!(tokio::fs::metadata(&path).await.is_ok());
    let score = reputation.peers.get(&peer).expect("reputation").score;
    assert!(score > 0.5);
    assert_eq!(metrics.model_shards_received.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(metrics.model_bytes_received.load(std::sync::atomic::Ordering::Relaxed), data.len() as u64);
}

#[tokio::test]
async fn hash_verification_failure_penalizes_peer() {
    let (manager, _publish_rx, _request_rx, _event_rx, registry, _storage, reputation, metrics) =
        setup_manager();
    let data = vec![9u8, 8, 7];
    let hashes = register_model(&registry, "model-b", &[data.clone()], false);
    let peer = PeerId::random();

    let mut bad_hash = hashes[0];
    bad_hash[0] ^= 0xFF;

    let response = ModelShardResponse {
        model_id: "model-b".to_string(),
        shard_index: 0,
        total_shards: 1,
        data,
        hash: bad_hash,
        size: 3,
    };

    let err = manager.handle_received_shard(peer, response).await.expect_err("hash mismatch");
    assert!(matches!(err, ModelSyncError::Storage(_)));
    let score = reputation.peers.get(&peer).expect("reputation").score;
    assert!(score < 0.5);
    assert_eq!(metrics.model_transfer_failures.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn peer_selection_prefers_high_reputation() {
    let (manager, _publish_rx, _request_rx, _event_rx, _registry, _storage, reputation, _metrics) =
        setup_manager();
    let high_peer = PeerId::random();
    let low_peer = PeerId::random();

    reputation.peers.insert(
        high_peer,
        network::reputation::PeerReputation {
            peer_id: high_peer,
            score: 0.9,
            successful_msgs: 0,
            failed_msgs: 0,
            last_seen: Instant::now(),
            is_banned: false,
        },
    );
    reputation.peers.insert(
        low_peer,
        network::reputation::PeerReputation {
            peer_id: low_peer,
            score: 0.4,
            successful_msgs: 0,
            failed_msgs: 0,
            last_seen: Instant::now(),
            is_banned: false,
        },
    );

    manager.handle_model_announcement("model-c".to_string(), 1, low_peer);
    manager.handle_model_announcement("model-c".to_string(), 1, high_peer);

    let selected = manager.select_best_peer_for_shard("model-c", 1).expect("peer");
    assert_eq!(selected, high_peer);
}

#[tokio::test]
async fn download_failure_requeues_task() {
    let (manager, _publish_rx, _request_rx, _event_rx, _registry, _storage, _reputation, metrics) =
        setup_manager();
    let peer = PeerId::random();

    manager.handle_download_failure(peer, "model-d", 0, FailureReason::ModelTransferTimeout);

    let queue = manager.download_queue.read().expect("queue");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].model_id, "model-d");
    assert_eq!(metrics.model_transfer_failures.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn redundancy_tracking_identifies_under_replicated_shards() {
    let (manager, _publish_rx, _request_rx, _event_rx, registry, _storage, _reputation, _metrics) =
        setup_manager();
    let data = vec![3u8, 4, 5, 6];
    let hashes = register_model(&registry, "model-e", &[data.clone()], false);
    let shard = build_shard("model-e", 0, 1, hashes[0], data.len() as u64);
    registry.register_shard(shard.clone()).expect("register shard");
    registry
        .register_shard_location(
            "model-e",
            0,
            model::ShardLocation {
                node_id: "node-1".to_string(),
                backend_type: "local".to_string(),
                location_uri: "local://node-1".to_string(),
                last_verified: 0,
                is_healthy: true,
            },
        )
        .expect("location");

    let under = manager.check_redundancy_levels().await.expect("redundancy");
    assert_eq!(under, vec![("model-e".to_string(), 0)]);
}

#[tokio::test]
async fn concurrent_downloads_respect_limit() {
    let (manager, _publish_rx, mut request_rx, _event_rx, registry, _storage, _reputation, _metrics) =
        setup_manager();
    let model_id = "model-f";
    let shard_data: Vec<Vec<u8>> = (0..5).map(|i| vec![i as u8; 4]).collect();
    register_model(&registry, model_id, &shard_data, false);

    let peer = PeerId::random();
    for shard_index in 0..5u32 {
        manager.handle_model_announcement(model_id.to_string(), shard_index, peer);
        manager
            .request_shard_download(model_id.to_string(), shard_index)
            .expect("queue");
    }

    manager.process_download_queue().await.expect("process queue");

    let mut requests = Vec::new();
    for _ in 0..3 {
        let envelope = request_rx.recv().await.expect("request");
        requests.push(envelope);
    }

    assert_eq!(requests.len(), 3);
    assert!(manager.active_downloads.len() <= 3);
}

#[tokio::test]
async fn announce_local_shards_updates_metrics() {
    let (manager, mut publish_rx, _request_rx, _event_rx, registry, _storage, _reputation, metrics) =
        setup_manager();
    let data = vec![6u8, 7, 8];
    let hashes = register_model(&registry, "model-g", &[data.clone()], false);
    let shard = build_shard("model-g", 0, 1, hashes[0], data.len() as u64);
    registry.register_shard(shard).expect("register shard");

    manager.announce_local_shards().await.expect("announce");

    let msg = publish_rx.recv().await.expect("publish message");
    assert!(matches!(msg, network::NetworkMessage::ModelAnnouncement { .. }));
    assert_eq!(metrics.model_announcements_sent.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn shutdown_prevents_operations() {
    let (manager, _publish_rx, _request_rx, _event_rx, _registry, _storage, _reputation, _metrics) =
        setup_manager();
    let command = build_shutdown_command("test shutdown");
    emergency_shutdown(command).expect("shutdown");

    let result = manager.request_shard_download("model-h".to_string(), 0);
    assert!(matches!(result, Err(ModelSyncError::Shutdown)));

    reset_shutdown_for_tests();
}
