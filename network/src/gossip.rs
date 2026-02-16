// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::collections::VecDeque;

use blockchain_core::{Block, Transaction};
use libp2p::gossipsub::{IdentTopic as Topic, TopicHash};

use crate::protocol::NetworkMessage;
use crate::shutdown_propagation::ShutdownMessage;

pub const TOPIC_BLOCKS: &str = "/aigen/blocks/1.0.0";
pub const TOPIC_TRANSACTIONS: &str = "/aigen/transactions/1.0.0";
pub const TOPIC_POI_PROOFS: &str = "/aigen/poi-proofs/1.0.0";
pub const TOPIC_MODEL_ANNOUNCEMENTS: &str = "/aigen/model-announcements/1.0.0";
pub const TOPIC_SHUTDOWN: &str = "/aigen/shutdown/1.0.0";
pub const TOPIC_VRAM_CAPABILITIES: &str = "/aigen/vram-capabilities/1.0.0";
pub const TOPIC_HEARTBEAT: &str = "/aigen/heartbeat/1.0.0";
pub const TOPIC_CHECKPOINT: &str = "/aigen/checkpoint/1.0.0";
pub const TOPIC_FAILOVER: &str = "/aigen/failover/1.0.0";
pub const TOPIC_BLOCK_PREFIX: &str = "/aigen/block/";
// Federated learning topics
pub const TOPIC_TRAINING_BURST: &str = "/aigen/training-burst/1.0.0";
pub const TOPIC_GLOBAL_DELTA: &str = "/aigen/global-delta/1.0.0";
pub const TOPIC_TRAINING_PROOF: &str = "/aigen/training-proof/1.0.0";

/// Generate topic for a specific block ID
pub fn topic_block(block_id: u32) -> Topic {
    Topic::new(format!("{}{}/1.0.0", TOPIC_BLOCK_PREFIX, block_id))
}

/// Parse block ID from topic string
pub fn parse_block_id(topic: &str) -> Option<u32> {
    topic
        .strip_prefix(TOPIC_BLOCK_PREFIX)
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.parse::<u32>().ok())
}

pub fn topic_blocks() -> Topic {
    Topic::new(TOPIC_BLOCKS)
}

pub fn topic_transactions() -> Topic {
    Topic::new(TOPIC_TRANSACTIONS)
}

pub fn topic_poi_proofs() -> Topic {
    Topic::new(TOPIC_POI_PROOFS)
}

pub fn topic_model_announcements() -> Topic {
    Topic::new(TOPIC_MODEL_ANNOUNCEMENTS)
}

pub fn topic_shutdown() -> Topic {
    Topic::new(TOPIC_SHUTDOWN)
}

pub fn topic_vram_capabilities() -> Topic {
    Topic::new(TOPIC_VRAM_CAPABILITIES)
}

#[derive(Default)]
pub struct GossipHandler {
    pub shutdown_queue: VecDeque<NetworkMessage>,
    pub normal_queue: VecDeque<NetworkMessage>,
}

impl GossipHandler {
    pub fn enqueue(&mut self, msg: NetworkMessage) {
        if msg.priority() == 255 {
            self.shutdown_queue.push_back(msg);
        } else {
            self.normal_queue.push_back(msg);
        }
    }

    pub fn pop_next(&mut self) -> Option<NetworkMessage> {
        self.shutdown_queue
            .pop_front()
            .or_else(|| self.normal_queue.pop_front())
    }

    pub fn handle_gossip_message(
        &mut self,
        topic: &TopicHash,
        data: &[u8],
    ) -> Option<NetworkMessage> {
        let msg = NetworkMessage::deserialize(data).ok()?;

        if topic.as_str() == TOPIC_SHUTDOWN {
            // Always prioritize shutdown.
            self.shutdown_queue.push_back(msg.clone());
            return Some(msg);
        }

        if topic.as_str() == TOPIC_MODEL_ANNOUNCEMENTS {
            self.normal_queue.push_back(msg.clone());
            return Some(msg);
        }

        self.normal_queue.push_back(msg.clone());
        Some(msg)
    }

    pub fn publish_block(block: Block) -> NetworkMessage {
        NetworkMessage::Block(block)
    }

    pub fn publish_transaction(tx: Transaction) -> NetworkMessage {
        NetworkMessage::Transaction(tx)
    }

