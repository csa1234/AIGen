// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use blockchain_core::types::Amount;
use config as config_rs;
use consensus::InferenceVerificationConfig;
use genesis::GenesisConfig;
use libp2p::Multiaddr;
use model::AdConfig;
use network::config::NetworkConfig;
use network::node_types::{NodeConfig, NodeType};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DistributedInferenceConfig {
    pub enabled: bool,
    pub block_id: Option<u32>,
    pub total_blocks: u32,
    pub pipeline_config_path: Option<PathBuf>,
    pub coordinator_mode: bool, // true if this node initiates inference
}

impl Default for DistributedInferenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            block_id: None,
            total_blocks: 1,
            pipeline_config_path: None,
            coordinator_mode: false,
        }
    }
}

/// Training configuration for federated learning
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TrainingConfig {
    pub enabled: bool,
    pub buffer_size: usize,        // Default: 1000
    pub trigger_threshold: usize,  // Default: 900
    pub batch_size: usize,         // Default: 128
    pub learning_rate: f32,        // Default: 1e-6
    pub epochs: u32,               // Default: 1
    pub ewc_lambda: f32,           // Default: 0.4
    pub replay_ratio: f32,         // Default: 0.01 (1%)
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 1000,
            trigger_threshold: 900,
            batch_size: 128,
            learning_rate: 1e-6,
            epochs: 1,
            ewc_lambda: 0.4,
            replay_ratio: 0.01,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NodeConfiguration {
    pub node_id: String,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub verification: InferenceVerificationConfig,
    #[serde(default)]
    pub ads: AdConfig,
    pub network: NetworkConfig,
    pub genesis: GenesisConfig,
    pub node: NodeConfig,
    pub rpc: RpcConfig,
    #[serde(default)]
    pub keypair_path: PathBuf,
    #[serde(default)]
    pub distributed: DistributedInferenceConfig,
    #[serde(default)]
    pub training: TrainingConfig,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub cache_dir: PathBuf,
    pub model_storage_path: PathBuf,
    pub core_model_id: Option<String>,
    pub worker_mode: bool,
    pub max_memory_mb: usize,
    pub num_threads: usize,
    pub download_timeout_secs: u64,
    pub min_redundancy_nodes: usize,
    pub fragment_size_mb: u32,
    pub enable_fragments: bool,
    pub fragment_compression: bool,
    pub fragment_compression_level: i32,
}

impl ModelConfig {
    pub fn default_with_data_dir(data_dir: &Path) -> Self {
        Self {
            cache_dir: data_dir.join("model-cache"),
            model_storage_path: data_dir.join("models"),
            core_model_id: None,
            worker_mode: false,
            max_memory_mb: 2048,
            num_threads: 0,
            download_timeout_secs: 30,
            min_redundancy_nodes: 5,
            fragment_size_mb: 200,
            enable_fragments: false,  // Opt-in for Phase 1
            fragment_compression: true,
            fragment_compression_level: 3,
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        let data_dir = NodeConfiguration::default_data_dir();
        Self::default_with_data_dir(&data_dir)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RpcConfig {
    pub rpc_addr: SocketAddr,
    pub rpc_cors: Vec<String>,
    pub rpc_max_connections: usize,
    pub rpc_enabled: bool,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            rpc_addr: "127.0.0.1:9944".parse().expect("default rpc addr"),
            rpc_cors: vec!["*".to_string()],
            rpc_max_connections: 100,
            rpc_enabled: true,
        }
    }
}

impl NodeConfiguration {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let format = if ext == "toml" {
            config_rs::FileFormat::Toml
        } else {
            config_rs::FileFormat::Json
        };

        let cfg = config_rs::Config::builder()
            .add_source(config_rs::File::from(path).format(format))
            .build()
            .with_context(|| format!("failed to load config file: {}", path.display()))?;

        let mut cfg = cfg
            .try_deserialize::<NodeConfiguration>()
            .with_context(|| format!("failed to deserialize config: {}", path.display()))?;
        if cfg.model.cache_dir.as_os_str().is_empty()
            || cfg.model.model_storage_path.as_os_str().is_empty()
        {
            cfg.derive_model_paths();
        }
        if cfg.model.download_timeout_secs == 0 {
            cfg.model.download_timeout_secs = 30;
        }
        if cfg.model.min_redundancy_nodes == 0 {
            cfg.model.min_redundancy_nodes = 5;
        }
        // Ensure keypair_path is set
        if cfg.keypair_path.as_os_str().is_empty() {
            cfg.keypair_path = crate::keypair::default_keypair_path(&cfg.data_dir);
        }
        Ok(cfg)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create config parent directory: {}",
                    parent.display()
                )
            })?;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let out = if ext == "toml" {
            toml::to_string_pretty(self).context("failed to serialize config as toml")?
        } else {
            serde_json::to_string_pretty(self).context("failed to serialize config as json")?
        };

        std::fs::write(path, out)
            .with_context(|| format!("failed to write config file: {}", path.display()))?;
        Ok(())
    }

    pub fn derive_model_paths(&mut self) {
        self.model.cache_dir = self.data_dir.join("model-cache");
        self.model.model_storage_path = self.data_dir.join("models");
    }

    pub fn merge_with_env(mut self) -> Self {
        if let Ok(v) = std::env::var("AIGEN_NODE_ID") {
            self.node_id = v;
        }
        if let Ok(v) = std::env::var("AIGEN_DATA_DIR") {
            self.data_dir = PathBuf::from(v);
            self.derive_model_paths();
        }
        if let Ok(v) = std::env::var("AIGEN_LISTEN_ADDR") {
            if let Ok(ma) = v.parse::<Multiaddr>() {
                self.network.listen_addr = ma;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_BOOTSTRAP_PEERS") {
            let peers = v
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse::<Multiaddr>().ok())
                .collect::<Vec<_>>();
            if !peers.is_empty() {
                self.network.bootstrap_peers = peers;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_MAX_PEERS") {
            if let Ok(n) = v.parse::<usize>() {
                self.network.max_peers = n;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_ENABLE_MDNS") {
            if let Ok(b) = v.parse::<bool>() {
                self.network.enable_mdns = b;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_NODE_TYPE") {
            if let Ok(t) = parse_node_type(&v) {
                self.node.node_type = t;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_VALIDATOR_ADDRESS") {
            self.node.validator_address = Some(v);
        }
        if let Ok(v) = std::env::var("AIGEN_STAKE_AMOUNT") {
            if let Ok(n) = v.parse::<u64>() {
                self.node.stake_amount = Some(Amount::new(n));
            }
        }

        if let Ok(v) = std::env::var("AIGEN_RPC_ADDR") {
            if let Ok(addr) = v.parse::<SocketAddr>() {
                self.rpc.rpc_addr = addr;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_RPC_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                self.rpc.rpc_enabled = b;
            }
        }

        if let Ok(v) = std::env::var("AIGEN_MODEL_CACHE_DIR") {
            if !v.trim().is_empty() {
                self.model.cache_dir = PathBuf::from(v);
            }
        }
        if let Ok(v) = std::env::var("AIGEN_MODEL_MAX_MEMORY_MB") {
            if let Ok(n) = v.parse::<usize>() {
                self.model.max_memory_mb = n;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_MODEL_NUM_THREADS") {
            if let Ok(n) = v.parse::<usize>() {
                self.model.num_threads = n;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_CORE_MODEL_ID") {
            if !v.trim().is_empty() {
                self.model.core_model_id = Some(v);
            }
        }
        if let Ok(v) = std::env::var("AIGEN_WORKER_MODE") {
            if let Ok(b) = v.parse::<bool>() {
                self.model.worker_mode = b;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_VERIFICATION_CACHE_CAPACITY") {
            if let Ok(n) = v.parse::<usize>() {
                self.verification.cache_capacity = n;
            }
        }
        if let Ok(v) = std::env::var("AIGEN_VERIFICATION_EPSILON") {
            if let Ok(n) = v.parse::<f32>() {
                self.verification.epsilon = n;
            }
        }

        self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);

        self
    }

    pub fn merge_with_cli(mut self, cli_args: &crate::cli::Cli) -> Self {
        match &cli_args.command {
            crate::cli::Commands::Init(args) => {
                if let Some(v) = &args.node_id {
                    self.node_id = v.clone();
                }
                if let Some(v) = &args.data_dir {
                    self.data_dir = v.clone();
                    self.derive_model_paths();
                }
                if let Some(model) = &args.model {
                    self.model.core_model_id = Some(model.clone());
                }
                if let Some(role) = &args.role {
                    self.model.worker_mode = role.to_lowercase() == "worker";
                }
                self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);
            }
            crate::cli::Commands::Start(args) => {
                if let Some(v) = &args.data_dir {
                    self.data_dir = v.clone();
                    self.derive_model_paths();
                }
                if let Some(v) = &args.listen_addr {
                    if let Ok(ma) = v.parse::<Multiaddr>() {
                        self.network.listen_addr = ma;
                    }
                }
                if let Some(v) = &args.bootstrap_peers {
                    let peers = v
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .filter_map(|s| s.parse::<Multiaddr>().ok())
                        .collect::<Vec<_>>();
                    if !peers.is_empty() {
                        self.network.bootstrap_peers = peers;
                    }
                }
                if let Some(v) = &args.node_type {
                    if let Ok(t) = parse_node_type(v) {
                        self.node.node_type = t;
                    }
                }
                if let Some(v) = &args.validator_address {
                    self.node.validator_address = Some(v.clone());
                }
                if let Some(v) = args.stake_amount {
                    self.node.stake_amount = Some(Amount::new(v));
                }

                if let Some(v) = &args.rpc_addr {
                    if let Ok(addr) = v.parse::<SocketAddr>() {
                        self.rpc.rpc_addr = addr;
                    }
                }
                if let Some(port) = args.rpc_port {
                    let ip = self.rpc.rpc_addr.ip();
                    self.rpc.rpc_addr = SocketAddr::new(ip, port);
                }
                if args.disable_rpc {
                    self.rpc.rpc_enabled = false;
                }
                if let Some(model) = &args.model {
                    self.model.core_model_id = Some(model.clone());
                }
                if let Some(role) = &args.role {
                    self.model.worker_mode = role.to_lowercase() == "worker";
                }

                // Distributed inference CLI args
                if args.distributed {
                    self.distributed.enabled = true;
                }
                if let Some(block_id) = args.block_id {
                    self.distributed.block_id = Some(block_id);
                }
                if let Some(total_blocks) = args.total_blocks {
                    self.distributed.total_blocks = total_blocks;
                }
                if let Some(ref path) = args.pipeline_config {
                    self.distributed.pipeline_config_path = Some(path.clone());
                }
                if args.coordinator {
                    self.distributed.coordinator_mode = true;
                }

                self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);
            }
            _ => {}
        }
        self
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.node_id.trim().is_empty() {
            return Err(anyhow!("node_id: must not be empty"));
        }
        if !self
            .node_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow!(
                "node_id: must contain only alphanumeric, dash, underscore"
            ));
        }

        std::fs::create_dir_all(&self.data_dir).with_context(|| {
            format!(
                "data_dir: failed to create or access directory: {}",
                self.data_dir.display()
            )
        })?;

        let test_path = self.data_dir.join(".write_test");
        let write_res = std::fs::write(&test_path, b"");
        if let Err(e) = write_res {
            use std::io::ErrorKind;
            if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) {
                let fallback = std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("aigen-data");
                std::fs::create_dir_all(&fallback).with_context(|| {
                    format!(
                        "data_dir: failed to create fallback directory: {}",
                        fallback.display()
                    )
                })?;
                let fallback_test = fallback.join(".write_test");
                std::fs::write(&fallback_test, b"").with_context(|| {
                    format!("data_dir: fallback not writable: {}", fallback.display())
                })?;
                let _ = std::fs::remove_file(&fallback_test);
                self.data_dir = fallback.clone();
                self.derive_model_paths();
                self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);
                eprintln!(
                    "data_dir not writable; using fallback: {}",
                    self.data_dir.display()
                );
            } else {
                return Err(e).with_context(|| {
                    format!("data_dir: not writable: {}", self.data_dir.display())
                });
            }
        } else {
            let _ = std::fs::remove_file(&test_path);
        }

        validate_multiaddr_with_field("network.listen_addr", &self.network.listen_addr)?;
        for (idx, a) in self.network.bootstrap_peers.iter().enumerate() {
            validate_multiaddr_with_field(&format!("network.bootstrap_peers[{idx}]"), a)?;
        }

        if self.network.max_peers < 1 || self.network.max_peers > 10_000 {
            return Err(anyhow!("network.max_peers: must be between 1 and 10000"));
        }

        if self.node.node_type == NodeType::Validator {
            if self
                .node
                .validator_address
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(anyhow!(
                    "node.validator_address: required for validator node_type"
                ));
            }
            if self.node.stake_amount.map(|a| a.value()).unwrap_or(0) == 0 {
                return Err(anyhow!(
                    "node.stake_amount: required and must be > 0 for validator node_type"
                ));
            }
        }

        if let Some(parent) = self.keypair_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "keypair_path: failed to create parent directory: {}",
                    parent.display()
                )
            })?;
        }

        if self.rpc.rpc_max_connections < 1 || self.rpc.rpc_max_connections > 100_000 {
            return Err(anyhow!(
                "rpc.rpc_max_connections: must be between 1 and 100000"
            ));
        }
        if self.rpc.rpc_addr.port() == 0 {
            return Err(anyhow!("rpc.rpc_addr: port must not be 0"));
        }

        if self.rpc.rpc_addr.ip().is_loopback() {
            eprintln!("⚠️  RPC server binding to localhost only ({})", self.rpc.rpc_addr);
            eprintln!("    This will NOT be accessible from other machines.");
            eprintln!("    To allow external access, bind to 0.0.0.0:{} instead.", self.rpc.rpc_addr.port());
            eprintln!("    Update rpc_addr in your config file or use AIGEN_RPC_ADDR environment variable.");
        }

        if self.model.max_memory_mb == 0 {
            return Err(anyhow!("model.max_memory_mb: must be > 0"));
        }
        if self.verification.epsilon <= 0.0 {
            return Err(anyhow!("verification.epsilon: must be > 0"));
        }

        if self.model.worker_mode && self.model.core_model_id.is_none() {
            return Err(anyhow!("model.core_model_id: required when worker_mode is enabled"));
        }
        if self.model.download_timeout_secs == 0 {
            return Err(anyhow!("model.download_timeout_secs: must be > 0"));
        }
        if self.model.min_redundancy_nodes == 0 {
            return Err(anyhow!("model.min_redundancy_nodes: must be > 0"));
        }
        std::fs::create_dir_all(&self.model.model_storage_path).with_context(|| {
            format!(
                "model.model_storage_path: failed to create directory: {}",
                self.model.model_storage_path.display()
            )
        })?;

        // Validate distributed inference configuration
        if self.distributed.enabled {
            if self.distributed.block_id.is_none() {
                return Err(anyhow!(
                    "distributed.block_id: required when distributed mode is enabled"
                ));
            }
            if self.distributed.pipeline_config_path.is_none() {
                return Err(anyhow!(
                    "distributed.pipeline_config_path: required when distributed mode is enabled"
                ));
            }
            let block_id = self.distributed.block_id.unwrap();
            if block_id >= self.distributed.total_blocks {
                return Err(anyhow!(
                    "distributed.block_id: must be less than total_blocks"
                ));
            }
            // Verify pipeline config file exists and is readable
            if let Some(ref path) = self.distributed.pipeline_config_path {
                if !path.exists() {
                    return Err(anyhow!(
                        "distributed.pipeline_config_path: file does not exist: {}",
                        path.display()
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn default_config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("aigen").join("config.toml")
    }

    pub fn default_data_dir() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("aigen")
    }
}

impl Default for NodeConfiguration {
    fn default() -> Self {
        let data_dir = Self::default_data_dir();
        Self {
            node_id: "node-1".to_string(),
            data_dir: data_dir.clone(),
            model: ModelConfig::default_with_data_dir(&data_dir),
            verification: InferenceVerificationConfig::default(),
            ads: AdConfig::default(),
            network: NetworkConfig::default(),
            genesis: GenesisConfig::default(),
            node: NodeConfig {
                node_type: NodeType::FullNode,
                validator_address: None,
                stake_amount: None,
            },
            rpc: RpcConfig::default(),
            keypair_path: crate::keypair::default_keypair_path(&data_dir),
            distributed: DistributedInferenceConfig::default(),
            training: TrainingConfig::default(),
        }
    }
}

fn parse_node_type(s: &str) -> Result<NodeType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "miner" => Ok(NodeType::Miner),
        "validator" => Ok(NodeType::Validator),
        "full_node" | "fullnode" | "full" => Ok(NodeType::FullNode),
        "light_client" | "lightclient" | "light" => Ok(NodeType::LightClient),
        _ => Err(anyhow!("node.node_type: invalid value '{s}'")),
    }
}

fn validate_multiaddr_with_field(field: &str, addr: &Multiaddr) -> Result<()> {
    use libp2p::multiaddr::Protocol;

    let mut tcp_port: Option<u16> = None;
    for proto in addr.iter() {
        if let Protocol::Tcp(port) = proto {
            tcp_port = Some(port);
        }
    }

    if let Some(port) = tcp_port {
        if port < 1024 {
            eprintln!(
                "port is privileged (<1024); requires elevated privileges: field={}, port={}",
                field, port
            );
        }
    }

    Ok(())
}
