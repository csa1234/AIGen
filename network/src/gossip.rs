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

/// GossipManager for handling heartbeat and checkpoint propagation
pub struct GossipManager {
    local_peer_id: libp2p::PeerId,
    heartbeat_topic: Topic,
    checkpoint_topic: Topic,
    failover_topic: Topic,
}

impl GossipManager {
    pub fn new(local_peer_id: libp2p::PeerId) -> Self {
        Self {
            local_peer_id,
            heartbeat_topic: topic_heartbeat(),
            checkpoint_topic: topic_checkpoint(),
            failover_topic: topic_failover(),
        }
    }

    /// Subscribe to heartbeat and checkpoint topics
    pub fn subscribe_topics(
        &self,
        gossipsub: &mut libp2p::gossipsub::Behaviour,
    ) -> Result<(), libp2p::gossipsub::SubscriptionError> {
        gossipsub.subscribe(&self.heartbeat_topic)?;
        gossipsub.subscribe(&self.checkpoint_topic)?;
        gossipsub.subscribe(&self.failover_topic)?;
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

    /// Get local peer ID
    pub fn local_peer_id(&self) -> libp2p::PeerId {
        self.local_peer_id
    }
}
