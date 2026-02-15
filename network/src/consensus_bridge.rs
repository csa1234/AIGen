// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use blockchain_core::Block;
use consensus::{PoIConsensus, PoIProof, ValidatorVote, verify_poi_proof};

use crate::events::NetworkEvent;
use crate::p2p::P2PNode;
use crate::protocol::NetworkMessage;

pub struct ConsensusBridge {
    pub consensus: Arc<Mutex<PoIConsensus>>,
    pub p2p_node: Arc<Mutex<P2PNode>>,
    pub event_rx: mpsc::Receiver<NetworkEvent>,
    pub poi_proof_tx: Option<mpsc::Sender<PoIProof>>,
    pub block_tx: Option<mpsc::Sender<Block>>,
}

impl ConsensusBridge {
    pub async fn run(mut self) {
        while let Some(ev) = self.event_rx.recv().await {
            match ev {
                NetworkEvent::MessageReceived(msg) => {
                    self.handle_network_message(msg).await;
                }
                NetworkEvent::ShutdownSignal => {
                    if let Ok(consensus) = self.consensus.try_lock() {
                        consensus.start_shutdown_monitor();
                    }
                    break;
                }
                _ => {}
            }
        }
    }

    async fn handle_network_message(&mut self, msg: NetworkMessage) {
        match msg {
            NetworkMessage::PoIProof(proof) => {
                // Verify the proof (cheap gating) and then forward it to the node role that owns
                // submission context (txs/prev hash/height) to trigger block production.
                if verify_poi_proof(&proof).is_ok() {
                    if let Some(tx) = &self.poi_proof_tx {
                        let _ = tx.try_send(proof);
                    }
                }
            }
            NetworkMessage::ValidatorVote(vote) => {
                let consensus = self.consensus.lock().await;
                let _ = consensus.submit_validator_vote(vote);
            }
            NetworkMessage::Block(block) => {
                // Forward blocks to the local chain applier (blockchain-core / coordination layer).
                if let Some(tx) = &self.block_tx {
                    let _ = tx.try_send(block);
                }
            }
            _ => {}
        }
    }
}

pub fn wire_consensus_network_publisher(
    _consensus: &mut PoIConsensus,
    mut rx: mpsc::Receiver<consensus::NetworkMessage>,
    p2p_node: Arc<Mutex<P2PNode>>,
) {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                consensus::NetworkMessage::Block(block) => {
                    let mut node = p2p_node.lock().await;
                    node.publish_message(NetworkMessage::Block(block));
                }
                consensus::NetworkMessage::ValidatorVote(vote) => {
                    let mut node = p2p_node.lock().await;
                    node.publish_message(NetworkMessage::ValidatorVote(vote));
                }
            }
        }
    });
}

pub fn _dummy_types_to_satisfy_compiler(_proof: PoIProof, _vote: ValidatorVote) {}
