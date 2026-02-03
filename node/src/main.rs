// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

mod cli;
mod config;
mod keypair;
mod rpc;

#[cfg(test)]
mod chat_completion_test;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use blockchain_core::{BlockHeight, Blockchain, Transaction};
use consensus::{set_inference_verification_config, PoIConsensus};
use model::{set_global_inference_engine, InferenceEngine, LocalStorage, ModelRegistry, TierManager, AdManager};
use network::model_sync::ModelShardRequestEnvelope;
use network::{ModelSyncManager, NetworkEvent, NetworkMessage, P2PNode};
use tokio::sync::{mpsc, Mutex};

fn main() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async_main())
}

async fn async_main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = crate::cli::parse_cli();

    match &cli.command {
        crate::cli::Commands::Init(args) => {
            let config_path = args
                .config
                .clone()
                .unwrap_or_else(crate::config::NodeConfiguration::default_config_path);

            let mut cfg = crate::config::NodeConfiguration::default();
            cfg = cfg.merge_with_env();
            cfg = cfg.merge_with_cli(&cli);

            if !args.force {
                if config_path.exists() {
                    return Err(anyhow!(
                        "config file already exists: {} (use --force to overwrite)",
                        config_path.display()
                    ));
                }
                if crate::keypair::keypair_exists(&cfg.keypair_path) {
                    return Err(anyhow!(
                        "keypair already exists: {} (use --force to overwrite)",
                        cfg.keypair_path.display()
                    ));
                }
            }

            std::fs::create_dir_all(&cfg.data_dir).with_context(|| {
                format!("failed to create data_dir: {}", cfg.data_dir.display())
            })?;

            let kp = crate::keypair::generate_keypair();
            crate::keypair::save_keypair(&kp, &cfg.keypair_path)?;

            cfg.save_to_file(&config_path)?;

            println!(
                "init complete: config_path={}, data_dir={}, keypair_path={}",
                config_path.display(),
                cfg.data_dir.display(),
                cfg.keypair_path.display()
            );
        }
        crate::cli::Commands::Start(args) => {
            let config_path = args
                .config
                .clone()
                .unwrap_or_else(crate::config::NodeConfiguration::default_config_path);

            let mut cfg = if config_path.exists() {
                let loaded = crate::config::NodeConfiguration::load_from_file(&config_path)?;
                println!("loaded config: {}", config_path.display());
                loaded
            } else {
                println!(
                    "config not found; using defaults: {}",
                    config_path.display()
                );
                crate::config::NodeConfiguration::default()
            };

            cfg = cfg.merge_with_env();
            cfg = cfg.merge_with_cli(&cli);
            cfg.validate()?;

            println!(
                "starting node: node_id={}, data_dir={}",
                cfg.node_id,
                cfg.data_dir.display()
            );
            println!(
                "network config: listen_addr={}, bootstrap_peers={}",
                cfg.network.listen_addr,
                cfg.network.bootstrap_peers.len()
            );

            let kp = if crate::keypair::keypair_exists(&cfg.keypair_path) {
                crate::keypair::load_keypair(&cfg.keypair_path)?
            } else {
                let kp = crate::keypair::generate_keypair();
                crate::keypair::save_keypair(&kp, &cfg.keypair_path)?;
                kp
            };

            run_node(cfg, kp).await?;
        }
        crate::cli::Commands::Keygen(args) => {
            let kp = crate::keypair::generate_keypair();

            let out = args
                .output
                .clone()
                .unwrap_or_else(|| PathBuf::from("node_keypair.bin"));
            crate::keypair::save_keypair(&kp, &out)?;

            if args.show_peer_id {
                let peer_id = libp2p::PeerId::from(kp.public());
                println!("generated keypair: peer_id={}", peer_id);
            }
        }
        crate::cli::Commands::Version(args) => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            if args.verbose {
                let git_commit = option_env!("GIT_COMMIT").unwrap_or("unknown");
                let build_date = option_env!("BUILD_DATE").unwrap_or("unknown");
                let rustc_version = option_env!("RUSTC_VERSION").unwrap_or("unknown");
                println!("git_commit: {git_commit}");
                println!("build_date: {build_date}");
                println!("rustc: {rustc_version}");
            }
        }
    }

    Ok(())
}

