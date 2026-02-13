use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use libp2p::PeerId;
use blockchain_core::types::{Timestamp, Amount};
use blockchain_core::state::ChainState;
use network::protocol::NodeCapabilities;
use uuid::Uuid;
use crate::task::ComputeTask;

pub type NodeId = String;
pub type TaskId = Uuid;

/// Minimum stake requirements for different node roles
pub const MIN_STAKE_INFERENCE: u64 = 1_000; // 1000 AIGEN
pub const MIN_STAKE_TRAINING: u64 = 5_000;  // 5000 AIGEN

/// Default and adaptive fragment size constants (in bytes)
pub const MIN_FRAGMENT_SIZE_BYTES: u64 = 100 * 1024 * 1024;   // 100MB
pub const MAX_FRAGMENT_SIZE_BYTES: u64 = 500 * 1024 * 1024;   // 500MB
pub const DEFAULT_FRAGMENT_SIZE_BYTES: u64 = 200 * 1024 * 1024; // 200MB

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
    // NEW: Bidding fields for market-based task assignment
    pub bid_price_per_task: Option<Amount>, // None = default pricing
    pub accepts_bids: bool,
}

impl NodeState {
    /// Check if node meets minimum stake requirement for its role
    pub fn meets_stake_requirement(&self) -> bool {
        match self.role {
            NodeRole::Inference => self.stake.value() >= MIN_STAKE_INFERENCE,
            NodeRole::Training => self.stake.value() >= MIN_STAKE_TRAINING,
            NodeRole::Both => self.stake.value() >= MIN_STAKE_TRAINING,
        }
    }

