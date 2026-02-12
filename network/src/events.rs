// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use consensus::CompressionMethod;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};

use crate::protocol::NetworkMessage;

#[derive(Clone, Debug)]
pub enum NetworkEvent {
    PeerDiscovered(PeerId),
    MessageReceived(NetworkMessage),
    TensorChunkReceived(TensorChunk),
    ModelShardAnnounced {
        model_id: String,
        shard_index: u32,
        peer: Option<PeerId>,
    },
    ModelShardReceived {
        model_id: String,
        shard_index: u32,
        data: Vec<u8>,
        hash: [u8; 32],
    },
    ModelFragmentReceived {
        model_id: String,
        fragment_index: u32,
        data: Vec<u8>,
        hash: [u8; 32],
        size: u64,
        is_compressed: bool,
    },
    ModelQueryReceived {
        model_id: String,
        peer: PeerId,
    },
    VramCapabilityReceived {
        node_id: String,
        peer: Option<PeerId>,
        vram_total_gb: f32,
        vram_free_gb: f32,
        vram_allocated_gb: f32,
        capabilities: crate::protocol::NodeCapabilities,
        timestamp: i64,
    },
    ShutdownSignal,
    ReputationUpdate(PeerId, i32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensorChunk {
    pub request_id: u64,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
    pub compression: CompressionMethod,
}
