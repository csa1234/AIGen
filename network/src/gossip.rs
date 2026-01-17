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

pub struct GossipHandler {
    pub shutdown_queue: VecDeque<NetworkMessage>,
    pub normal_queue: VecDeque<NetworkMessage>,
}

impl Default for GossipHandler {
    fn default() -> Self {
        Self {
            shutdown_queue: VecDeque::new(),
            normal_queue: VecDeque::new(),
        }
    }
}

impl GossipHandler {
    pub fn enqueue(&mut self, msg: NetworkMessage) {
        if msg.priority() >= 255 {
            self.shutdown_queue.push_back(msg);
        } else {
            self.normal_queue.push_back(msg);
        }
    }

    pub fn pop_next(&mut self) -> Option<NetworkMessage> {
        self.shutdown_queue.pop_front().or_else(|| self.normal_queue.pop_front())
    }

    pub fn handle_gossip_message(&mut self, topic: &TopicHash, data: &[u8]) -> Option<NetworkMessage> {
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