    pub fn publish_shutdown(msg: ShutdownMessage) -> NetworkMessage {
        NetworkMessage::ShutdownSignal(msg)
    }
}

pub fn topic_heartbeat() -> Topic {
    Topic::new(TOPIC_HEARTBEAT)
}

pub fn topic_checkpoint() -> Topic {
    Topic::new(TOPIC_CHECKPOINT)
}

pub fn topic_failover() -> Topic {
    Topic::new(TOPIC_FAILOVER)
}

/// Federated learning topic helpers
pub fn topic_training_burst() -> Topic {
    Topic::new(TOPIC_TRAINING_BURST)
}

pub fn topic_global_delta() -> Topic {
    Topic::new(TOPIC_GLOBAL_DELTA)
}

pub fn topic_training_proof() -> Topic {
    Topic::new(TOPIC_TRAINING_PROOF)
}

/// GossipManager for handling heartbeat, checkpoint, and federated learning propagation
pub struct GossipManager {
    local_peer_id: libp2p::PeerId,
    heartbeat_topic: Topic,
    checkpoint_topic: Topic,
    failover_topic: Topic,
    training_burst_topic: Topic,
    global_delta_topic: Topic,
    training_proof_topic: Topic,
}

impl GossipManager {
    pub fn new(local_peer_id: libp2p::PeerId) -> Self {
        Self {
            local_peer_id,
            heartbeat_topic: topic_heartbeat(),
            checkpoint_topic: topic_checkpoint(),
            failover_topic: topic_failover(),
            training_burst_topic: topic_training_burst(),
            global_delta_topic: topic_global_delta(),
            training_proof_topic: topic_training_proof(),
        }
    }

    /// Subscribe to heartbeat, checkpoint, and federated learning topics
    pub fn subscribe_topics(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
    ) -> Result<(), libp2p::gossipsub::SubscriptionError> {
        gossipsub.subscribe(&self.heartbeat_topic)?;
        gossipsub.subscribe(&self.checkpoint_topic)?;
        gossipsub.subscribe(&self.failover_topic)?;
        gossipsub.subscribe(&self.training_burst_topic)?;
        gossipsub.subscribe(&self.global_delta_topic)?;
        gossipsub.subscribe(&self.training_proof_topic)?;
        Ok(())
    }

    /// Get the heartbeat topic
    pub fn heartbeat_topic(&self) -> Topic {
        self.heartbeat_topic.clone()
    }

    /// Get the checkpoint topic
    pub fn checkpoint_topic(&self) -> Topic {
        self.checkpoint_topic.clone()
    }

    /// Get the failover topic
    pub fn failover_topic(&self) -> Topic {
        self.failover_topic.clone()
    }

    /// Get the training burst topic
    pub fn training_burst_topic(&self) -> Topic {
        self.training_burst_topic.clone()
    }

    /// Get the global delta topic
    pub fn global_delta_topic(&self) -> Topic {
        self.global_delta_topic.clone()
    }

    /// Get the training proof topic
    pub fn training_proof_topic(&self) -> Topic {
        self.training_proof_topic.clone()
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> libp2p::PeerId {
        self.local_peer_id
    }

    /// Subscribe to a specific block topic
    pub fn subscribe_block(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
        block_id: u32,
    ) -> Result<bool, libp2p::gossipsub::SubscriptionError> {
        let topic = topic_block(block_id);
        gossipsub.subscribe(&topic)
    }

    /// Subscribe to multiple block topics
    pub fn subscribe_blocks(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
        block_ids: &[u32],
    ) -> Result<(), libp2p::gossipsub::SubscriptionError> {
        for &block_id in block_ids {
            self.subscribe_block(gossipsub, block_id)?;
        }
        Ok(())
    }

    /// Unsubscribe from a block topic
    pub fn unsubscribe_block(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
        block_id: u32,
    ) -> Result<bool, libp2p::gossipsub::PublishError> {
        let topic = topic_block(block_id);
        gossipsub.unsubscribe(&topic)
    }

    /// Unsubscribe from multiple block topics
    pub fn unsubscribe_blocks(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
        block_ids: &[u32],
    ) -> Result<bool, libp2p::gossipsub::PublishError> {
        for &block_id in block_ids {
            if !self.unsubscribe_block(gossipsub, block_id)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
