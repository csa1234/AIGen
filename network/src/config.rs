use libp2p::Multiaddr;
 use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GossipsubConfig {
    pub mesh_n: usize,
    pub mesh_n_low: usize,
    pub mesh_n_high: usize,
    pub heartbeat_ms: u64,
    pub max_transmit_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReputationConfig {
    pub ban_threshold: f64,
    pub cleanup_interval_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(with = "multiaddr_serde")]
    pub listen_addr: Multiaddr,
    #[serde(with = "multiaddr_serde::vec")]
    pub bootstrap_peers: Vec<Multiaddr>,
    pub max_peers: usize,
    pub enable_mdns: bool,
    pub gossipsub_config: GossipsubConfig,
    pub reputation_config: ReputationConfig,
}

mod multiaddr_serde {
    use std::fmt;
    use std::str::FromStr;

    use libp2p::Multiaddr;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Multiaddr, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Multiaddr, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Multiaddr::from_str(&s)
            .map_err(|e| D::Error::custom(format!("invalid multiaddr '{s}': {e}")))
    }

    pub mod vec {
        use std::str::FromStr;

        use libp2p::Multiaddr;
        use serde::de::Error as _;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(value: &[Multiaddr], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let out: Vec<String> = value.iter().map(|m| m.to_string()).collect();
            out.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Multiaddr>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let raw = Vec::<String>::deserialize(deserializer)?;
            raw.into_iter()
                .map(|s| {
                    Multiaddr::from_str(&s)
                        .map_err(|e| D::Error::custom(format!("invalid multiaddr '{s}': {e}")))
                })
                .collect()
        }
    }

    fn _silent_import_for_fmt(_v: fmt::Error) {}
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/9000".parse().unwrap(),
            bootstrap_peers: Vec::new(),
            max_peers: 50,
            enable_mdns: true,
            gossipsub_config: GossipsubConfig {
                mesh_n: 6,
                mesh_n_low: 4,
                mesh_n_high: 12,
                heartbeat_ms: 700,
                max_transmit_size: 10 * 1024 * 1024,
            },
            reputation_config: ReputationConfig {
                ban_threshold: 0.2,
                cleanup_interval_secs: 60 * 60,
            },
        }
    }
}
