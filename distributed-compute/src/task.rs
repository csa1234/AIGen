use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use model::registry::ModelRegistry;
use crate::state::GlobalState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationRef {
    pub location: String, // e.g., IPFS CID or temporary storage ID
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceTask {
    pub inference_id: Uuid,
    pub model_id: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub model_id: String,
    pub layer_range: (u32, u32),
    pub required_fragments: Vec<String>, // fragment_ids
    pub input_activation_ref: Option<ActivationRef>,
    pub assigned_node: String,
    pub status: TaskStatus, // Pending | Running | Completed | Failed
    // Tensor parallelism fields
    pub tensor_shard_index: u32,
    pub total_tensor_shards: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub inference_id: Uuid,
    pub tasks: Vec<ComputeTask>,
    pub pipeline_order: Vec<String>, // node_ids in execution order
}

pub fn decompose_inference(
    request: &InferenceTask,
    state: &Arc<GlobalState>,
    registry: &Arc<ModelRegistry>
) -> Result<TaskPlan, crate::scheduler::SchedulerError> {
    let manifest = registry.get_fragment_manifest(&request.model_id)
        .map_err(|_| crate::scheduler::SchedulerError::ModelNotFound(request.model_id.clone()))?;
    
    let mut tasks = Vec::new();
    
    for (index, fragment) in manifest.fragments.iter().enumerate() {
        let layer_range = fragment.layer_range.unwrap_or((0, 0));
        let task = ComputeTask {
            task_id: Uuid::new_v4(),
            inference_id: request.inference_id,
            model_id: request.model_id.clone(),
            layer_range,
            required_fragments: vec![fragment.fragment_id.clone()],
            input_activation_ref: None, // To be linked
            assigned_node: String::new(),
            status: TaskStatus::Pending,
            tensor_shard_index: index as u32,
            total_tensor_shards: manifest.fragments.len() as u32,
        };
        tasks.push(task);
        
        // Populate FragmentLocation record in GlobalState
        let replicas: Vec<String> = fragment.locations
            .iter()
            .filter(|loc| loc.is_healthy)
            .map(|loc| loc.node_id.clone())
            .collect();
        
        let fragment_location = crate::state::FragmentLocation {
            fragment_id: fragment.fragment_id.clone(),
            model_id: fragment.model_id.clone(),
            fragment_index: fragment.fragment_index,
            size_bytes: fragment.size_bytes,
            replicas,
        };
        state.fragments.insert(fragment.fragment_id.clone(), fragment_location);
    }
    
    // Sort tasks by layer range start
    tasks.sort_by_key(|t| t.layer_range.0);

    Ok(TaskPlan {
        inference_id: request.inference_id,
        tasks,
        pipeline_order: Vec::new(),
    })
}
