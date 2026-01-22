use std::sync::Arc;

use anyhow::Result;
use blockchain_core::{Block, Blockchain, Transaction};
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use tokio::sync::{broadcast, Mutex};

use crate::config::RpcConfig;
use crate::rpc::ceo::{CeoRpcMethods, CeoRpcServer};
use crate::rpc::methods::{PublicRpcServer, RpcMethods};
use crate::rpc::subscriptions::{RpcSubscriptions, SubscriptionsRpcServer};

pub async fn start_rpc_server(
    blockchain: Arc<Mutex<Blockchain>>,
    block_tx: broadcast::Sender<Block>,
    tx_tx: broadcast::Sender<Transaction>,
    tx_broadcast: broadcast::Sender<Transaction>,
    network_metrics: network::metrics::SharedNetworkMetrics,
    config: RpcConfig,
) -> Result<ServerHandle> {
    let server = ServerBuilder::default()
        .max_connections(config.rpc_max_connections.try_into().unwrap_or(u32::MAX))
        .build(config.rpc_addr)
        .await?;

    let addr = server.local_addr()?;

    let public = RpcMethods::new(blockchain.clone(), tx_broadcast, network_metrics);
    let ceo = CeoRpcMethods::new(blockchain.clone());
    let subs = RpcSubscriptions::new(block_tx, tx_tx);

    let mut module = public.into_rpc();
    module.merge(ceo.into_rpc())?;
    module.merge(subs.into_rpc())?;

    let handle = server.start(module);

    println!("rpc server started on {}", addr);

    Ok(handle)
}