async fn load_core_model(
    cfg: &crate::config::NodeConfiguration,
    registry: Arc<ModelRegistry>,
    model_sync: Arc<ModelSyncManager>,
    inference_engine: Arc<InferenceEngine>,
) -> Result<()> {
    let Some(core_model_id) = &cfg.model.core_model_id else {
        println!("no core model specified; skipping model loading");
        return Ok(());
    };

    println!("loading core model: {}", core_model_id);

    let model_exists = registry
        .model_exists(core_model_id)
        .map_err(|e| anyhow!("registry check failed: {}", e))?;

    if !model_exists {
        return Err(anyhow!(
            "core model '{}' not found in registry; register model metadata first",
            core_model_id
        ));
    }

    model_sync
        .query_model_shards(core_model_id)
        .await
        .map_err(|e| anyhow!("shard query failed: {}", e))?;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let metadata = registry
        .get_model(core_model_id)
        .map_err(|e| anyhow!("failed to get model metadata: {}", e))?;
    let registered_shards = registry
        .list_shards(core_model_id)
        .map_err(|e| anyhow!("failed to list shards: {}", e))?;

    let missing_shards: Vec<u32> = (0..metadata.shard_count)
        .filter(|idx| !registered_shards.iter().any(|s| s.shard_index == *idx))
        .collect();

    if !missing_shards.is_empty() {
        println!(
            "downloading {} missing shards for core model",
            missing_shards.len()
        );

        for shard_idx in &missing_shards {
            model_sync
                .request_shard_download(core_model_id.clone(), *shard_idx)
                .map_err(|e| anyhow!("failed to queue shard download: {}", e))?;
        }

        let timeout = std::time::Duration::from_secs(cfg.model.download_timeout_secs);
        let download_result = tokio::time::timeout(timeout, async {
            loop {
                model_sync
                    .process_download_queue()
                    .await
                    .map_err(|e| anyhow!("download processing failed: {}", e))?;

                let current_shards = registry
                    .list_shards(core_model_id)
                    .map_err(|e| anyhow!("failed to list shards: {}", e))?;

                if current_shards.len() >= metadata.shard_count as usize {
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            Ok::<(), anyhow::Error>(())
        })
        .await;

        match download_result {
            Ok(Ok(())) => println!("core model shards downloaded successfully"),
            Ok(Err(e)) => return Err(anyhow!("shard download failed: {}", e)),
            Err(_) => {
                return Err(anyhow!(
                    "core model download timeout after {} seconds",
                    cfg.model.download_timeout_secs
                ))
            }
        }
    }

    println!("loading core model into memory...");
    let load_result =
        tokio::time::timeout(std::time::Duration::from_secs(30), inference_engine.preload_core_model())
            .await;

    match load_result {
        Ok(Ok(Some(_))) => {
            println!("core model loaded successfully: {}", core_model_id);
        }
        Ok(Ok(None)) => {
            return Err(anyhow!("core model not found after download"));
        }
        Ok(Err(e)) => {
            return Err(anyhow!("model loading failed: {}", e));
        }
        Err(_) => {
            return Err(anyhow!("model loading timeout after 30 seconds"));
        }
    }

    model_sync
        .announce_local_shards()
        .await
        .map_err(|e| anyhow!("failed to announce shards: {}", e))?;

    let mut min_replicas = usize::MAX;
    for idx in 0..metadata.shard_count {
        let count = registry
            .get_shard_locations(core_model_id, idx)
            .map(|locs| locs.len())
            .unwrap_or(0);
        if count < min_replicas {
            min_replicas = count;
        }
    }
    if min_replicas < cfg.model.min_redundancy_nodes {
        eprintln!(
            "warning: core model redundancy below target ({} < {})",
            min_replicas,
            cfg.model.min_redundancy_nodes
        );
    }

    Ok(())
}
async fn run_node(
    cfg: crate::config::NodeConfiguration,
    keypair: libp2p::identity::Keypair,
) -> Result<()> {
    let blockchain = Blockchain::new_with_genesis(cfg.genesis.clone());
    let chain_state = Arc::new(blockchain.state.clone());
    let blockchain = Arc::new(Mutex::new(blockchain));

    let (block_broadcast_tx, _) = tokio::sync::broadcast::channel::<blockchain_core::Block>(100);
    let (tx_broadcast_tx, _) = tokio::sync::broadcast::channel::<blockchain_core::Transaction>(100);

    let (p2p_node, mut network_event_rx) =
        P2PNode::new(keypair, cfg.network.clone(), Some(cfg.node.clone()))?;
    let local_peer_id = p2p_node.local_peer_id;
    let network_metrics = p2p_node.metrics();
    println!("p2p initialized: peer_id={}", local_peer_id);

    let rpc_handle: Option<jsonrpsee::server::ServerHandle>;

    let mut consensus = PoIConsensus::new(chain_state.clone());
    let (consensus_network_tx, consensus_network_rx) =
        mpsc::channel::<consensus::NetworkMessage>(1024);
    consensus.set_network_publisher(consensus_network_tx);
    let consensus = Arc::new(Mutex::new(consensus));

    let (publish_tx, publish_rx) = mpsc::channel::<NetworkMessage>(1024);
    let model_registry = Arc::new(ModelRegistry::new());
    let model_storage = Arc::new(LocalStorage::new(cfg.model.model_storage_path.clone()));
    
    // Initialize TierManager and AdManager with configuration
    let tier_manager = Arc::new(TierManager::with_chain_state(
        model::default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    ));
    let ad_manager = Arc::new(AdManager::new(tier_manager.clone(), cfg.ads.clone()));
    
    let inference_engine = Arc::new(InferenceEngine::new(
        model_registry.clone(),
        model_storage.clone(),
        cfg.model.cache_dir.clone(),
        cfg.model.max_memory_mb.saturating_mul(1024 * 1024),
        cfg.model.num_threads,
        Some(ad_manager.clone()),
    ));
    if let Err(err) = set_global_inference_engine(inference_engine.clone()) {
        eprintln!("failed to register inference engine: {}", err);
    }
    if let Err(err) = set_inference_verification_config(cfg.verification.clone()) {
        eprintln!("failed to set verification config: {}", err);
    }
    let model_reputation = p2p_node.reputation_manager.clone();
    let model_metrics = p2p_node.metrics();
    let (model_request_tx, model_request_rx) = mpsc::channel::<ModelShardRequestEnvelope>(256);
    let (model_event_tx, _model_event_rx) = mpsc::channel::<NetworkEvent>(256);
    let model_sync = Arc::new(ModelSyncManager::new(
        model_registry.clone(),
        model_storage.clone(),
        model_reputation,
        model_metrics,
        publish_tx.clone(),
        model_request_tx,
        model_event_tx,
        cfg.node_id.clone(),
    ));

    // networking tasks are spawned below; defer core model loading until after they start

    let discount_tracker = model::VolumeDiscountTracker::new(model::VolumeDiscountTracker::default_tiers());
    let batch_queue = Arc::new(model::BatchQueue::new(
        tier_manager.clone(),
        inference_engine.clone(),
        Some(ad_manager.clone()),
        chain_state.clone(),
        discount_tracker,
    ));
    let (batch_shutdown_tx, batch_shutdown_rx) = tokio::sync::watch::channel(false);
    let batch_worker = model::BatchWorker::new(
        batch_queue.clone(),
        inference_engine.clone(),
        batch_shutdown_rx,
        None,
    );
    let batch_worker_handle = batch_worker.start();

    rpc_handle = if cfg.rpc.rpc_enabled {
        Some(
            crate::rpc::server::start_rpc_server(
                blockchain.clone(),
                block_broadcast_tx.clone(),
                tx_broadcast_tx.clone(),
                tx_broadcast_tx.clone(),
                network_metrics,
                model_registry.clone(),
                tier_manager.clone(),
                batch_queue.clone(),
                inference_engine.clone(),
                cfg.rpc.clone(),
            )
            .await?,
        )
    } else {
        None
    };

    let (local_shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(4);
    let local_shutdown_rx = local_shutdown_tx.subscribe();

    tokio::spawn(async move {
        let mut rx = consensus_network_rx;
        while let Some(msg) = rx.recv().await {
            #[allow(unreachable_patterns)]
            match msg {
                consensus::NetworkMessage::Block(block) => {
                    let _ = publish_tx.send(NetworkMessage::Block(block)).await;
                }
                consensus::NetworkMessage::ValidatorVote(vote) => {
                    let _ = publish_tx.send(NetworkMessage::ValidatorVote(vote)).await;
                }
                _ => {
                    eprintln!("unhandled consensus network message variant");
                }
            }
        }
    });

    {
        let consensus = consensus.clone();
        consensus.lock().await.start_shutdown_monitor();
    }
    let mut shutdown_rx = { consensus.lock().await.shutdown_subscribe() };

    let p2p_task = tokio::spawn(async move {
        p2p_node
            .start_with_publisher(publish_rx, model_request_rx)
            .await;
    });

    let network_task = {
        let blockchain = blockchain.clone();
        let consensus = consensus.clone();
        let model_sync = model_sync.clone();
        let mut local_shutdown_rx = local_shutdown_rx;
        let block_broadcast_tx = block_broadcast_tx.clone();
        let tx_broadcast_tx = tx_broadcast_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = local_shutdown_rx.recv() => {
                        break;
                    }
                    maybe_ev = network_event_rx.recv() => {
                        let Some(ev) = maybe_ev else {
                            break;
                        };
                        match ev {
                            NetworkEvent::PeerDiscovered(peer_id) => {
                                println!("peer discovered: peer_id={}", peer_id);
                            }
                            NetworkEvent::MessageReceived(msg) => {
                                match msg {
                                    NetworkMessage::Block(block) => {
                                        let hashes: Vec<blockchain_core::TxHash> = block
                                            .transactions
                                            .iter()
                                            .map(|t: &Transaction| t.tx_hash)
                                            .collect();
                                        let mut bc = blockchain.lock().await;
                                        match bc.add_block(block) {
                                            Ok(()) => {
                                                if let Some(b) = bc.blocks.last() {
                                                    let _ = block_broadcast_tx.send(b.clone());
                                                }
                                                bc.remove_transactions_from_pool(&hashes);
                                                println!("block added: height={}", bc.blocks.len().saturating_sub(1));
                                            }
                                            Err(e) => {
                                                eprintln!("failed to add block: {}", e);
                                            }
                                        }
                                    }
                                    NetworkMessage::Transaction(tx) => {
                                        let mut bc = blockchain.lock().await;
                                        let tx_for_pool = tx.clone();
                                        if let Err(e) = bc.add_transaction_to_pool(tx_for_pool) {
                                            eprintln!("failed to add transaction to pool: {}", e);
                                        } else {
                                            let _ = tx_broadcast_tx.send(tx);
                                        }
                                    }
                                    NetworkMessage::PoIProof(proof) => {
                                        let (pending_txs, prev_hash, next_height) = {
                                            let bc = blockchain.lock().await;
                                            let prev_hash = bc
                                                .blocks
                                                .last()
                                                .map(|b| b.block_hash)
                                                .unwrap_or(blockchain_core::BlockHash([0u8; 32]));
                                            let next_height = BlockHeight::new(bc.blocks.len() as u64);
                                            let pending = bc.get_pending_transactions(512);
                                            (pending, prev_hash, next_height)
                                        };

                                        let miner_address = proof.miner_address.clone();
                                        let produced = {
                                            let mut c = consensus.lock().await;
                                            c.process_poi_submission(
                                                proof,
                                                miner_address,
                                                pending_txs,
                                                prev_hash,
                                                next_height,
                                            )
                                            .await
                                        };

                                        match produced {
                                            Ok(block) => {
                                                let hashes: Vec<blockchain_core::TxHash> = block
                                                    .transactions
                                                    .iter()
                                                    .map(|t: &Transaction| t.tx_hash)
                                                    .collect();

                                                let mut bc = blockchain.lock().await;
                                                match bc.add_block(block) {
                                                    Ok(()) => {
                                                        if let Some(b) = bc.blocks.last() {
                                                            let _ = block_broadcast_tx.send(b.clone());
                                                        }
                                                        bc.remove_transactions_from_pool(&hashes);
                                                        println!("block added: height={}", bc.blocks.len().saturating_sub(1));
                                                    }
                                                    Err(e) => {
                                                        eprintln!("failed to add block: {}", e);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("poi submission rejected: {}", e);
                                            }
                                        }
                                    }
                                    NetworkMessage::ValidatorVote(vote) => {
                                        let c = consensus.lock().await;
                                        let _ = c.submit_validator_vote(vote);
                                    }
                                    _ => {}
                                }
                            }
                            NetworkEvent::ShutdownSignal => {
                                eprintln!("shutdown signal received from network");
                                break;
                            }
                            NetworkEvent::ModelQueryReceived { model_id, .. } => {
                                if let Err(err) = model_sync.handle_model_query(&model_id).await {
                                    eprintln!("failed to handle model query: {}", err);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
    };

    if cfg.model.core_model_id.is_some() {
        let load_result = load_core_model(
            &cfg,
            model_registry.clone(),
            model_sync.clone(),
            inference_engine.clone(),
        )
        .await;

        match load_result {
            Ok(()) => {
                println!("core AI model ready for inference");
            }
            Err(e) => {
                if cfg.model.worker_mode {
                    return Err(anyhow!("worker node failed to load core model: {}", e));
                } else {
                    eprintln!("warning: core model loading failed: {}", e);
                    eprintln!("node will continue without AI inference capabilities");
                }
            }
        }
    }

    println!("node started; waiting for shutdown");

    tokio::select! {
        _ = shutdown_rx.recv() => {
            eprintln!("shutdown signal received from consensus");
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("ctrl-c received");
        }
        _ = async {
            loop {
                if genesis::is_shutdown() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        } => {
            eprintln!("shutdown active (genesis)");
        }
    }

    if let Some(h) = rpc_handle {
        h.stop()?;
    }

    let _ = local_shutdown_tx.send(());
    let _ = batch_shutdown_tx.send(true);

    let join_timeout = std::time::Duration::from_secs(5);
    let mut network_task = network_task;
    let net_join = tokio::time::timeout(join_timeout, async { (&mut network_task).await }).await;
    if net_join.is_err() {
        eprintln!("network task did not exit in time; aborting");
        network_task.abort();
        let _ = network_task.await;
    }

    let mut p2p_task = p2p_task;
    let p2p_join = tokio::time::timeout(join_timeout, async { (&mut p2p_task).await }).await;
    if p2p_join.is_err() {
        eprintln!("p2p task did not exit in time; aborting");
        p2p_task.abort();
        let _ = p2p_task.await;
    }

    let batch_join = tokio::time::timeout(join_timeout, batch_worker_handle).await;
    if batch_join.is_err() {
        eprintln!("batch worker did not exit in time");
    }

    println!("node exiting");
    Ok(())
}
