use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use genesis::is_shutdown;
use futures::StreamExt;
use libp2p::core::upgrade;
use libp2p::dns;
use libp2p::gossipsub;
use libp2p::identify;
use libp2p::kad;
use libp2p::multiaddr::Protocol;
use libp2p::noise;
use libp2p::ping;
use libp2p::request_response;
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::tcp;
use libp2p::yamux;
use libp2p::PeerId;
use libp2p::Transport;
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc};

use crate::events::NetworkEvent;
use crate::gossip::{
    topic_blocks, topic_model_announcements, topic_poi_proofs, topic_shutdown, topic_transactions,
    TOPIC_MODEL_ANNOUNCEMENTS, TOPIC_SHUTDOWN,
};
use crate::node_types::{NodeConfig, NodeType};
use crate::protocol::NetworkMessage;
use crate::reputation::PeerReputationManager;
use crate::reputation::FailureReason;
use crate::shutdown_propagation::handle_shutdown_message;
use crate::shutdown_propagation::ShutdownBroadcaster;
use crate::tensor_stream::{protocol as tensor_stream_protocol, TensorRequest, TensorResponse, TensorStreamCodec};
use crate::model_stream::{protocol as model_stream_protocol, ModelShardRequest, ModelShardResponse, ModelStreamCodec};
use crate::model_sync::ModelShardRequestEnvelope;
use crate::metrics::{NetworkMetrics, SharedNetworkMetrics};
use crate::tensor_stream::TENSOR_CHUNK_SIZE_BYTES;
use crate::config::NetworkConfig;
use crate::events::TensorChunk;

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "P2PBehaviourEvent")]
pub struct P2PBehaviour {
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub gossipsub: gossipsub::Behaviour,
    pub request_response: request_response::Behaviour<TensorStreamCodec>,
    pub model_stream: request_response::Behaviour<ModelStreamCodec>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
}

#[derive(Debug)]
pub enum P2PBehaviourEvent {
    Kademlia(kad::Event),
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<TensorRequest, TensorResponse>),
    ModelStream(request_response::Event<ModelShardRequest, ModelShardResponse>),
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<kad::Event> for P2PBehaviourEvent {
    fn from(event: kad::Event) -> Self {
        Self::Kademlia(event)
    }
}

impl From<gossipsub::Event> for P2PBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}

impl From<request_response::Event<TensorRequest, TensorResponse>> for P2PBehaviourEvent {
    fn from(event: request_response::Event<TensorRequest, TensorResponse>) -> Self {
        Self::RequestResponse(event)
    }
}

impl From<request_response::Event<ModelShardRequest, ModelShardResponse>> for P2PBehaviourEvent {
    fn from(event: request_response::Event<ModelShardRequest, ModelShardResponse>) -> Self {
        Self::ModelStream(event)
    }
}

impl From<identify::Event> for P2PBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ping::Event> for P2PBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

pub struct P2PNode {
    pub local_peer_id: PeerId,
    pub swarm: Swarm<P2PBehaviour>,
    pub reputation_manager: Arc<PeerReputationManager>,
    pub shutdown_rx: broadcast::Receiver<()>,
    pub event_tx: mpsc::Sender<NetworkEvent>,
    pub shutdown_active: bool,
    shutdown_pending: bool,
    outgoing_shutdown: VecDeque<(gossipsub::IdentTopic, Vec<u8>)>,
    outgoing_normal: VecDeque<(gossipsub::IdentTopic, Vec<u8>)>,
    metrics: SharedNetworkMetrics,
    proof_store: Arc<std::sync::Mutex<std::collections::HashMap<[u8; 32], Vec<u8>>>>,
    #[allow(clippy::type_complexity)]
    model_shard_store: Arc<std::sync::Mutex<std::collections::HashMap<(String, u32), Vec<u8>>>>,
    model_shard_counts: Arc<std::sync::Mutex<HashMap<String, u32>>>,
}

impl ShutdownBroadcaster for P2PNode {
    fn broadcast_shutdown_message(&mut self, msg: crate::ShutdownMessage) {
        self.broadcast_shutdown_signal(msg);
    }
}