    /// Calculate optimal fragment size for this node based on RTT and VRAM
    pub fn calculate_optimal_fragment_size(&self, avg_rtt_ms: f32) -> u64 {
        // Base size by RTT:
        // - RTT < 10ms (LAN): 500MB
        // - RTT 10-50ms (regional): 300MB
        // - RTT > 50ms (WAN): 100MB
        let base_size = if avg_rtt_ms < 10.0 {
            MAX_FRAGMENT_SIZE_BYTES
        } else if avg_rtt_ms < 50.0 {
            300 * 1024 * 1024 // 300MB
        } else {
            MIN_FRAGMENT_SIZE_BYTES
        };

        // Cap at 200MB if low VRAM (< 2GB free)
        if self.vram_free_gb < 2.0 {
            base_size.min(200 * 1024 * 1024)
        } else {
            base_size
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentLocation {
    pub fragment_id: String,
    pub model_id: String,
    pub fragment_index: u32,
    pub size_bytes: u64, // Added for routing
    pub replicas: Vec<NodeId>, // K=3 replicas
    // NEW: Adaptive fragment size for this fragment
    pub target_fragment_size_bytes: u64,
}

pub struct GlobalState {
    pub nodes: DashMap<String, NodeState>,
    pub fragments: DashMap<String, FragmentLocation>, // fragment_id -> location
    pub active_tasks: DashMap<TaskId, ComputeTask>,
    // NEW: Checkpoint storage
    pub checkpoints: DashMap<TaskId, Vec<CheckpointRecord>>,
    // NEW: Reward tracking per node
    pub node_rewards: DashMap<String, RewardRecord>,
    // NEW: Detailed reward history for RPC queries
    pub reward_history: DashMap<String, Vec<RewardHistoryEntry>>,
    // NEW: Chain state for stake queries
    pub chain_state: Arc<ChainState>,
}

impl GlobalState {
    pub fn new(chain_state: Arc<ChainState>) -> Self {
        Self {
            nodes: DashMap::new(),
            fragments: DashMap::new(),
            active_tasks: DashMap::new(),
            checkpoints: DashMap::new(),
            node_rewards: DashMap::new(),
            reward_history: DashMap::new(),
            chain_state,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardRecord {
    pub node_id: String,
    pub total_earned: Amount,
    pub compute_rewards: Amount,
    pub storage_rewards: Amount,
    pub last_reward_timestamp: i64,
    pub tasks_completed: u64,
    pub fragments_hosted: u32,
}

impl RewardRecord {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            total_earned: Amount::ZERO,
            compute_rewards: Amount::ZERO,
            storage_rewards: Amount::ZERO,
            last_reward_timestamp: 0,
            tasks_completed: 0,
            fragments_hosted: 0,
        }
    }
}

/// Individual reward event for detailed history tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardHistoryEntry {
    pub node_id: String,
    pub amount: Amount,
    pub reward_type: String, // "compute" or "storage"
    pub timestamp: i64,
    pub task_id: Option<String>,
    pub inference_id: Option<String>,
    pub proof_verified: bool,
}

impl RewardHistoryEntry {
    pub fn new(
        node_id: String,
        amount: Amount,
        reward_type: String,
        timestamp: i64,
        task_id: Option<String>,
        inference_id: Option<String>,
        proof_verified: bool,
    ) -> Self {
        Self {
            node_id,
            amount,
            reward_type,
            timestamp,
            task_id,
            inference_id,
            proof_verified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub checkpoint_hash: [u8; 32],
    pub layer_range: (u32, u32),
    pub timestamp: i64,
    pub node_id: String, // Node that created checkpoint
}

impl GlobalState {
    /// Store checkpoint for a task
    pub fn store_checkpoint(&self, record: CheckpointRecord) {
        self.checkpoints
            .entry(record.task_id)
            .or_insert_with(Vec::new)
            .push(record);
    }

    /// Get latest checkpoint for a task
    pub fn get_latest_checkpoint(&self, task_id: &Uuid) -> Option<CheckpointRecord> {
        self.checkpoints
            .get(task_id)
            .and_then(|records| records.iter().max_by_key(|r| r.timestamp).cloned())
    }

    /// Cleanup old checkpoints (keep last 3 per task)
    pub fn cleanup_old_checkpoints(&self, task_id: &Uuid) {
        if let Some(mut records) = self.checkpoints.get_mut(task_id) {
            records.sort_by_key(|r| r.timestamp);
            let len = records.len();
            if len > 3 {
                records.drain(0..len - 3);
            }
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
        // Query stake from chain state
        let stake = self.get_stake_from_chain(&node_id);
        
        if let Some(mut node) = self.nodes.get_mut(&node_id) {
            // Update existing entry
            node.vram_free_gb = vram_free_gb;
            node.vram_total_gb = vram_total_gb;
            node.vram_allocated_gb = vram_allocated_gb;
            node.capabilities = capabilities;
            node.last_heartbeat = timestamp;
            node.stake = stake; // Update stake from chain
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
                stake, // Set stake from chain query
                capabilities,
                rtt_map: HashMap::new(),
                // NEW: Bidding fields (default to None/false)
                bid_price_per_task: None,
                accepts_bids: false,
            };
            self.nodes.insert(node_id, new_node);
        }
    }

    /// Query stake from chain state for a node
    fn get_stake_from_chain(&self, node_id: &str) -> Amount {
        // Query actual stake, not balance
        let amount = self.chain_state.get_staked_amount(node_id);
        if amount.value() > 0 {
            eprintln!("Stake query for node {}: {} AIGEN", node_id, amount.value());
        } else {
            eprintln!("No stake found for node {}. Defaulting to ZERO.", node_id);
        }
        amount
    }

    /// Refresh stakes for all nodes from chain state
    pub fn refresh_all_stakes(&self) {
        eprintln!("Refreshing stakes for all {} nodes...", self.nodes.len());
        for mut node in self.nodes.iter_mut() {
            let new_stake = self.get_stake_from_chain(&node.node_id);
            if node.stake.value() != new_stake.value() {
                eprintln!(
                    "Node {} stake updated: {} -> {} AIGEN",
                    node.node_id,
                    node.stake.value(),
                    new_stake.value()
                );
                node.stake = new_stake;
            }
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

    /// Record a reward for a node
    pub fn record_reward(
        &self,
        node_id: &str,
        amount: Amount,
        storage_reward: Amount,
        is_compute: bool,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut record = self.node_rewards.entry(node_id.to_string()).or_insert_with(|| {
            RewardRecord::new(node_id.to_string())
        });

        record.last_reward_timestamp = now;

        if is_compute {
            // Add compute reward to both total and compute_rewards
            record.total_earned = Amount::new(record.total_earned.value().saturating_add(amount.value()));
            record.compute_rewards = Amount::new(record.compute_rewards.value().saturating_add(amount.value()));
            record.tasks_completed += 1;
        } else {
            // Add storage reward to both total and storage_rewards
            // Use the storage_reward parameter (not amount) for storage rewards
            record.total_earned = Amount::new(record.total_earned.value().saturating_add(storage_reward.value()));
            record.storage_rewards = Amount::new(record.storage_rewards.value().saturating_add(storage_reward.value()));
        }
    }

    /// Get node earnings for RPC queries
    pub fn get_node_earnings(&self, node_id: &str) -> Option<RewardRecord> {
        self.node_rewards.get(node_id).map(|r| r.clone())
    }

    /// Get all nodes hosting fragments (for storage rewards)
    pub fn get_nodes_hosting_fragments(&self) -> HashMap<String, Vec<String>> {
        let mut node_fragments: HashMap<String, Vec<String>> = HashMap::new();

        for fragment_entry in self.fragments.iter() {
            let fragment = fragment_entry.value();
            for replica_node_id in &fragment.replicas {
                node_fragments
                    .entry(replica_node_id.clone())
                    .or_default()
                    .push(fragment.fragment_id.clone());
            }
        }

        node_fragments
    }

    /// Calculate VRAM allocated for a given node
    pub fn calculate_vram_allocated(&self, node_id: &str) -> f32 {
        let mut total_bytes: u64 = 0;

        for fragment_entry in self.fragments.iter() {
            let fragment = fragment_entry.value();
            if fragment.replicas.contains(&node_id.to_string()) {
                total_bytes += fragment.size_bytes;
            }
        }

        // Convert to GB
        total_bytes as f32 / 1_073_741_824.0 // 1024^3
    }

    /// Update fragments hosted count for reward records
    pub fn update_fragments_hosted(&self, node_id: &str) {
        let count = self.fragments.iter()
            .filter(|f| f.value().replicas.contains(&node_id.to_string()))
            .count() as u32;

        if let Some(mut record) = self.node_rewards.get_mut(node_id) {
            record.fragments_hosted = count;
        }
    }

    /// Record a reward event to history
    pub fn record_reward_event(&self, entry: RewardHistoryEntry) {
        let mut history = self.reward_history.entry(entry.node_id.clone()).or_insert_with(Vec::new);
        history.push(entry);
        
        // Keep only last 1000 entries per node to prevent unbounded growth
        if history.len() > 1000 {
            history.remove(0);
        }
    }

    /// Get reward history for a node
    pub fn get_reward_history(&self, node_id: &str, limit: usize) -> Vec<RewardHistoryEntry> {
        self.reward_history
            .get(node_id)
            .map(|h| {
                let mut entries: Vec<_> = h.clone();
                // Sort by timestamp descending (newest first)
                entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                // Apply limit
                entries.truncate(limit);
                entries
            })
            .unwrap_or_default()
    }
}
