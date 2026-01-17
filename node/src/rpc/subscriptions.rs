use blockchain_core::{Block, Transaction};
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage};
use tokio::sync::broadcast;
use tracing::info;

use crate::rpc::types::{BlockResponse, TransactionResponse};

#[rpc(server)]
pub trait SubscriptionsRpc {
    #[subscription(name = "subscribeNewBlocks", item = BlockResponse)]
    async fn subscribe_new_blocks(&self);

    #[subscription(name = "subscribeNewTransactions", item = TransactionResponse)]
    async fn subscribe_new_transactions(&self);

    #[subscription(name = "subscribeShutdown", item = String)]
    async fn subscribe_shutdown(&self);
}

pub struct RpcSubscriptions {
    pub block_tx: broadcast::Sender<Block>,
    pub tx_tx: broadcast::Sender<Transaction>,
}

impl RpcSubscriptions {
    pub fn new(block_tx: broadcast::Sender<Block>, tx_tx: broadcast::Sender<Transaction>) -> Self {
        Self { block_tx, tx_tx }
    }
}

#[async_trait]
impl SubscriptionsRpcServer for RpcSubscriptions {
    async fn subscribe_new_blocks(&self, pending: PendingSubscriptionSink) {
        if let Ok(sink) = pending.accept().await {
            let mut rx = self.block_tx.subscribe();
            info!("ws subscribeNewBlocks connected");
            loop {
                match rx.recv().await {
                    Ok(block) => {
                        let msg = BlockResponse::from_block(&block);
                        if let Ok(payload) = SubscriptionMessage::from_json(&msg) {
                            if sink.send(payload).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }

    async fn subscribe_new_transactions(
        &self,
        pending: PendingSubscriptionSink,
    ) {
        if let Ok(sink) = pending.accept().await {
            let mut rx = self.tx_tx.subscribe();
            info!("ws subscribeNewTransactions connected");
            loop {
                match rx.recv().await {
                    Ok(tx) => {
                        let msg = TransactionResponse::from_tx(&tx);
                        if let Ok(payload) = SubscriptionMessage::from_json(&msg) {
                            if sink.send(payload).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }

    async fn subscribe_shutdown(&self, pending: PendingSubscriptionSink) {
        if let Ok(sink) = pending.accept().await {
            info!("ws subscribeShutdown connected");
            loop {
                if genesis::is_shutdown() {
                    if let Ok(payload) = SubscriptionMessage::from_json(&"shutdown") {
                        let _ = sink.send(payload).await;
                    }
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}
