use std::sync::Arc;

use blockchain_core::Blockchain;
use genesis::CeoSignature;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use tokio::sync::Mutex;
use tracing::warn;

use crate::rpc::types::{
    parse_hex_bytes, RpcError, ShutdownRequest, SipActionRequest, SipActionResponse,
    SipStatusResponse, SuccessResponse,
};

fn verify_ceo_request(message: &[u8], signature_hex: &str) -> Result<CeoSignature, RpcError> {
    use ed25519_dalek::Signature;

    let sig_bytes = parse_hex_bytes(signature_hex)?;
    if sig_bytes.len() != 64 {
        return Err(RpcError::InvalidParams(
            "signature must be 64 bytes".to_string(),
        ));
    }
    let sig = Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
    let ceo_sig = CeoSignature(sig);

    genesis::verify_ceo_signature(message, &ceo_sig).map_err(|_| RpcError::InvalidSignature)?;

    Ok(ceo_sig)
}

#[rpc(server)]
pub trait CeoRpc {
    #[method(name = "submitShutdown")]
    async fn submit_shutdown(&self, request: ShutdownRequest) -> Result<SuccessResponse, RpcError>;

    #[method(name = "approveSIP")]
    async fn approve_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError>;

    #[method(name = "vetoSIP")]
    async fn veto_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError>;

    #[method(name = "getSIPStatus")]
    async fn get_sip_status(&self, proposal_id: String) -> Result<SipStatusResponse, RpcError>;
}

#[derive(Clone)]
pub struct CeoRpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
}

impl CeoRpcMethods {
    pub fn new(blockchain: Arc<Mutex<Blockchain>>) -> Self {
        Self { blockchain }
    }
}

#[async_trait]
impl CeoRpcServer for CeoRpcMethods {
    async fn submit_shutdown(&self, request: ShutdownRequest) -> Result<SuccessResponse, RpcError> {
        let bc = self.blockchain.lock().await;
        let network_magic = bc.genesis_config.network_magic;

        let msg = format!(
            "shutdown:{}:{}:{}:{}",
            network_magic, request.timestamp, request.nonce, request.reason
        )
        .into_bytes();

        let ceo_sig = verify_ceo_request(&msg, &request.signature)?;
        let command = genesis::ShutdownCommand {
            timestamp: request.timestamp,
            reason: request.reason,
            ceo_signature: ceo_sig,
            nonce: request.nonce,
            network_magic,
        };

        genesis::emergency_shutdown(command).map_err(|e| {
            warn!(error = %e, "shutdown rejected");
            RpcError::Internal(e.to_string())
        })?;

        Ok(SuccessResponse {
            success: true,
            message: "Shutdown initiated".to_string(),
        })
    }

    async fn approve_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError> {
        use ed25519_dalek::Signature;

        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams(
                "signature must be 64 bytes".to_string(),
            ));
        }
        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
        let ceo_sig = genesis::CeoSignature(sig);

        genesis::approve_sip(&request.proposal_id, ceo_sig)
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(SipActionResponse {
            success: true,
            proposal_id: request.proposal_id,
        })
    }

    async fn veto_sip(&self, request: SipActionRequest) -> Result<SipActionResponse, RpcError> {
        use ed25519_dalek::Signature;

        let sig_bytes = parse_hex_bytes(&request.signature)?;
        if sig_bytes.len() != 64 {
            return Err(RpcError::InvalidParams(
                "signature must be 64 bytes".to_string(),
            ));
        }
        let sig = Signature::try_from(sig_bytes.as_slice())
            .map_err(|e| RpcError::InvalidParams(format!("invalid signature bytes: {e}")))?;
        let ceo_sig = genesis::CeoSignature(sig);

        genesis::veto_sip(&request.proposal_id, ceo_sig)
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(SipActionResponse {
            success: true,
            proposal_id: request.proposal_id,
        })
    }

    async fn get_sip_status(&self, proposal_id: String) -> Result<SipStatusResponse, RpcError> {
        let status = genesis::get_sip_status(&proposal_id)
            .ok_or_else(|| RpcError::InvalidParams("proposal not found".to_string()))?;

        let status_str = match status {
            genesis::SipStatus::Pending => "Pending",
            genesis::SipStatus::Approved => "Approved",
            genesis::SipStatus::Vetoed => "Vetoed",
            genesis::SipStatus::Deployed => "Deployed",
        }
        .to_string();

        Ok(SipStatusResponse {
            proposal_id,
            status: status_str,
        })
    }
}
