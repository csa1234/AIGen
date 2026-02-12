use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use libp2p::PeerId;
use crate::state::GlobalState;
use crate::task::TaskPlan;
use crate::scheduler::SchedulerError;

/// Reputation score for a peer, used in route selection
#[derive(Debug, Clone, Copy)]
pub struct PeerReputationInfo {
    pub score: f64,
    pub is_banned: bool,
}

/// Manager for peer reputation data used in routing decisions
#[derive(Debug, Default)]
pub struct RoutingReputationManager {
    reputations: HashMap<PeerId, PeerReputationInfo>,
}

impl RoutingReputationManager {
    pub fn new() -> Self {
        Self {
            reputations: HashMap::new(),
        }
    }

    pub fn update_reputation(&mut self, peer_id: PeerId, score: f64, is_banned: bool) {
        self.reputations.insert(peer_id, PeerReputationInfo { score, is_banned });
    }

    pub fn get_reputation(&self, peer_id: &PeerId) -> Option<PeerReputationInfo> {
        self.reputations.get(peer_id).copied()
    }

    pub fn is_banned(&self, peer_id: &PeerId) -> bool {
        self.reputations
            .get(peer_id)
            .map(|r| r.is_banned)
            .unwrap_or(false)
    }

    pub fn get_score(&self, peer_id: &PeerId) -> f64 {
        self.reputations
            .get(peer_id)
            .map(|r| r.score)
            .unwrap_or(0.5) // Default neutral score
    }
}

pub struct RouteSelector {
    state: Arc<GlobalState>,
    reputation_manager: RoutingReputationManager,
}

impl RouteSelector {
    pub fn new(state: Arc<GlobalState>) -> Self {
        Self {
            state,
            reputation_manager: RoutingReputationManager::new(),
        }
    }

    pub fn with_reputation_manager(state: Arc<GlobalState>, reputation_manager: RoutingReputationManager) -> Self {
        Self {
            state,
            reputation_manager,
        }
    }

    /// Update RTT for a node pair in GlobalState
    pub fn update_rtt(&self, from_node: &str, to_node: &str, rtt: Duration) {
        if let Some(mut node) = self.state.nodes.get_mut(from_node) {
            node.rtt_map.insert(to_node.to_string(), rtt);
        }
    }

    /// Update reputation for a peer
    pub fn update_peer_reputation(&mut self, peer_id: PeerId, score: f64, is_banned: bool) {
        self.reputation_manager.update_reputation(peer_id, score, is_banned);
    }

    pub fn select_route(&self, task_plan: &mut TaskPlan) -> Result<(), SchedulerError> {
        let mut prev_node_id = String::new(); // Ideally the source of the request
        let mut prev_task_id: Option<uuid::Uuid> = None;

        for task in &mut task_plan.tasks {
            if task.required_fragments.is_empty() {
                continue;
            }
            let frag_id = &task.required_fragments[0];
            let candidates = self.state.get_nodes_with_fragment(frag_id)?;
            
            let frag_loc = self.state.fragments.get(frag_id)
                .ok_or(SchedulerError::FragmentNotFound(frag_id.clone()))?;
            let fragment_size_gb = frag_loc.size_bytes as f32 / 1_000_000_000.0;

            let best_node = candidates.iter()
                .filter(|node| node.vram_free_gb >= fragment_size_gb)
                .filter(|node| {
                    // Filter out banned peers
                    if let Some(peer_id) = node.peer_id {
                        !self.reputation_manager.is_banned(&peer_id)
                    } else {
                        true // No peer_id means no reputation data, allow it
                    }
                })
                .min_by(|a, b| {
                    // Calculate latency score incorporating RTT and reputation penalties
                    let score_a = self.calculate_latency_score(a, &prev_node_id);
                    let score_b = self.calculate_latency_score(b, &prev_node_id);
                    score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or(SchedulerError::NoAvailableNode(frag_id.clone()))?;
            
            task.assigned_node = best_node.node_id.clone();
            
            // Link activations between pipeline nodes
            if let Some(prev_id) = prev_task_id {
                // This task receives activation from previous task
                task.input_activation_ref = Some(crate::task::ActivationRef {
                    location: format!("{}/{}", prev_node_id, prev_id),
                    size_bytes: 0, // Will be populated after first task runs
                });
            }
            
            // Store task in active_tasks
            self.state.active_tasks.insert(task.task_id, task.clone());
            
            prev_node_id = best_node.node_id.clone();
            prev_task_id = Some(task.task_id);
            
            // Allocate task update load
            self.state.allocate_task(task, &best_node.node_id);
        }
        
        // Optimize pipeline order (approximate TSP for small graphs)
        self.optimize_pipeline_order(task_plan);
        Ok(())
    }

    /// Calculate a latency score for a node, incorporating RTT and reputation penalties
    fn calculate_latency_score(&self, node: &crate::state::NodeState, prev_node_id: &str) -> Duration {
        // Base RTT from the node's rtt_map or use default
        let rtt = if prev_node_id.is_empty() {
            Duration::from_millis(0)
        } else {
            *node.rtt_map.get(prev_node_id).unwrap_or(&Duration::from_millis(10))
        };
        
        // Get reputation score (default 0.5 if no data)
        let reputation_score = if let Some(peer_id) = node.peer_id {
            self.reputation_manager.get_score(&peer_id)
        } else {
            0.5
        };
        
        // Queue time based on load score
        let queue_time = Duration::from_millis((node.load_score * 100.0) as u64);
        
        // Compute time estimate per fragment
        let compute_time = Duration::from_millis(5);
        
        // Reputation penalty: lower reputation = higher penalty (up to 100ms additional)
        let reputation_penalty = if reputation_score < 0.3 {
            Duration::from_millis(((0.3 - reputation_score) * 500.0) as u64)
        } else {
            Duration::ZERO
        };
        
        rtt + queue_time + compute_time + reputation_penalty
    }

    fn optimize_pipeline_order(&self, task_plan: &mut TaskPlan) {
        task_plan.pipeline_order = task_plan.tasks.iter().map(|t| t.assigned_node.clone()).collect();
    }
}
