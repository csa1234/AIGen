use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use libp2p::PeerId;
use blockchain_core::types::{Timestamp, Amount};
use network::protocol::NodeCapabilities;
use uuid::Uuid;
use crate::task::ComputeTask;

pub type NodeId = String;
pub type TaskId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeRole {
    Inference,
    Training,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub node_id: String,
    #[serde(skip)]
    pub peer_id: Option<PeerId>,
    pub role: NodeRole, // Inference | Training | Both
    pub vram_total_gb: f32,
    pub vram_free_gb: f32,
    pub vram_allocated_gb: f32,
    pub region: Option<String>,
    pub last_heartbeat: Timestamp,
    pub load_score: f32, // 0.0-1.0, computed from active tasks
    pub stake: Amount,
    pub capabilities: NodeCapabilities,
    pub rtt_map: HashMap<String, Duration>, // node_id -> RTT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentLocation {
    pub fragment_id: String,
    pub model_id: String,
    pub fragment_index: u32,
    pub size_bytes: u64, // Added for routing
    pub replicas: Vec<NodeId>, // K=3 replicas
}

pub struct GlobalState {
    pub nodes: DashMap<String, NodeState>,
    pub fragments: DashMap<String, FragmentLocation>, // fragment_id -> location
    pub active_tasks: DashMap<TaskId, ComputeTask>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            nodes: DashMap::new(),
            fragments: DashMap::new(),
            active_tasks: DashMap::new(),
        }
    }

    pub fn update_node_state(&self, state: NodeState) {
        self.nodes.insert(state.node_id.clone(), state);
    }
    
    pub fn update_node_from_announcement(
        &self,
        node_id: String,
        peer_id: Option<PeerId>,
        vram_total_gb: f32,
        vram_free_gb: f32,
        vram_allocated_gb: f32,
        capabilities: NodeCapabilities,
        timestamp: Timestamp,
    ) {
        if let Some(mut node) = self.nodes.get_mut(&node_id) {
            // Update existing entry
            node.vram_free_gb = vram_free_gb;
            node.vram_total_gb = vram_total_gb;
            node.vram_allocated_gb = vram_allocated_gb;
            node.capabilities = capabilities;
            node.last_heartbeat = timestamp;
            if let Some(pid) = peer_id {
                node.peer_id = Some(pid);
            }
        } else {
            // Insert new NodeState
            let new_node = NodeState {
                node_id: node_id.clone(),
                peer_id,
                role: NodeRole::Inference,
                vram_total_gb,
                vram_free_gb,
                vram_allocated_gb,
                region: None,
                last_heartbeat: timestamp,
                load_score: 0.0,
                stake: Amount::ZERO,
                capabilities,
                rtt_map: HashMap::new(),
            };
            self.nodes.insert(node_id, new_node);
        }
    }

    pub fn get_nodes_with_fragment(&self, fragment_id: &str) -> Result<Vec<NodeState>, crate::scheduler::SchedulerError> {
        let location = self.fragments.get(fragment_id)
            .ok_or(crate::scheduler::SchedulerError::FragmentNotFound(fragment_id.to_string()))?;
            
        let mut nodes = Vec::new();
        for node_id in &location.replicas {
            if let Some(node) = self.nodes.get(node_id) {
                nodes.push(node.clone());
            }
        }
        Ok(nodes)
    }

    pub fn allocate_task(&self, task: &ComputeTask, node_id: &str) {
        if let Some(mut node) = self.nodes.get_mut(node_id) {
            // update load score roughly
            node.load_score = (node.load_score + 0.1).min(1.0);
            self.active_tasks.insert(task.task_id, task.clone());
        }
    }
}