impl P2PNode {
    pub fn new(
        keypair: libp2p::identity::Keypair,
        config: NetworkConfig,
        node_config: Option<NodeConfig>,
    ) -> Result<(Self, mpsc::Receiver<NetworkEvent>), crate::NetworkError> {
        let local_peer_id = PeerId::from(keypair.public());

        let noise_config = noise::Config::new(&keypair)
            .map_err(|e| crate::NetworkError::Serialization(e.to_string()))?;
        let yamux_config = yamux::Config::default();
        let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));
        let transport = dns::tokio::Transport::system(tcp_transport)
            .map_err(|e| crate::NetworkError::Serialization(e.to_string()))?
            .upgrade(upgrade::Version::V1)
            .authenticate(noise_config)
            .multiplex(yamux_config)
            .boxed();

        let store = kad::store::MemoryStore::new(local_peer_id);
        let mut kad_config = kad::Config::default();
        kad_config
            .set_kbucket_inserts(kad::BucketInserts::Manual)
            .set_query_timeout(Duration::from_secs(60));
        let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);

        for addr in config.bootstrap_peers.iter().cloned() {
            let mut peer_id = None;
            for proto in addr.iter() {
                if let Protocol::P2p(hash) = proto {
                    peer_id = Some(hash);
                }
            }
            if let Some(pid) = peer_id {
                kademlia.add_address(&pid, addr);
            }
        }

        let gossipsub_cfg = &config.gossipsub_config;
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_millis(gossipsub_cfg.heartbeat_ms))
            .mesh_n(gossipsub_cfg.mesh_n)
            .mesh_n_low(gossipsub_cfg.mesh_n_low)
            .mesh_n_high(gossipsub_cfg.mesh_n_high)
            .max_transmit_size(gossipsub_cfg.max_transmit_size)
            .duplicate_cache_time(Duration::from_secs(60))
            .message_id_fn(|message: &gossipsub::Message| {
                let mut hasher = DefaultHasher::new();
                message.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_string())
            })
            .build()
            .map_err(|e| crate::NetworkError::Serialization(e.to_string()))?;

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|e| crate::NetworkError::Serialization(e.to_string()))?;

        // Subscriptions are installed by higher-level handlers (gossip.rs) once integrated.

        let identify = identify::Behaviour::new(identify::Config::new(
            "aigen/identify/1.0.0".to_string(),
            keypair.public(),
        ));

        let ping = ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(10)));

        let rr_protocols = std::iter::once((tensor_stream_protocol(), request_response::ProtocolSupport::Full));
        let rr_config = request_response::Config::default();
        let request_response = request_response::Behaviour::<TensorStreamCodec>::new(rr_protocols, rr_config);

        let model_protocols = std::iter::once((model_stream_protocol(), request_response::ProtocolSupport::Full));
        let model_rr_config = request_response::Config::default();
        let model_stream = request_response::Behaviour::<ModelStreamCodec>::new(model_protocols, model_rr_config);

        let behaviour = P2PBehaviour {
            kademlia,
            gossipsub,
            request_response,
            model_stream,
            identify,
            ping,
        };

        let swarm_config = libp2p::swarm::Config::with_tokio_executor();
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id, swarm_config);
        swarm
            .listen_on(config.listen_addr)
            .map_err(|e| crate::NetworkError::Serialization(e.to_string()))?;

        let role = node_config.as_ref().map(|c| c.node_type).unwrap_or(NodeType::FullNode);
        match role {
            NodeType::Miner => {
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_transactions());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_poi_proofs());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_model_announcements());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_shutdown());
            }
            NodeType::Validator => {
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_blocks());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_poi_proofs());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_model_announcements());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_shutdown());
            }
            NodeType::LightClient => {
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_blocks());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_model_announcements());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_shutdown());
            }
            NodeType::FullNode => {
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_blocks());
                let _ = swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&topic_transactions());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_poi_proofs());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_model_announcements());
                let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_shutdown());
            }
        }

        let reputation_manager = Arc::new(PeerReputationManager::new(
            config.reputation_config.ban_threshold,
            Duration::from_secs(config.reputation_config.cleanup_interval_secs),
        ));
        let metrics = Arc::new(NetworkMetrics::default());
        let (shutdown_tx, shutdown_rx) = broadcast::channel(16);
        let _ = shutdown_tx;

        let (event_tx, event_rx) = mpsc::channel(1024);

        Ok((
            Self {
                local_peer_id,
                swarm,
                reputation_manager,
                shutdown_rx,
                event_tx,
                shutdown_active: false,
                shutdown_pending: false,
                outgoing_shutdown: VecDeque::new(),
                outgoing_normal: VecDeque::new(),
                metrics,
                proof_store: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
                model_shard_store: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
                model_shard_counts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            },
            event_rx,
        ))
    }

    pub fn insert_local_proof_bytes(&mut self, hash: [u8; 32], bytes: Vec<u8>) {
        if let Ok(mut store) = self.proof_store.lock() {
            store.insert(hash, bytes);
        }
    }

    pub fn insert_local_shard(&mut self, model_id: String, shard_index: u32, data: Vec<u8>) {
        if let Ok(mut store) = self.model_shard_store.lock() {
            store.insert((model_id, shard_index), data);
        }
    }

    pub fn insert_local_model_metadata(&mut self, model_id: String, total_shards: u32) {
        if let Ok(mut store) = self.model_shard_counts.lock() {
            store.insert(model_id, total_shards);
        }
    }

    pub fn metrics(&self) -> SharedNetworkMetrics {
        self.metrics.clone()
    }

    pub fn publish_message(&mut self, msg: NetworkMessage) {
        let payload = match msg.serialize() {
            Ok(p) => p,
            Err(_) => return,
        };

        self.metrics.inc_messages_sent();
        self.metrics.add_bytes_sent(payload.len() as u64);

        let (topic, is_shutdown) = match &msg {
            NetworkMessage::ShutdownSignal(_) => (topic_shutdown(), true),
            NetworkMessage::Block(_) => (topic_blocks(), false),
            NetworkMessage::Transaction(_) => (topic_transactions(), false),
            NetworkMessage::PoIProof(_) => (topic_poi_proofs(), false),
            NetworkMessage::ModelAnnouncement { .. } => {
                self.metrics.inc_model_announcements_sent();
                (topic_model_announcements(), false)
            }
            NetworkMessage::ModelQuery { .. } => (topic_model_announcements(), false),
            _ => (topic_transactions(), false),
        };

        if is_shutdown {
            self.shutdown_pending = true;
            self.outgoing_shutdown.push_back((topic, payload));
        } else {
            self.outgoing_normal.push_back((topic, payload));
        }
    }

    pub fn broadcast_shutdown_signal(&mut self, msg: crate::ShutdownMessage) {
        self.publish_message(NetworkMessage::ShutdownSignal(msg));
    }

    fn flush_outgoing(&mut self) {
        while let Some((topic, payload)) = self.outgoing_shutdown.pop_front() {
            let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, payload);
        }

        if self.shutdown_pending {
            return;
        }

        while let Some((topic, payload)) = self.outgoing_normal.pop_front() {
            let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, payload);
        }
    }

    pub async fn start(mut self) {
        loop {
            if self.shutdown_active || is_shutdown() {
                self.shutdown_active = true;
            }

            if self.shutdown_active {
                break;
            }

            self.flush_outgoing();

            tokio::select! {
                biased;
                _ = self.shutdown_rx.recv() => {
                    self.shutdown_active = true;
                    let _ = self.event_tx.send(NetworkEvent::ShutdownSignal).await;
                    break;
                }
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await;
                }
            }
        }
    }

    pub async fn start_with_publisher(
        mut self,
        mut publish_rx: mpsc::Receiver<NetworkMessage>,
        mut model_request_rx: mpsc::Receiver<ModelShardRequestEnvelope>,
    ) {
        loop {
            if self.shutdown_active || is_shutdown() {
                self.shutdown_active = true;
            }

            if self.shutdown_active {
                break;
            }

            self.flush_outgoing();

            tokio::select! {
                biased;
                _ = self.shutdown_rx.recv() => {
                    self.shutdown_active = true;
                    let _ = self.event_tx.send(NetworkEvent::ShutdownSignal).await;
                    break;
                }
                maybe_msg = publish_rx.recv() => {
                    if let Some(msg) = maybe_msg {
                        self.publish_message(msg);
                    }
                }
                maybe_request = model_request_rx.recv() => {
                    if let Some(envelope) = maybe_request {
                        self.request_model_shard(
                            envelope.peer,
                            envelope.request.model_id,
                            envelope.request.shard_index,
                        );
                    }
                }
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await;
                }
            }
        }
    }

    pub fn request_tensor(&mut self, peer: PeerId, poi_proof_hash: [u8; 32], chunk_index: u32) -> request_response::OutboundRequestId {
        let req = TensorRequest { poi_proof_hash, chunk_index };
        self.swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer, req)
    }

    pub fn request_model_shard(&mut self, peer: PeerId, model_id: String, shard_index: u32) -> request_response::OutboundRequestId {
        let req = ModelShardRequest { model_id, shard_index };
        self.swarm
            .behaviour_mut()
            .model_stream
            .send_request(&peer, req)
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<P2PBehaviourEvent>) {
        match event {
            SwarmEvent::ConnectionEstablished { .. } => {
                self.metrics.inc_peers_connected();
            }
            SwarmEvent::ConnectionClosed { .. } => {
                self.metrics.dec_peers_connected();
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. })) => {
                let _ = self.event_tx.send(NetworkEvent::PeerDiscovered(peer)).await;
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                self.metrics.inc_messages_received();
                self.metrics.add_bytes_received(message.data.len() as u64);

                let source = message.source;
                if let Some(peer) = source {
                    if self.reputation_manager.is_banned(&peer) {
                        return;
                    }
                }

                if message.topic.as_str() == TOPIC_SHUTDOWN {
                    if let Ok(NetworkMessage::ShutdownSignal(shutdown_msg)) =
                        NetworkMessage::deserialize(&message.data)
                    {
                        if handle_shutdown_message(shutdown_msg).is_ok() {
                            self.shutdown_active = true;
                            self.shutdown_pending = true;
                            let _ = self.event_tx.send(NetworkEvent::ShutdownSignal).await;
                            if let Some(peer) = source {
                                self.reputation_manager.record_success(peer);
                            }
                        }
                    }
                    return;
                }

                if message.topic.as_str() == TOPIC_MODEL_ANNOUNCEMENTS {
                    match NetworkMessage::deserialize(&message.data) {
                        Ok(NetworkMessage::ModelAnnouncement { model_id, shard_index, .. }) => {
                            let event = NetworkEvent::ModelShardAnnounced {
                                model_id,
                                shard_index,
                                peer: source,
                            };
                            let _ = self.event_tx.send(event).await;
                            self.metrics.inc_model_announcements_received();
                            if let Some(peer) = source {
                                self.reputation_manager.record_success(peer);
                            }
                        }
                        Ok(NetworkMessage::ModelQuery { model_id }) => {
                            if let Some(peer) = source {
                                let _ = self
                                    .event_tx
                                    .send(NetworkEvent::ModelQueryReceived { model_id, peer })
                                    .await;
                                self.reputation_manager.record_success(peer);
                            }
                        }
                        _ => {
                            if let Some(peer) = source {
                                let was_banned = self.reputation_manager.is_banned(&peer);
                                self.reputation_manager.record_failure(peer, FailureReason::MalformedMessage);
                                if !was_banned && self.reputation_manager.is_banned(&peer) {
                                    self.metrics.inc_reputation_bans();
                                }
                            }
                        }
                    }
                    return;
                }

                match NetworkMessage::deserialize(&message.data) {
                    Ok(msg) => {
                        let _ = self.event_tx.send(NetworkEvent::MessageReceived(msg)).await;
                        if let Some(peer) = source {
                            self.reputation_manager.record_success(peer);
                        }
                    }
                    Err(_) => {
                        if let Some(peer) = source {
                            let was_banned = self.reputation_manager.is_banned(&peer);
                            self.reputation_manager.record_failure(peer, FailureReason::MalformedMessage);
                            if !was_banned && self.reputation_manager.is_banned(&peer) {
                                self.metrics.inc_reputation_bans();
                            }
                        }
                    }
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::RequestResponse(rr_event)) => {
                match rr_event {
                    request_response::Event::Message { peer, message } => {
                        if self.reputation_manager.is_banned(&peer) {
                            return;
                        }

                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                let bytes = self
                                    .proof_store
                                    .lock()
                                    .ok()
                                    .and_then(|s| s.get(&request.poi_proof_hash).cloned());

                                if bytes.is_none() {
                                    let was_banned = self.reputation_manager.is_banned(&peer);
                                    self.reputation_manager.record_failure(peer, FailureReason::Timeout);
                                    if !was_banned && self.reputation_manager.is_banned(&peer) {
                                        self.metrics.inc_reputation_bans();
                                    }
                                    return;
                                }
                                let bytes = bytes.unwrap();

                                let total_chunks = bytes.len().div_ceil(TENSOR_CHUNK_SIZE_BYTES) as u32;
                                let start = request.chunk_index as usize * TENSOR_CHUNK_SIZE_BYTES;
                                if start >= bytes.len() {
                                    let was_banned = self.reputation_manager.is_banned(&peer);
                                    self.reputation_manager.record_failure(peer, FailureReason::MalformedMessage);
                                    if !was_banned && self.reputation_manager.is_banned(&peer) {
                                        self.metrics.inc_reputation_bans();
                                    }
                                    return;
                                }
                                let end = (start + TENSOR_CHUNK_SIZE_BYTES).min(bytes.len());
                                let chunk = bytes[start..end].to_vec();

                                let response = TensorResponse {
                                    request_id: 0,
                                    chunk_index: request.chunk_index,
                                    total_chunks,
                                    data: chunk,
                                    compression: consensus::CompressionMethod::None,
                                    original_size: bytes.len() as u64,
                                };

                                let _ = self
                                    .swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_response(channel, response);
                                self.reputation_manager.record_success(peer);
                            }
                            request_response::Message::Response { response, .. } => {
                                let chunk = TensorChunk {
                                    request_id: response.request_id,
                                    chunk_index: response.chunk_index,
                                    total_chunks: response.total_chunks,
                                    data: response.data,
                                    compression: response.compression,
                                };

                                let _ = self.event_tx.send(NetworkEvent::TensorChunkReceived(chunk)).await;
                                self.reputation_manager.record_success(peer);
                            }
                        }
                    }
                    request_response::Event::OutboundFailure { peer, .. } => {
                        let was_banned = self.reputation_manager.is_banned(&peer);
                        self.reputation_manager.record_failure(peer, FailureReason::Timeout);
                        if !was_banned && self.reputation_manager.is_banned(&peer) {
                            self.metrics.inc_reputation_bans();
                        }
                    }
                    request_response::Event::InboundFailure { peer, .. } => {
                        let was_banned = self.reputation_manager.is_banned(&peer);
                        self.reputation_manager.record_failure(peer, FailureReason::MalformedMessage);
                        if !was_banned && self.reputation_manager.is_banned(&peer) {
                            self.metrics.inc_reputation_bans();
                        }
                    }
                    _ => {}
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::ModelStream(ms_event)) => {
                match ms_event {
                    request_response::Event::Message { peer, message } => {
                        if self.reputation_manager.is_banned(&peer) {
                            return;
                        }

                        match message {
                            request_response::Message::Request { request, channel, .. } => {
                                let shard_data = self
                                    .model_shard_store
                                    .lock()
                                    .ok()
                                    .and_then(|s| s.get(&(request.model_id.clone(), request.shard_index)).cloned());
                                let total_shards = self
                                    .model_shard_counts
                                    .lock()
                                    .ok()
                                    .and_then(|s| s.get(&request.model_id).copied());

                                if let (Some(data), Some(total_shards)) = (shard_data, total_shards) {
                                    let hash = compute_shard_hash(&data);
                                    let response = ModelShardResponse {
                                        model_id: request.model_id,
                                        shard_index: request.shard_index,
                                        total_shards,
                                        data: data.clone(),
                                        hash,
                                        size: data.len() as u64,
                                    };
                                    let _ = self.swarm.behaviour_mut().model_stream.send_response(channel, response);
                                    self.reputation_manager.record_success(peer);
                                    self.metrics.inc_model_shards_sent();
                                    self.metrics.add_model_bytes_sent(data.len() as u64);
                                }
                            }
                            request_response::Message::Response { response, .. } => {
                                let actual_hash = compute_shard_hash(&response.data);
                                if actual_hash != response.hash {
                                    let was_banned = self.reputation_manager.is_banned(&peer);
                                    self.reputation_manager.record_failure(peer, FailureReason::ModelHashMismatch);
                                    if !was_banned && self.reputation_manager.is_banned(&peer) {
                                        self.metrics.inc_reputation_bans();
                                    }
                                    self.metrics.inc_model_transfer_failures();
                                    return;
                                }

                                let ModelShardResponse {
                                    model_id,
                                    shard_index,
                                    data,
                                    hash,
                                    size,
                                    ..
                                } = response;
                                let event = NetworkEvent::ModelShardReceived {
                                    model_id,
                                    shard_index,
                                    data,
                                    hash,
                                };
                                let _ = self.event_tx.send(event).await;
                                self.reputation_manager.record_success(peer);
                                self.metrics.inc_model_shards_received();
                                self.metrics.add_model_bytes_received(size);
                            }
                        }
                    }
                    request_response::Event::OutboundFailure { peer, .. } => {
                        let was_banned = self.reputation_manager.is_banned(&peer);
                        self.reputation_manager.record_failure(peer, FailureReason::ModelTransferTimeout);
                        if !was_banned && self.reputation_manager.is_banned(&peer) {
                            self.metrics.inc_reputation_bans();
                        }
                        self.metrics.inc_model_transfer_failures();
                    }
                    request_response::Event::InboundFailure { peer, .. } => {
                        let was_banned = self.reputation_manager.is_banned(&peer);
                        self.reputation_manager.record_failure(peer, FailureReason::InvalidModelShard);
                        if !was_banned && self.reputation_manager.is_banned(&peer) {
                            self.metrics.inc_reputation_bans();
                        }
                        self.metrics.inc_model_transfer_failures();
                    }
                    _ => {}
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Identify(_)) => {}
            SwarmEvent::Behaviour(P2PBehaviourEvent::Ping(_)) => {}
            _ => {}
        }
    }
}

fn compute_shard_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&digest);
    hash
}
