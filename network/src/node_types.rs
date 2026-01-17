use blockchain_core::types::Amount;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Miner,
    Validator,
    FullNode,
    LightClient,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_type: NodeType,
    pub validator_address: Option<String>,
    pub stake_amount: Option<Amount>,
}
