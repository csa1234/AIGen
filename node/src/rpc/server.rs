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
use tower::{Layer, Service};
use hyper::{Request, Response, Body, Method};
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use std::task::{Context, Poll};
use tower_http::cors::{Any, AllowOrigin, CorsLayer};

use crate::config::RpcConfig;
use crate::rpc::ceo::{CeoRpcMethods, CeoRpcServer};
use crate::rpc::methods::{PublicRpcServer, RpcMethods, RewardRpcServer, RewardRpcImpl};
use crate::rpc::subscriptions::{RpcSubscriptions, SubscriptionsRpcServer};
use crate::rpc::model::{ModelRpcMethods, ModelRpcServer};
use distributed_compute::scheduler::DynamicScheduler;
use distributed_compute::state::GlobalState;
use distributed_inference::OrchestratorNode;

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
    dcs: Option<Arc<DynamicScheduler>>,
    dcs_state: Option<Arc<GlobalState>>,
    orchestrator: Option<Arc<distributed_inference::OrchestratorNode>>,
) -> Result<ServerHandle> {
    let cors = build_cors_layer(&config);

    let server = ServerBuilder::default()
        .max_connections(config.rpc_max_connections.try_into().unwrap_or(u32::MAX))
        .set_http_middleware(
            tower::ServiceBuilder::new()
                .layer(HealthProxyLayer)
                .layer(cors),
        )
        .build(config.rpc_addr)
        .await?;

    let addr = server.local_addr()?;

    println!("RPC server started successfully");
    println!("  Address: http://{}", addr);
    println!("  CORS: Allowed origins: {:?}", config.rpc_cors);
    println!("  Max connections: {}", config.rpc_max_connections);
    println!("  Health endpoint: GET /health or POST / {{\"method\":\"health\"}}");

    if addr.ip().is_loopback() {
        println!("  ⚠️  Binding to localhost only - not accessible from other machines");
    }

    let public = RpcMethods::new(blockchain.clone(), tx_broadcast, network_metrics.clone(), dcs.clone(), dcs_state.clone(), orchestrator.clone());
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
    let reward_rpc = RewardRpcImpl::new(dcs.clone(), dcs_state.clone());

    let mut module = public.into_rpc();
    module.merge(ceo.into_rpc())?;
    module.merge(subs.into_rpc())?;
    module.merge(model_rpc.into_rpc())?;
    module.merge(reward_rpc.into_rpc())?;

    let handle = server.start(module);

    Ok(handle)
}

fn build_cors_layer(config: &RpcConfig) -> CorsLayer {
    let mut cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::ACCEPT,
            http::header::AUTHORIZATION,
        ]);

    if config.rpc_cors.iter().any(|s| s.trim() == "*") {
        cors = cors.allow_origin(Any);
        return cors;
    }

    let origins = config
        .rpc_cors
        .iter()
        .filter_map(|s| HeaderValue::from_str(s.trim()).ok())
        .collect::<Vec<_>>();

    if origins.is_empty() {
        return cors.allow_origin(Any);
    }

    cors.allow_origin(AllowOrigin::list(origins))
}

#[derive(Clone)]
struct HealthProxyLayer;

impl<S> Layer<S> for HealthProxyLayer {
    type Service = HealthProxyService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HealthProxyService { inner }
    }
}

#[derive(Clone)]
struct HealthProxyService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for HealthProxyService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        if req.method() == Method::GET && req.uri().path() == "/health" {
            *req.method_mut() = Method::POST;
            *req.uri_mut() = "/".parse().unwrap();
            req.headers_mut().insert(CONTENT_TYPE, "application/json".parse().unwrap());
            let body = r#"{"jsonrpc":"2.0","method":"health","params":[],"id":1}"#;
            *req.body_mut() = Body::from(body);
        }
        self.inner.call(req)
    }
}
