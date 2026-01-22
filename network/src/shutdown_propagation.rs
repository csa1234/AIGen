use genesis::{
    emergency_shutdown, verify_ceo_signature, CeoSignature, GenesisConfig, GenesisError,
    ShutdownCommand,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("genesis error: {0}")]
    Genesis(#[from] GenesisError),
    #[error("serialization error: {0}")]
    Serialization(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShutdownMessage {
    pub command: ShutdownCommand,
    pub signature: CeoSignature,
    pub timestamp: i64,
    pub network_magic: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShutdownPropagationConfig {
    pub retry_attempts: u32,
    pub timeout_ms: u64,
}

pub trait ShutdownBroadcaster {
    fn broadcast_shutdown_message(&mut self, msg: ShutdownMessage);
}

pub async fn broadcast_shutdown(
    command: ShutdownCommand,
    _config: ShutdownPropagationConfig,
    broadcaster: Option<&mut dyn ShutdownBroadcaster>,
) -> Result<(), NetworkError> {
    // Verify CEO signature before broadcasting
    verify_ceo_signature(&command.message_to_sign(), &command.ceo_signature)?;

    let expected_magic = GenesisConfig::default().network_magic;
    if command.network_magic != expected_magic {
        return Err(NetworkError::Genesis(GenesisError::InvalidNetworkMagic));
    }

    let msg = ShutdownMessage {
        command: command.clone(),
        signature: command.ceo_signature.clone(),
        timestamp: command.timestamp,
        network_magic: expected_magic,
    };

    // Broadcast to the network (best-effort) before triggering local shutdown.
    if let Some(b) = broadcaster {
        b.broadcast_shutdown_message(msg.clone());
    }

    emergency_shutdown(command)?;

    Ok(())
}

pub fn handle_shutdown_message(msg: ShutdownMessage) -> Result<(), NetworkError> {
    let expected_magic = GenesisConfig::default().network_magic;
    if msg.network_magic != expected_magic {
        return Err(NetworkError::Genesis(GenesisError::InvalidNetworkMagic));
    }
    verify_ceo_signature(&msg.command.message_to_sign(), &msg.signature)?;
    emergency_shutdown(msg.command)?;
    Ok(())
}
