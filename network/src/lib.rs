//! Networking layer with priority shutdown signal propagation.

pub mod config;
pub mod consensus_bridge;
pub mod discovery;
pub mod events;
pub mod gossip;
pub mod metrics;
pub mod model_stream;
pub mod model_sync;
pub mod node_types;
pub mod p2p;
pub mod protocol;
pub mod reputation;
pub mod shutdown_propagation;
pub mod tensor_stream;

pub use crate::events::{NetworkEvent, TensorChunk};
pub use crate::model_stream::{ModelShardRequest, ModelShardResponse, ModelStreamCodec};
pub use crate::model_sync::{ModelSyncManager, ShardAvailability};
pub use crate::p2p::P2PNode;
pub use crate::protocol::NetworkMessage;
pub use crate::shutdown_propagation::{
    broadcast_shutdown, handle_shutdown_message, NetworkError, ShutdownMessage,
    ShutdownPropagationConfig,
};
