//! Networking layer with priority shutdown signal propagation.

pub mod shutdown_propagation;
pub mod p2p;
pub mod protocol;
pub mod events;
pub mod reputation;
pub mod tensor_stream;
pub mod model_stream;
pub mod model_sync;
pub mod gossip;
pub mod discovery;
pub mod metrics;
pub mod consensus_bridge;
pub mod config;
pub mod node_types;

pub use crate::shutdown_propagation::{
    ShutdownMessage,
    ShutdownPropagationConfig,
    NetworkError,
    broadcast_shutdown,
    handle_shutdown_message,
};
pub use crate::p2p::P2PNode;
pub use crate::protocol::NetworkMessage;
pub use crate::events::{NetworkEvent, TensorChunk};
pub use crate::model_stream::{ModelShardRequest, ModelShardResponse, ModelStreamCodec};
pub use crate::model_sync::{ModelSyncManager, ShardAvailability};
