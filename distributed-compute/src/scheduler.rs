use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use thiserror::Error;
use consensus::validator::{ValidatorRegistry, select_validators_weighted};
use network::protocol::NetworkMessage;
use model::registry::ModelRegistry;
use uuid::Uuid;
use crate::state::GlobalState;
use crate::routing::RouteSelector;
use crate::task::{InferenceTask, TaskPlan, decompose_inference};

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
}

pub struct DynamicScheduler {
    state: Arc<GlobalState>,
    router: RouteSelector,
    is_leader: Arc<RwLock<bool>>,
    validator_registry: Arc<ValidatorRegistry>,
    network_tx: mpsc::Sender<NetworkMessage>,
    local_validator_address: String,
    registry: Arc<ModelRegistry>,
}

impl DynamicScheduler {
    pub fn new(
        state: Arc<GlobalState>,
        router: RouteSelector,
        validator_registry: Arc<ValidatorRegistry>,
        network_tx: mpsc::Sender<NetworkMessage>,
        local_validator_address: String,
        registry: Arc<ModelRegistry>,
    ) -> Self {
        Self {
            state,
            router,
            is_leader: Arc::new(RwLock::new(false)),
            validator_registry,
            network_tx,
            local_validator_address,
            registry,
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
        
        // 2. Select routes
        self.router.select_route(&mut plan)?;
        
        // 3. Send ComputeTask messages to assigned nodes
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
    
    /// Detect nodes that missed 3+ heartbeats
    async fn detect_failed_nodes(&self) -> Vec<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let threshold = Duration::from_millis(1500); // 3 x 500ms
        
        self.state.nodes
            .iter()
            .filter(|entry| {
                let node = entry.value();
                let elapsed = now.saturating_sub(node.last_heartbeat.0);
                elapsed > threshold.as_secs() as i64
            })
            .map(|entry| entry.key().clone())
            .collect()
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
    }
}
