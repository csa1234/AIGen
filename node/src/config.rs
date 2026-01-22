use std::path::{Path, PathBuf};
use std::net::SocketAddr;

use anyhow::{anyhow, Context, Result};
use blockchain_core::types::Amount;
use config as config_rs;
use consensus::InferenceVerificationConfig;
use genesis::GenesisConfig;
use libp2p::Multiaddr;
use network::config::NetworkConfig;
use network::node_types::{NodeConfig, NodeType};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NodeConfiguration {
    pub node_id: String,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub verification: InferenceVerificationConfig,
    pub network: NetworkConfig,
    pub genesis: GenesisConfig,
    pub node: NodeConfig,
    pub rpc: RpcConfig,
    pub keypair_path: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ModelConfig {
    pub cache_dir: PathBuf,
    pub max_memory_mb: usize,
    pub num_threads: usize,
}

impl ModelConfig {
    pub fn default_with_data_dir(data_dir: &Path) -> Self {
        Self {
            cache_dir: data_dir.join("model-cache"),
            max_memory_mb: 2048,
            num_threads: 0,
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
        if cfg.model.cache_dir.as_os_str().is_empty() {
            cfg.model.cache_dir = cfg.data_dir.join("model-cache");
        }
        Ok(cfg)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config parent directory: {}", parent.display()))?;
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

    pub fn merge_with_env(mut self) -> Self {
        if let Ok(v) = std::env::var("AIGEN_NODE_ID") {
            self.node_id = v;
        }
        if let Ok(v) = std::env::var("AIGEN_DATA_DIR") {
            self.data_dir = PathBuf::from(v);
            self.model.cache_dir = self.data_dir.join("model-cache");
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
                    self.model.cache_dir = self.data_dir.join("model-cache");
                }
                self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);
            }
            crate::cli::Commands::Start(args) => {
                if let Some(v) = &args.data_dir {
                    self.data_dir = v.clone();
                    self.model.cache_dir = self.data_dir.join("model-cache");
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

                self.keypair_path = crate::keypair::default_keypair_path(&self.data_dir);
            }
            _ => {}
        }
        self
    }

    pub fn validate(&self) -> Result<()> {
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
        std::fs::write(&test_path, b"")
            .with_context(|| format!("data_dir: not writable: {}", self.data_dir.display()))?;
        let _ = std::fs::remove_file(&test_path);

        validate_multiaddr_with_field("network.listen_addr", &self.network.listen_addr)?;
        for (idx, a) in self.network.bootstrap_peers.iter().enumerate() {
            validate_multiaddr_with_field(&format!("network.bootstrap_peers[{idx}]"), a)?;
        }

        if self.network.max_peers < 1 || self.network.max_peers > 10_000 {
            return Err(anyhow!(
                "network.max_peers: must be between 1 and 10000"
            ));
        }

        if self.node.node_type == NodeType::Validator {
            if self.node.validator_address.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(anyhow!("node.validator_address: required for validator node_type"));
            }
            if self.node.stake_amount.map(|a| a.value()).unwrap_or(0) == 0 {
                return Err(anyhow!("node.stake_amount: required and must be > 0 for validator node_type"));
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

        if self.model.max_memory_mb == 0 {
            return Err(anyhow!("model.max_memory_mb: must be > 0"));
        }
        if self.verification.epsilon <= 0.0 {
            return Err(anyhow!("verification.epsilon: must be > 0"));
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
            network: NetworkConfig::default(),
            genesis: GenesisConfig::default(),
            node: NodeConfig {
                node_type: NodeType::FullNode,
                validator_address: None,
                stake_amount: None,
            },
            rpc: RpcConfig::default(),
            keypair_path: crate::keypair::default_keypair_path(&data_dir),
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
            eprintln!("port is privileged (<1024); requires elevated privileges: field={}, port={}", field, port);
        }
    }

    Ok(())
}
