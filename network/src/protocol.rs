use crate::shutdown_propagation::ShutdownMessage;
use blockchain_core::{Block, Transaction};
use consensus::{PoIProof, ValidatorVote};
use serde::{Deserialize, Serialize};

use crate::shutdown_propagation::NetworkError;

use crate::events::TensorChunk;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    ShutdownSignal(ShutdownMessage),
    Block(Block),
    Transaction(Transaction),
    PoIProof(PoIProof),
    ValidatorVote(ValidatorVote),
    ModelShardRequest { model_id: String, shard_index: u32 },
    ModelShardResponse { model_id: String, shard_index: u32, data: Vec<u8>, hash: [u8; 32] },
    ModelAnnouncement { model_id: String, shard_index: u32, node_id: String },
    ModelQuery { model_id: String },
    TensorRequest { poi_proof_hash: [u8; 32], chunk_index: u32 },
    TensorResponse(TensorChunk),
    Ping,
    Pong,
}

impl NetworkMessage {
    pub fn priority(&self) -> u8 {
        match self {
            NetworkMessage::ShutdownSignal(_) => 255,
            NetworkMessage::ValidatorVote(_) => 200,
            NetworkMessage::Block(_) => 100,
            NetworkMessage::ModelAnnouncement { .. } => 80,
            NetworkMessage::ModelQuery { .. } => 80,
            NetworkMessage::ModelShardRequest { .. } => 85,
            NetworkMessage::ModelShardResponse { .. } => 85,
            NetworkMessage::PoIProof(_) => 75,
            NetworkMessage::Transaction(_) => 50,
            NetworkMessage::TensorRequest { .. } => 150,
            NetworkMessage::TensorResponse(_) => 150,
            NetworkMessage::Ping | NetworkMessage::Pong => 10,
        }
    }

    pub fn serialize(&self) -> Result<Vec<u8>, NetworkError> {
        serde_json::to_vec(self).map_err(|e| NetworkError::Serialization(e.to_string()))
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, NetworkError> {
        serde_json::from_slice(bytes).map_err(|e| NetworkError::Serialization(e.to_string()))
    }
}
