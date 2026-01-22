use std::sync::atomic::Ordering;
use std::sync::Arc;

use blockchain_core::types::{validate_address, BlockHash};
use blockchain_core::Blockchain;
use genesis;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use tokio::sync::Mutex;
use tracing::info;

use crate::rpc::types::{
    BalanceResponse, BlockResponse, ChainInfoResponse, HealthResponse, RpcError,
    SubmitTransactionResponse, TransactionResponse,
};

#[rpc(server)]
pub trait PublicRpc {
    #[method(name = "getBlock")]
    async fn get_block(&self, block_height: Option<u64>, block_hash: Option<String>) -> Result<Option<BlockResponse>, RpcError>;

    #[method(name = "getBalance")]
    async fn get_balance(&self, address: String) -> Result<BalanceResponse, RpcError>;

    #[method(name = "submitTransaction")]
    async fn submit_transaction(&self, transaction: TransactionResponse) -> Result<SubmitTransactionResponse, RpcError>;

    #[method(name = "getChainInfo")]
    async fn get_chain_info(&self) -> Result<ChainInfoResponse, RpcError>;

    #[method(name = "health")]
    async fn health(&self) -> Result<HealthResponse, RpcError>;

    #[method(name = "getPendingTransactions")]
    async fn get_pending_transactions(&self, limit: Option<usize>) -> Result<Vec<TransactionResponse>, RpcError>;
}

#[derive(Clone)]
pub struct RpcMethods {
    pub blockchain: Arc<Mutex<Blockchain>>,
    pub tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
    pub network_metrics: network::metrics::SharedNetworkMetrics,
}

impl RpcMethods {
    pub fn new(
        blockchain: Arc<Mutex<Blockchain>>,
        tx_broadcast: tokio::sync::broadcast::Sender<blockchain_core::Transaction>,
        network_metrics: network::metrics::SharedNetworkMetrics,
    ) -> Self {
        Self {
            blockchain,
            tx_broadcast,
            network_metrics,
        }
    }
}

#[async_trait]
impl PublicRpcServer for RpcMethods {
    async fn get_block(
        &self,
        block_height: Option<u64>,
        block_hash: Option<String>,
    ) -> Result<Option<BlockResponse>, RpcError> {
        let bc = self.blockchain.lock().await;

        let block = if let Some(h) = block_height {
            bc.blocks.get(h as usize)
        } else if let Some(hs) = block_hash {
            let parsed: BlockHash = hs
                .parse()
                .map_err(|e| RpcError::InvalidParams(format!("invalid block_hash: {e}")))?;
            bc.blocks.iter().find(|b| b.block_hash == parsed)
        } else {
            return Err(RpcError::InvalidParams(
                "must provide block_height or block_hash".to_string(),
            ));
        };

        let Some(block) = block else {
            return Err(RpcError::BlockNotFound);
        };

        Ok(Some(BlockResponse::from_block(block)))
    }

    async fn get_balance(&self, address: String) -> Result<BalanceResponse, RpcError> {
        validate_address(&address)
            .map_err(|e| RpcError::InvalidParams(format!("invalid address: {e}")))?;

        let bc = self.blockchain.lock().await;
        let bal = bc.get_balance(&address);
        let account = bc.get_account_state(&address);

        Ok(BalanceResponse {
            address,
            balance: bal,
            nonce: account.nonce.value(),
        })
    }

    async fn submit_transaction(
        &self,
        transaction: TransactionResponse,
    ) -> Result<SubmitTransactionResponse, RpcError> {
        use blockchain_core::crypto::PublicKey;
        use ed25519_dalek::VerifyingKey;

        let pk_hex = transaction
            .sender_public_key
            .clone()
            .ok_or_else(|| RpcError::InvalidParams("sender_public_key is required".to_string()))?;
        let pk_bytes = crate::rpc::types::parse_hex_bytes(&pk_hex)?;
        if pk_bytes.len() != 32 {
            return Err(RpcError::InvalidParams(
                "sender_public_key must be 32 bytes".to_string(),
            ));
        }
        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(&pk_bytes);
        let public_key = VerifyingKey::from_bytes(&pk_arr)
            .map_err(|_| RpcError::InvalidParams("invalid sender_public_key".to_string()))?;
        let public_key: PublicKey = public_key;

        let tx = transaction.into_tx()?;

        let derived_addr = blockchain_core::derive_address_from_pubkey(&public_key);
        if !derived_addr.eq_ignore_ascii_case(&tx.sender) {
            return Err(RpcError::InvalidParams(
                "sender does not match sender_public_key".to_string(),
            ));
        }

        tx.verify_with_pubkey(&public_key)
            .map_err(|_| RpcError::InvalidParams("invalid transaction signature".to_string()))?;
        let tx_hash = tx.tx_hash;

        let mut bc = self.blockchain.lock().await;
        bc.add_transaction_to_pool(tx.clone())
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        let _ = self.tx_broadcast.send(tx);

        info!(tx_hash = %tx_hash, "transaction submitted");

        Ok(SubmitTransactionResponse {
            tx_hash: format!("0x{}", hex::encode(tx_hash.0)),
            status: "accepted".to_string(),
        })
    }

    async fn get_chain_info(&self) -> Result<ChainInfoResponse, RpcError> {
        let bc = self.blockchain.lock().await;

        let height = bc.blocks.len().saturating_sub(1) as u64;
        let latest = bc
            .blocks
            .last()
            .map(|b| b.block_hash)
            .unwrap_or(blockchain_core::types::BlockHash([0u8; 32]));

        let snapshot = bc.state.snapshot();
        let total_supply = snapshot
            .accounts
            .values()
            .map(|a| a.balance.amount().value())
            .sum::<u64>();

        Ok(ChainInfoResponse {
            chain_id: bc.chain_id.value(),
            height,
            latest_block_hash: format!("0x{}", hex::encode(latest.0)),
            shutdown_status: bc.shutdown,
            total_supply,
        })
    }

    async fn health(&self) -> Result<HealthResponse, RpcError> {
        let uptime_seconds = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };

        let bc = self.blockchain.lock().await;

        let shutdown_active = genesis::is_shutdown() || bc.shutdown;
        let latest_block_time = bc
            .blocks
            .last()
            .map(|b| b.header.timestamp.0)
            .unwrap_or(0);

        let now_ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => 0,
        };

        let peer_count: u64 = self
            .network_metrics
            .peers_connected
            .load(Ordering::Relaxed);
        let sync_status = if bc.blocks.len() <= 1 { "syncing" } else { "synced" };

        let status = if shutdown_active {
            "unhealthy"
        } else if now_ts > 0 && latest_block_time > 0 && now_ts.saturating_sub(latest_block_time) > 60 {
            "degraded"
        } else {
            "healthy"
        };

        Ok(HealthResponse {
            status: status.to_string(),
            uptime_seconds,
            peer_count,
            sync_status: sync_status.to_string(),
            latest_block_time,
            shutdown_active,
        })
    }

    async fn get_pending_transactions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<TransactionResponse>, RpcError> {
        let limit = limit.unwrap_or(50).min(500);
        let bc = self.blockchain.lock().await;
        let txs = bc.get_pending_transactions(limit);
        Ok(txs.iter().map(TransactionResponse::from_tx).collect())
    }
}
