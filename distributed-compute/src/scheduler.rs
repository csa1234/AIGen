use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use thiserror::Error;
use consensus::validator::{ValidatorRegistry, select_validators_weighted};
use consensus::poi::{PoIProof, calculate_poi_reward, verify_distributed_task};
use network::protocol::NetworkMessage;
use model::registry::ModelRegistry;
use model::ContinualLearningManager;
use blockchain_core::types::Amount;
use blockchain_core::state::ChainState;
use blockchain_core::{RewardTx, RewardType};
use blockchain_core::crypto::{SecretKey, sign_message};
use genesis::{CeoTransactable, CeoSignature};
use uuid::Uuid;
use crate::state::GlobalState;
use crate::routing::RouteSelector;
use crate::task::{InferenceTask, TaskPlan, decompose_inference};
use hex;

/// Hourly storage reward pool (10,000 AIGEN tokens per hour)
const HOURLY_STORAGE_POOL: u64 = 10_000;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("not elected leader")]
    NotLeader,
    #[error("model not found: {0}")]
    ModelNotFound(String),
    #[error("fragment not found: {0}")]
    FragmentNotFound(String),
    #[error("no available node for fragment: {0}")]
    NoAvailableNode(String),
    #[error("consensus error: {0}")]
    ConsensusError(#[from] consensus::poi::ConsensusError),
    #[error("network error: {0}")]
    NetworkError(#[from] tokio::sync::mpsc::error::SendError<NetworkMessage>),
    #[error("chain state error: {0}")]
    ChainStateError(String),
}

pub struct DynamicScheduler {
    state: Arc<GlobalState>,
    router: RouteSelector,
    is_leader: Arc<RwLock<bool>>,
    validator_registry: Arc<ValidatorRegistry>,
    network_tx: mpsc::Sender<NetworkMessage>,
    local_validator_address: String,
    registry: Arc<ModelRegistry>,
    chain_state: Arc<ChainState>,
    reward_pool: Arc<RwLock<HashMap<String, (Amount, RewardType)>>>,
    pending_reward_count: Arc<RwLock<u32>>,
    ceo_secret_key: Option<SecretKey>,
    training_coordinator: Arc<crate::training::TrainingCoordinator>,
    /// Continual Learning manager for unified buffer and Fisher matrix handling
    cl_manager: Option<Arc<ContinualLearningManager>>,
    training_enabled: Arc<RwLock<bool>>,
}

impl DynamicScheduler {
    pub fn new(
        state: Arc<GlobalState>,
        router: RouteSelector,
        validator_registry: Arc<ValidatorRegistry>,
        network_tx: mpsc::Sender<NetworkMessage>,
        local_validator_address: String,
        registry: Arc<ModelRegistry>,
        chain_state: Arc<ChainState>,
        cl_manager: Option<Arc<ContinualLearningManager>>,
    ) -> Self {
        // Load CEO secret key from file if available
        let ceo_secret_key = Self::load_ceo_key();
        if ceo_secret_key.is_none() {
            eprintln!("Warning: CEO key not loaded. Reward transactions will not be signed.");
        }

        // Initialize training coordinator with CL manager
        let training_config = crate::training::TrainingConfig::default();
        let training_coordinator = Arc::new(crate::training::TrainingCoordinator::new(
            local_validator_address.clone(),
            network_tx.clone(),
            3, // Default to 3 nodes for secret sharing
            training_config,
            cl_manager.clone(),
        ));

        Self {
            state,
            router,
            is_leader: Arc::new(RwLock::new(false)),
            validator_registry,
            network_tx,
            local_validator_address,
            registry,
            chain_state,
            reward_pool: Arc::new(RwLock::new(HashMap::new())),
            pending_reward_count: Arc::new(RwLock::new(0)),
            ceo_secret_key,
            training_coordinator,
            cl_manager,
            training_enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Load CEO secret key from workspace root file
    fn load_ceo_key() -> Option<SecretKey> {
        let key_path = std::path::PathBuf::from("ceo_key.priv.hex");
        if !key_path.exists() {
            return None;
        }

        match std::fs::read_to_string(&key_path) {
            Ok(hex_str) => {
                let hex_clean = hex_str.trim();
                match hex::decode(hex_clean) {
                    Ok(bytes) if bytes.len() == 32 => {
                        let mut key_bytes = [0u8; 32];
                        key_bytes.copy_from_slice(&bytes);
                        Some(SecretKey::from_bytes(&key_bytes))
                    }
                    _ => {
                        eprintln!("Invalid CEO key format in {}", key_path.display());
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read CEO key: {}", e);
                None
            }
        }
    }

    pub async fn is_leader(&self) -> bool {
        *self.is_leader.read().await
    }

    pub async fn elect_leader(&self) -> Result<bool, SchedulerError> {
        let validators = self.validator_registry.get_active_validators();
        let selected = select_validators_weighted(&validators, 1)?;
        
        if let Some(leader) = selected.first() {
            let is_me = leader.address == self.local_validator_address;
            *self.is_leader.write().await = is_me;
            Ok(is_me)
        } else {
            Ok(false)
        }
    }
    
    pub async fn schedule_inference(&self, request: InferenceTask) -> Result<TaskPlan, SchedulerError> {
        if !*self.is_leader.read().await {
            return Err(SchedulerError::NotLeader);
        }
        
        // 1. Decompose into tasks
        let mut plan = decompose_inference(&request, &self.state, &self.registry)?;
        
        // 2. Filter nodes by stake requirements BEFORE route selection
        self.filter_nodes_by_stake(&mut plan)?;
        
        // 3. Calculate adaptive fragment sizes based on RTT
        self.calculate_adaptive_fragment_sizes(&mut plan)?;
        
        // 4. Select routes (now with stake-filtered nodes)
        self.router.select_route(&mut plan)?;
        
        // 5. Send ComputeTask messages to assigned nodes
        for task in &plan.tasks {
            let msg = NetworkMessage::ComputeTask {
                task_id: task.task_id,
                inference_id: task.inference_id,
                model_id: task.model_id.clone(),
                layer_range: task.layer_range,
                required_fragments: task.required_fragments.clone(),
                assigned_node: task.assigned_node.clone(),
                input_activation_ref: None,
                tensor_shard_index: 0,
                total_tensor_shards: 1,
            };
            self.network_tx.send(msg).await?;
        }
        
        Ok(plan)
    }

    /// Filter out nodes that don't meet stake requirements
    fn filter_nodes_by_stake(&self, plan: &mut TaskPlan) -> Result<(), SchedulerError> {
        for task in &mut plan.tasks {
            let mut rejected_nodes = Vec::new();
            
            // Check each required fragment's candidate nodes
            for fragment_id in &task.required_fragments {
                if let Ok(candidates) = self.state.get_nodes_with_fragment(fragment_id) {
                    for node in candidates {
                        // Enforce stake requirement - nodes with 0 stake are rejected
                        if !node.meets_stake_requirement() {
                            let min_stake = match node.role {
                                crate::state::NodeRole::Inference => crate::state::MIN_STAKE_INFERENCE,
                                crate::state::NodeRole::Training => crate::state::MIN_STAKE_TRAINING,
                                crate::state::NodeRole::Both => crate::state::MIN_STAKE_TRAINING,
                            };
                            eprintln!(
                                "Node {} rejected: insufficient stake ({} < {})",
                                node.node_id,
                                node.stake.value(),
                                min_stake
                            );
                            rejected_nodes.push(node.node_id.clone());
                        }
                    }
                }
            }
            
            // Store rejected nodes for route selector to skip
            task.rejected_nodes = rejected_nodes;
        }
        
        Ok(())
    }

    /// Calculate adaptive fragment sizes based on network RTT
    fn calculate_adaptive_fragment_sizes(&self, plan: &mut TaskPlan) -> Result<(), SchedulerError> {
        for task in &mut plan.tasks {
            // Calculate average RTT from selected nodes
            let avg_rtt_ms = self.calculate_avg_rtt_for_task(task);
            
            // Store adaptive fragment size hint in task
            task.target_fragment_size_bytes = if avg_rtt_ms > 0.0 {
                // Use first available node to calculate optimal size
                if let Some(fragment_id) = task.required_fragments.first() {
                    if let Ok(candidates) = self.state.get_nodes_with_fragment(fragment_id) {
                        if let Some(node) = candidates.first() {
                            node.calculate_optimal_fragment_size(avg_rtt_ms)
                        } else {
                            crate::state::DEFAULT_FRAGMENT_SIZE_BYTES
                        }
                    } else {
                        crate::state::DEFAULT_FRAGMENT_SIZE_BYTES
                    }
                } else {
                    crate::state::DEFAULT_FRAGMENT_SIZE_BYTES
                }
            } else {
                crate::state::DEFAULT_FRAGMENT_SIZE_BYTES
            };
        }
        
        Ok(())
    }

    /// Calculate average RTT for a task based on candidate nodes
    fn calculate_avg_rtt_for_task(&self, task: &crate::task::ComputeTask) -> f32 {
        let mut total_rtt_ms = 0.0;
        let mut rtt_count = 0;
        
        for fragment_id in &task.required_fragments {
            if let Ok(candidates) = self.state.get_nodes_with_fragment(fragment_id) {
                for node in candidates {
                    // Average RTT from this node's perspective
                    let avg_node_rtt: f32 = if node.rtt_map.is_empty() {
                        50.0 // Default 50ms if no RTT data
                    } else {
                        node.rtt_map.values()
                            .map(|d| d.as_millis() as f32)
                            .sum::<f32>() / node.rtt_map.len() as f32
                    };
                    total_rtt_ms += avg_node_rtt;
                    rtt_count += 1;
                }
            }
        }
        
        if rtt_count > 0 {
            total_rtt_ms / rtt_count as f32
        } else {
            50.0 // Default to 50ms (regional network)
        }
    }

    /// Monitor for node failures and trigger reassignment
    pub async fn monitor_failures(&self) -> Result<(), SchedulerError> {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            let failed_nodes = self.detect_failed_nodes().await;
            
            for node_id in failed_nodes {
                self.handle_node_failure(&node_id).await?;
            }
        }
    }
    
    /// Detect nodes that missed 3+ heartbeats and slash them
    async fn detect_failed_nodes(&self) -> Vec<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let threshold = Duration::from_millis(1500); // 3 x 500ms
        
        let failed_nodes: Vec<String> = self.state.nodes
            .iter()
            .filter(|entry| {
                let node = entry.value();
                let elapsed = now.saturating_sub(node.last_heartbeat.0);
                elapsed > threshold.as_secs() as i64
            })
            .map(|entry| entry.key().clone())
            .collect();
        
        // NEW: Slash failed nodes for heartbeat misses (5% per hour down)
        for node_id in &failed_nodes {
            if let Some(node) = self.state.nodes.get(node_id) {
                let elapsed_secs = now.saturating_sub(node.last_heartbeat.0);
                let hours_down = (elapsed_secs / 3600).max(1);
                let slash_per_hour = (node.stake.value() * 5) / 100; // 5% per hour
                let total_slash = Amount::new(slash_per_hour * hours_down as u64);
                
                if let Err(e) = self.chain_state.apply_slash(node_id, total_slash) {
                    eprintln!("Failed to slash node {}: {}", node_id, e);
                } else {
                    eprintln!("Node {} slashed {} AIGEN for {} hours of downtime", 
                        node_id, total_slash.value(), hours_down);
                }
            }
        }
        
        failed_nodes
    }
    
    /// Handle node failure: reassign tasks to replicas
    async fn handle_node_failure(&self, failed_node: &str) -> Result<(), SchedulerError> {
        // 1. Find all active tasks assigned to failed node
        let failed_tasks: Vec<crate::task::ComputeTask> = self.state.active_tasks
            .iter()
            .filter(|entry| entry.value().assigned_node == failed_node)
            .map(|entry| entry.value().clone())
            .collect();
        
        // 2. For each task, find replica nodes and reassign
        for task in failed_tasks {
            self.reassign_task_to_replica(&task, failed_node).await?;
        }
        
        // 3. Mark node as dead in GlobalState
        self.state.nodes.remove(failed_node);
        
        Ok(())
    }
    
    /// Reassign task to a healthy replica node
    async fn reassign_task_to_replica(
        &self,
        task: &crate::task::ComputeTask,
        failed_node: &str,
    ) -> Result<(), SchedulerError> {
        // Find fragment replicas
        let fragment_id = &task.required_fragments[0];
        let candidates = self.state.get_nodes_with_fragment(fragment_id)?;
        
        // Filter out failed node
        let candidates: Vec<crate::state::NodeState> = candidates.into_iter()
            .filter(|n| n.node_id != failed_node)
            .collect();
        
        if candidates.is_empty() {
            return Err(SchedulerError::NoAvailableNode(fragment_id.clone()));
        }
        
        // Select best replica using routing logic (lowest load score)
        let best_replica = candidates.iter()
            .min_by(|a, b| a.load_score.partial_cmp(&b.load_score).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();
        
        // Get latest checkpoint for this task
        let checkpoint = self.get_latest_checkpoint(&task.task_id).await?;
        
        // Send ReassignFragment message
        let msg = NetworkMessage::ReassignFragment {
            task_id: task.task_id,
            inference_id: task.inference_id,
            old_node: failed_node.to_string(),
            new_node: best_replica.node_id.clone(),
            checkpoint_ref: checkpoint.clone(),
        };
        self.network_tx.send(msg).await?;
        
        // Send Failover message to new node
        if let Some(cp) = checkpoint {
            let failover_msg = NetworkMessage::Failover {
                task_id: task.task_id,
                inference_id: task.inference_id,
                failed_node: failed_node.to_string(),
                replacement_node: best_replica.node_id.clone(),
                resume_from_checkpoint: cp.checkpoint_hash,
            };
            self.network_tx.send(failover_msg).await?;
        }
        
        // Update task assignment in GlobalState
        if let Some(mut active_task) = self.state.active_tasks.get_mut(&task.task_id) {
            active_task.assigned_node = best_replica.node_id.clone();
            active_task.status = crate::task::TaskStatus::Pending;
        }
        
        Ok(())
    }
    
    /// Retrieve latest checkpoint for a task
    async fn get_latest_checkpoint(&self, task_id: &Uuid) -> Result<Option<network::protocol::CheckpointRef>, SchedulerError> {
        // Query checkpoint storage from GlobalState
        Ok(self.state.get_latest_checkpoint(task_id).map(|record| network::protocol::CheckpointRef {
            checkpoint_hash: record.checkpoint_hash,
            layer_range: record.layer_range,
            location: record.node_id,
        }))
    }

    /// Start the scheduler with failure monitoring
    pub async fn start_with_monitoring(self: Arc<Self>) {
        // Spawn failure monitor task
        let scheduler = self.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler.monitor_failures().await {
                eprintln!("Failure monitor error: {}", e);
            }
        });

        // Spawn storage reward distribution task
        let scheduler = self.clone();
        tokio::spawn(async move {
            scheduler.storage_reward_loop().await;
        });

        // Spawn periodic stake refresh task (every 300 seconds)
        let scheduler = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            interval.tick().await; // Skip first tick
            loop {
                interval.tick().await;
                scheduler.state.refresh_all_stakes();
            }
        });

        // Spawn federated learning training loop
        let scheduler = self.clone();
        tokio::spawn(async move {
            scheduler.start_training_loop().await;
        });
    }

    /// Background task for hourly storage reward distribution
    async fn storage_reward_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1 hour
        interval.tick().await; // Skip first tick

        loop {
            interval.tick().await;
            if let Err(e) = self.distribute_storage_rewards().await {
                eprintln!("Storage reward distribution error: {}", e);
            }
        }
    }

    /// Distribute hourly storage rewards based on fragment hosting
    pub async fn distribute_storage_rewards(&self) -> Result<(), SchedulerError> {
        // 1. Get total stake pool for weight calculation
        let total_stake: u64 = self.state.nodes.iter()
            .map(|n| n.value().stake.value())
            .sum();

        if total_stake == 0 {
            return Ok(()); // No stake to weight by
        }

        // 2. Get total VRAM pool
        let total_vram_gb: f32 = self.state.nodes.iter()
            .map(|n| n.value().vram_total_gb)
            .sum();

        if total_vram_gb == 0.0 {
            return Ok(()); // No VRAM to weight by
        }

        // 3. Get nodes hosting fragments
        let node_fragments = self.state.get_nodes_hosting_fragments();
        if node_fragments.is_empty() {
            return Ok(()); // No fragments hosted
        }

        // 4. Calculate and distribute rewards
        for (node_id, _fragments) in node_fragments {
            // Get node stake
            let node_stake = self.state.nodes.get(&node_id)
                .map(|n| n.stake.value())
                .unwrap_or(0);

            // Calculate VRAM allocated
            let vram_allocated_gb = self.state.calculate_vram_allocated(&node_id);

            // Calculate weights
            let stake_weight = node_stake as f32 / total_stake as f32;
            let vram_weight = vram_allocated_gb / total_vram_gb;

            // Calculate storage reward
            let base_reward = (HOURLY_STORAGE_POOL as f32 * stake_weight * vram_weight) as u64;
            
            // Minimum reward threshold (1 AIGEN)
            if base_reward < 1 {
                continue;
            }

            let reward = Amount::new(base_reward);

            // Accumulate in reward pool with Storage type
            {
                let mut pool = self.reward_pool.write().await;
                let (entry_amount, _) = pool.entry(node_id.clone()).or_insert((Amount::ZERO, RewardType::Storage));
                *entry_amount = Amount::new(entry_amount.value().saturating_add(reward.value()));
            }

            // Update reward records
            self.state.record_reward(&node_id, Amount::ZERO, reward, false);
            self.state.update_fragments_hosted(&node_id);

            println!(
                "Storage reward allocated for node {}: {} AIGEN (stake: {:.2}%, vram: {:.2}GB)",
                node_id,
                reward.value(),
                stake_weight * 100.0,
                vram_allocated_gb
            );
        }

        println!("Hourly storage reward distribution complete");
        Ok(())
    }

    /// Background task to periodically flush rewards (every 5 minutes)
    #[allow(dead_code)]
    async fn reward_flush_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
        interval.tick().await; // Skip first tick

        loop {
            interval.tick().await;
            if let Err(e) = self.flush_rewards().await {
                eprintln!("Reward flush error: {}", e);
            }
        }
    }

    /// Handle task completion with PoI proof and reward distribution
    pub async fn handle_task_completion(
        &self,
        task_id: Uuid,
        poi_proof: &PoIProof,
        _compute_time_ms: u64,
        fragment_ids: &[String],
    ) -> Result<(), SchedulerError> {
        // 1. Get task info first for validation
        let task = self.state.active_tasks.get(&task_id);
        let (node_id, task_layer_range, task_fragment_ids) = if let Some(ref t) = task {
            (t.assigned_node.clone(), t.layer_range, t.required_fragments.clone())
        } else {
            return Err(SchedulerError::ChainStateError(format!("Task {} not found", task_id)));
        };

        // 2. Validate the PoI proof - with slashing on failure
        // Note: verify_poi_proof is the standalone function from consensus
        if let Err(e) = consensus::poi::verify_poi_proof(poi_proof) {
            eprintln!("Task {} proof verification failed: {}", task_id, e);
            
            // NEW: Slash 10% of stake for inference PoI failure
            if let Some(node) = self.state.nodes.get(&node_id) {
                let slash_amount = Amount::new(
                    (node.stake.value() * 10) / 100
                );
                if let Err(e) = self.chain_state.apply_slash(&node_id, slash_amount) {
                    eprintln!("Failed to slash node {}: {}", node_id, e);
                } else {
                    eprintln!("Node {} slashed {} AIGEN for PoI failure", node_id, slash_amount.value());
                }
            }
            
            // Remove task and reassign to replica
            if let Some(task) = self.state.active_tasks.get(&task_id) {
                self.reassign_task_to_replica(&task, &node_id).await?;
            }
            return Err(SchedulerError::ConsensusError(e));
        }

        // 3. Verify distributed task specific data with cross-checks
        if let Some(distributed_proof) = poi_proof
            .verification_data
            .get("distributed_task_proof")
            .and_then(|v| serde_json::from_value::<consensus::poi::DistributedTaskProof>(v.clone()).ok())
        {
            // Cross-check: fragment_ids must match task's required_fragments
            if distributed_proof.fragment_ids.len() != task_fragment_ids.len() {
                eprintln!("Task {} fragment count mismatch: proof={}, task={}", 
                    task_id, distributed_proof.fragment_ids.len(), task_fragment_ids.len());
                return Err(SchedulerError::ConsensusError(
                    consensus::poi::ConsensusError::InvalidProof));
            }
            for (proof_frag, task_frag) in distributed_proof.fragment_ids.iter().zip(task_fragment_ids.iter()) {
                if proof_frag != task_frag {
                    eprintln!("Task {} fragment mismatch: proof={}, task={}", task_id, proof_frag, task_frag);
                    return Err(SchedulerError::ConsensusError(
                        consensus::poi::ConsensusError::InvalidProof));
                }
            }

            // Cross-check: layer_range must match task's layer_range
            if distributed_proof.layer_range.0 != task_layer_range.0 || 
               distributed_proof.layer_range.1 != task_layer_range.1 {
                eprintln!("Task {} layer_range mismatch: proof={:?}, task={:?}", 
                    task_id, distributed_proof.layer_range, task_layer_range);
                return Err(SchedulerError::ConsensusError(
                    consensus::poi::ConsensusError::InvalidProof));
            }

            // Cross-check: provided fragment_ids must match proof fragment_ids
            if fragment_ids.len() != distributed_proof.fragment_ids.len() {
                eprintln!("Task {} provided fragment_ids mismatch: provided={}, proof={}", 
                    task_id, fragment_ids.len(), distributed_proof.fragment_ids.len());
                return Err(SchedulerError::ConsensusError(
                    consensus::poi::ConsensusError::InvalidProof));
            }
            for (provided_frag, proof_frag) in fragment_ids.iter().zip(distributed_proof.fragment_ids.iter()) {
                if provided_frag != proof_frag {
                    eprintln!("Task {} provided fragment mismatch: provided={}, proof={}", 
                        task_id, provided_frag, proof_frag);
                    return Err(SchedulerError::ConsensusError(
                        consensus::poi::ConsensusError::InvalidProof));
                }
            }

            // Verify the distributed proof with expected values
            if let Err(e) = verify_distributed_task(
                &distributed_proof,
                Some(&task_fragment_ids),
                Some(task_layer_range),
                Some(poi_proof.nonce)
            ) {
                eprintln!("Task {} distributed proof verification failed: {}", task_id, e);
                return Err(SchedulerError::ConsensusError(e));
            }

            // Cross-check: compute_time_ms bounds validation
            // Min: 1ms (sanity check), Max: 1 hour (3600000ms) to prevent abuse
            const MIN_COMPUTE_TIME_MS: u64 = 1;
            const MAX_COMPUTE_TIME_MS: u64 = 3_600_000; // 1 hour
            if distributed_proof.compute_time_ms < MIN_COMPUTE_TIME_MS ||
               distributed_proof.compute_time_ms > MAX_COMPUTE_TIME_MS {
                eprintln!("Task {} compute_time_ms out of bounds: {} (expected {}-{})",
                    task_id, distributed_proof.compute_time_ms, MIN_COMPUTE_TIME_MS, MAX_COMPUTE_TIME_MS);
                return Err(SchedulerError::ConsensusError(
                    consensus::poi::ConsensusError::InvalidProof));
            }
        } else {
            eprintln!("Task {} missing distributed_task_proof in verification_data", task_id);
            return Err(SchedulerError::ConsensusError(
                consensus::poi::ConsensusError::InvalidProof));
        }

        // 4. Calculate reward
        let total_reward = calculate_poi_reward(poi_proof)
            .map_err(|e| SchedulerError::ConsensusError(e))?;

        // 5. Split reward: 80% compute, 15% storage, 5% treasury
        let compute_reward = Amount::new((total_reward.value() * 80) / 100);
        let storage_reward = Amount::new((total_reward.value() * 15) / 100);
        let treasury_reward = Amount::new((total_reward.value() * 5) / 100);

        // 6. Accumulate compute reward in pool with Compute type
        {
            let mut pool = self.reward_pool.write().await;
            let (entry_amount, entry_type) = pool.entry(node_id.clone()).or_insert((Amount::ZERO, RewardType::Compute));
            // If there's already a storage reward for this node, keep the type as Compute (they'll be batched separately)
            *entry_amount = Amount::new(entry_amount.value().saturating_add(compute_reward.value()));
            // Ensure type is Compute for compute rewards
            *entry_type = RewardType::Compute;
        }

        // 7. Distribute storage rewards to fragment hosts with Storage type
        for fragment_id in fragment_ids {
            if let Some(fragment) = self.state.fragments.get(fragment_id) {
                // Split storage reward equally among replicas (K=3)
                let per_replica_reward = Amount::new(storage_reward.value() / fragment.replicas.len().max(1) as u64);
                for replica_node_id in &fragment.replicas {
                    let mut pool = self.reward_pool.write().await;
                    let (entry_amount, entry_type) = pool.entry(replica_node_id.clone()).or_insert((Amount::ZERO, RewardType::Storage));
                    *entry_amount = Amount::new(entry_amount.value().saturating_add(per_replica_reward.value()));
                    // Storage rewards are tracked as Storage type
                    *entry_type = RewardType::Storage;
                }
            }
        }

        // 8. Treasury reward (to CEO/admin address - simplified as network reserve)
        // Note: In production this would go to a treasury address
        println!("Treasury reward allocated: {} AIGEN", treasury_reward.value());

        // 9. Update reward records in GlobalState
        self.state.record_reward(
            &node_id,
            compute_reward,
            storage_reward, // Pass actual storage reward for tracking
            true, // Is compute reward
        );

        // 10. Record reward event to history for RPC queries
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        let reward_event = crate::state::RewardHistoryEntry::new(
            node_id.clone(),
            compute_reward,
            "compute".to_string(),
            now,
            Some(task_id.to_string()),
            task.as_ref().map(|t| t.inference_id.to_string()),
            true, // proof_verified
        );
        self.state.record_reward_event(reward_event);

        // 11. Mark task as completed
        if let Some(mut task) = self.state.active_tasks.get_mut(&task_id) {
            task.status = crate::task::TaskStatus::Completed;
        }

        // 12. Increment pending reward count and check if flush needed
        {
            let mut count = self.pending_reward_count.write().await;
            *count += 1;
            if *count >= 100 {
                drop(count); // Release lock before async call
                if let Err(e) = self.flush_rewards().await {
                    eprintln!("Auto-flush failed: {}", e);
                }
            }
        }

        println!(
            "Task {} completed - Total: {}, Compute: {}, Storage: {}, Treasury: {}",
            task_id,
            total_reward.value(),
            compute_reward.value(),
            storage_reward.value(),
            treasury_reward.value()
        );

        Ok(())
    }

    /// Flush accumulated rewards to chain state using RewardTx
    pub async fn flush_rewards(&self) -> Result<(), SchedulerError> {
        let rewards_to_flush: HashMap<String, (Amount, RewardType)> = {
            let mut pool = self.reward_pool.write().await;
            let drained: HashMap<String, (Amount, RewardType)> = pool.drain().collect();
            drained
        };

        if rewards_to_flush.is_empty() {
            return Ok(());
        }

        // Check if CEO key is available for signing
        let ceo_key = match &self.ceo_secret_key {
            Some(key) => key,
            None => {
                // Re-add all rewards back to pool since we can't sign
                let mut pool = self.reward_pool.write().await;
                for (node_id, (amount, reward_type)) in rewards_to_flush {
                    pool.insert(node_id, (amount, reward_type));
                }
                return Err(SchedulerError::ChainStateError(
                    "CEO key not available for signing reward transactions".to_string()
                ));
            }
        };

        let mut minted_count = 0u64;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        for (node_id, (amount, reward_type)) in rewards_to_flush {
            if amount.value() > 0 {
                // Create a RewardTx for this reward with appropriate type
                let reward_tx = RewardTx::new(
                    node_id.clone(),
                    amount.value(),
                    reward_type.clone(), // Use the tracked reward type (Compute or Storage)
                    None, // No specific task_id for batched rewards
                    timestamp,
                ).map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

                // Sign the reward transaction with CEO key
                let message = reward_tx.message_to_sign();
                let sig = sign_message(&message, ceo_key);
                let ceo_sig = CeoSignature(sig);
                let signed_reward_tx = reward_tx.sign_with_ceo(ceo_sig);

                // Apply the reward transaction through the chain state
                if let Err(e) = self.chain_state.apply_reward_transaction(&signed_reward_tx) {
                    eprintln!("Failed to apply reward tx for node {}: {}", node_id, e);
                    // Re-add failed reward back to pool
                    let mut pool = self.reward_pool.write().await;
                    pool.insert(node_id, (amount, reward_type));
                } else {
                    minted_count += 1;
                    println!("Applied reward tx: {} AIGEN for node {} (type: {:?})",
                        amount.value(), node_id, reward_type);
                }
            }
        }

        // Reset pending count
        {
            let mut count = self.pending_reward_count.write().await;
            *count = 0;
        }

        println!("Reward flush complete: {} reward transactions applied", minted_count);
        Ok(())
    }

    /// Get pending reward amount for a node (for RPC queries)
    pub async fn get_pending_reward(&self, node_id: &str) -> Amount {
        let pool = self.reward_pool.read().await;
        pool.get(node_id).map(|(amount, _)| *amount).unwrap_or(Amount::ZERO)
    }

    /// Federated learning training loop - monitors buffer and triggers training bursts
    pub async fn start_training_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30)); // Check every 30 seconds
        interval.tick().await; // Skip first tick

        loop {
            interval.tick().await;

            // Skip if training is disabled
            if !*self.training_enabled.read().await {
                continue;
            }

            // Check if buffer is ready for training via CL manager if available
            let buffer_ready = if let Some(cl_manager) = self.cl_manager.as_ref() {
                // Use CL manager's buffer readiness check
                cl_manager.is_buffer_ready("default-model")
            } else {
                // Fall back to training coordinator's buffer check
                let config = self.training_coordinator.config().clone();
                self.training_coordinator.buffer_size().await >= config.trigger_threshold
            };

            if buffer_ready {
                if let Err(e) = self.execute_training_burst().await {
                    eprintln!("Training burst failed: {}", e);
                }
            }
        }
    }

    /// Execute a training burst when buffer reaches threshold
    pub async fn execute_training_burst(&self) -> Result<(), SchedulerError> {
        // Check if model weights are initialized
        if !self.training_coordinator.are_weights_initialized().await {
            // Initialize with dummy weights for testing
            // In production, this would load from model registry
            let dummy_weights = vec![0.0f32; 1000]; // Initialize with 1000 parameters
            self.training_coordinator.initialize_model_weights(dummy_weights).await
                .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;
            tracing::info!("Initialized model weights for training");
        }

        let config = self.training_coordinator.config().clone();

        // Sample batch from buffer (via CL manager if available)
        let samples = self.training_coordinator.sample_from_buffer(config.batch_size).await;

        if samples.is_empty() {
            return Ok(());
        }

        // Get model ID (in real implementation, would get from registry)
        let model_id = "default-model".to_string();

        // Trigger training burst
        let round_id = self.training_coordinator.trigger_training_burst(model_id.clone()).await
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        println!("Training burst triggered for round {}, model {}", round_id, model_id);

        // Execute local training (uses CL manager for EWC if available)
        let delta = self.training_coordinator.execute_local_training(&samples, &model_id).await
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        // Compute and store Fisher matrix using CL manager
        let fisher_hash = if self.cl_manager.is_some() {
            match self.training_coordinator.compute_and_store_fisher(&model_id).await {
                Ok(hash) => {
                    tracing::info!("Computed and stored Fisher matrix with hash {} for model {}",
                        hex::encode(&hash), model_id);
                    hash
                }
                Err(e) => {
                    tracing::warn!("Failed to compute and store Fisher matrix: {}", e);
                    [0u8; 32]
                }
            }
        } else {
            [0u8; 32]
        };

        // Persist replay buffer to IPFS and store metadata on-chain
        let replay_buffer_hash = if self.cl_manager.is_some() {
            match self.training_coordinator.persist_replay_buffer(&model_id).await {
                Ok((hash, cid)) => {
                    tracing::info!("Persisted replay buffer for model {}: hash={}, cid={}",
                        model_id, hex::encode(&hash), cid);
                    hash
                }
                Err(e) => {
                    tracing::warn!("Failed to persist replay buffer: {}", e);
                    [0u8; 32]
                }
            }
        } else {
            [0u8; 32]
        };

        // Distribute delta shares to peers
        self.training_coordinator.distribute_delta_shares(delta, round_id).await
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        // Note: In a full implementation, the following would happen:
        // 1. Wait for aggregated shares from all peers
        // 2. Reconstruct global delta
        // 3. Apply global delta to model
        // 4. Generate training PoI proof
        // 5. Broadcast training proof
        // For now, this is handled by network message handlers

        // Record training completion with Fisher and replay buffer hashes
        // This would typically be called after receiving all aggregated shares
        // For now, we record immediately with the computed hashes
        let participants = vec![self.local_validator_address.clone()];
        
        // Record training round with hashes
        let training_round = blockchain_core::state::TrainingRound {
            round_id: round_id.to_string(),
            participants: participants.clone(),
            delta_hash: [0u8; 32], // Would be computed from actual delta
            fisher_hash,
            replay_buffer_hash,
            timestamp: chrono::Utc::now().timestamp(),
            model_id: model_id.clone(),
            sample_count: samples.len() as u64,
            learning_rate: 1, // 1e-6 scaled
            ewc_lambda: 40,   // 0.4 scaled by 100
            status: 1,        // Completed
        };

        self.chain_state.record_training_round(training_round)
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        tracing::info!("Training burst completed for round {}: fisher_hash={}, replay_buffer_hash={}",
            round_id, hex::encode(&fisher_hash), hex::encode(&replay_buffer_hash));

        Ok(())
    }

    /// Handle post-inference training trigger
    /// 
    /// Records inference sample for continual learning and triggers training when buffer is ready.
    /// Delegates to ContinualLearningManager when available for unified buffer management.
    pub async fn handle_training_after_inference(
        &self,
        task_input: Vec<f32>,
        task_output: Vec<f32>,
        task_loss: f32,
    ) -> Result<(), SchedulerError> {
        if !*self.training_enabled.read().await {
            return Ok(());
        }

        // Use CL manager for unified buffer management when available
        if let Some(cl_manager) = self.cl_manager.as_ref() {
            // Record inference directly in CL manager's buffer
            let model_id = "default-model";
            if let Err(e) = cl_manager.record_inference(model_id, task_input, task_output, task_loss).await {
                tracing::warn!("Failed to record inference in CL manager: {}", e);
            } else {
                tracing::debug!("Recorded inference for model {} in CL buffer", model_id);
                
                // Check if buffer is ready via CL manager
                if cl_manager.is_buffer_ready(model_id) {
                    tracing::info!("Training buffer ready for model {} - burst will be triggered by training loop", model_id);
                }
            }
            return Ok(());
        }

        // Legacy path: Use TrainingCoordinator for buffer management
        let should_trigger = self.training_coordinator
            .on_inference_complete(task_input.clone(), task_output.clone(), task_loss)
            .await
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        if should_trigger {
            tracing::info!("Training buffer ready (legacy path) - burst will be triggered by training loop");
        }

        Ok(())
    }

    /// Handle training completion and distribute rewards
    pub async fn handle_training_completion(
        &self,
        round_id: uuid::Uuid,
        participants: Vec<String>,
        reward_per_node: Amount,
    ) -> Result<(), SchedulerError> {
        // Distribute training rewards
        for participant in &participants {
            let mut pool = self.reward_pool.write().await;
            let (entry_amount, _) = pool.entry(participant.clone())
                .or_insert((Amount::ZERO, RewardType::Training));
            *entry_amount = Amount::new(entry_amount.value().saturating_add(reward_per_node.value()));
        }

        // Record training round on-chain with all required fields
        let training_round = blockchain_core::state::TrainingRound {
            round_id: round_id.to_string(),
            participants,
            delta_hash: [0u8; 32], // Would be computed from actual delta
            fisher_hash: [0u8; 32], // Would be computed from Fisher matrix
            replay_buffer_hash: [0u8; 32], // Would be computed from replay buffer
            timestamp: chrono::Utc::now().timestamp(),
            model_id: "default".to_string(),
            sample_count: 128,
            learning_rate: 1, // 1e-6 scaled
            ewc_lambda: 40,   // 0.4 scaled by 100
            status: 1,        // Completed
        };

        self.chain_state.record_training_round(training_round)
            .map_err(|e| SchedulerError::ChainStateError(e.to_string()))?;

        Ok(())
    }
}
