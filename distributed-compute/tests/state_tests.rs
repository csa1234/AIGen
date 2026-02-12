use distributed_compute::state::{GlobalState, NodeState, NodeRole};
use network::protocol::NodeCapabilities;
use libp2p::PeerId;
use blockchain_core::types::{Amount, Timestamp};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_node_state_update() {
    let state = GlobalState::new();
    let node_id = "node1".to_string();
    let peer_id = PeerId::random();
    
    let node = NodeState {
        node_id: node_id.clone(),
        peer_id: Some(peer_id),
        role: NodeRole::Inference,
        vram_total_gb: 24.0,
        vram_free_gb: 20.0,
        vram_allocated_gb: 4.0,
        region: None,
        last_heartbeat: Timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64),
        load_score: 0.1,
        stake: Amount::new(1000),
        capabilities: NodeCapabilities {
            has_gpu: true,
            gpu_model: Some("RTX 4090".to_string()),
            supports_inference: true,
            supports_training: false,
            max_fragment_size_mb: 1024,
        },
        rtt_map: Default::default(),
    };
    
    state.update_node_state(node.clone());
    
    let retrieved = state.nodes.get(&node_id).unwrap();
    assert_eq!(retrieved.vram_free_gb, 20.0);
    
    state.update_node_from_announcement(
        node_id.clone(),
        Some(peer_id),
        24.0,  // vram_total_gb
        15.0,  // vram_free_gb
        9.0,   // vram_allocated_gb
        node.capabilities.clone(),
        Timestamp(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64),
    );
    let updated = state.nodes.get(&node_id).unwrap();
    assert_eq!(updated.vram_free_gb, 15.0);
}
