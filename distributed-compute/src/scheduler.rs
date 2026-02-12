use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use thiserror::Error;
use consensus::validator::{ValidatorRegistry, select_validators_weighted};
use network::protocol::NetworkMessage;
use model::registry::ModelRegistry;
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
    local_node_id: String,
    local_validator_address: String,
    registry: Arc<ModelRegistry>,
}

impl DynamicScheduler {
    pub fn new(
        state: Arc<GlobalState>,
        router: RouteSelector,
        validator_registry: Arc<ValidatorRegistry>,
        network_tx: mpsc::Sender<NetworkMessage>,
        local_node_id: String,
        local_validator_address: String,
        registry: Arc<ModelRegistry>,
    ) -> Self {
        Self {
            state,
            router,
            is_leader: Arc::new(RwLock::new(false)),
            validator_registry,
            network_tx,
            local_node_id,
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
            };
            self.network_tx.send(msg).await?;
        }
        
        Ok(plan)
    }
}
