use distributed_compute::routing::RouteSelector;
use distributed_compute::state::{GlobalState, FragmentLocation, NodeState, NodeRole};
use distributed_compute::task::{TaskPlan, ComputeTask, TaskStatus};
use std::sync::Arc;
use network::protocol::NodeCapabilities;
use blockchain_core::types::{Amount, Timestamp};
use libp2p::PeerId;
use uuid::Uuid;

#[test]
fn test_routing_selection() {
    let state = Arc::new(GlobalState::new());
    let router = RouteSelector::new(state.clone());
    
    // Setup state
    let node_id = "node1".to_string();
    let frag_id = "frag1".to_string();
    
    state.fragments.insert(frag_id.clone(), FragmentLocation {
        fragment_id: frag_id.clone(),
        model_id: "model1".to_string(),
        fragment_index: 0,
        size_bytes: 1024 * 1024 * 1024, // 1GB
        replicas: vec![node_id.clone()],
    });
    
    state.nodes.insert(node_id.clone(), NodeState {
        node_id: node_id.clone(),
        peer_id: Some(PeerId::random()),
        role: NodeRole::Inference,
        vram_free_gb: 2.0,
        vram_total_gb: 24.0,
        vram_allocated_gb: 22.0,
        region: None,
        last_heartbeat: Timestamp(0),
        load_score: 0.0,
        stake: Amount::ZERO,
        capabilities: NodeCapabilities {
            has_gpu: true,
            gpu_model: None,
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 2048,
        },
        rtt_map: Default::default(),
    });
    
    let mut plan = TaskPlan {
        inference_id: Uuid::new_v4(),
        tasks: vec![ComputeTask {
            task_id: Uuid::new_v4(),
            inference_id: Uuid::new_v4(),
            model_id: "model1".to_string(),
            layer_range: (0, 10),
            required_fragments: vec![frag_id],
            input_activation_ref: None,
            assigned_node: String::new(),
            status: TaskStatus::Pending,
        }],
        pipeline_order: vec![],
    };
    
    router.select_route(&mut plan).unwrap();
    assert_eq!(plan.tasks[0].assigned_node, node_id);
}
