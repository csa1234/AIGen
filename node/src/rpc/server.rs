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

use anyhow::Result;
use blockchain_core::{Block, Blockchain, Transaction};
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use tokio::sync::{broadcast, Mutex};

use crate::config::RpcConfig;
use crate::rpc::ceo::{CeoRpcMethods, CeoRpcServer};
use crate::rpc::methods::{PublicRpcServer, RpcMethods};
use crate::rpc::subscriptions::{RpcSubscriptions, SubscriptionsRpcServer};
use crate::rpc::model::{ModelRpcMethods, ModelRpcServer};

pub async fn start_rpc_server(
    blockchain: Arc<Mutex<Blockchain>>,
    block_tx: broadcast::Sender<Block>,
    tx_tx: broadcast::Sender<Transaction>,
    tx_broadcast: broadcast::Sender<Transaction>,
    network_metrics: network::metrics::SharedNetworkMetrics,
    model_registry: Arc<model::ModelRegistry>,
    tier_manager: Arc<model::TierManager>,
    batch_queue: Arc<model::BatchQueue>,
    inference_engine: Arc<model::InferenceEngine>,
    config: RpcConfig,
) -> Result<ServerHandle> {
    let server = ServerBuilder::default()
        .max_connections(config.rpc_max_connections.try_into().unwrap_or(u32::MAX))
        .build(config.rpc_addr)
        .await?;

    let addr = server.local_addr()?;

    let public = RpcMethods::new(blockchain.clone(), tx_broadcast, network_metrics.clone());
    let ceo = CeoRpcMethods::new(
        blockchain.clone(),
        model_registry.clone(),
        inference_engine.clone(),
        tier_manager.clone(),
        batch_queue.clone(),
        network_metrics.clone(),
    );
    let subs = RpcSubscriptions::new(block_tx, tx_tx);
    let model_rpc = ModelRpcMethods::new(
        blockchain.clone(),
        model_registry,
        tier_manager,
        batch_queue,
        inference_engine,
    );

    let mut module = public.into_rpc();
    module.merge(ceo.into_rpc())?;
    module.merge(subs.into_rpc())?;
    module.merge(model_rpc.into_rpc())?;

    let handle = server.start(module);

    println!("rpc server started on {}", addr);

    Ok(handle)
}
