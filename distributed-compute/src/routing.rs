use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
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

/// Cache key for route decisions: (task_id, fragment_id)
pub type RouteCacheKey = (uuid::Uuid, String);

/// Cache entry with node_id, score, and timestamp
pub struct RouteCacheEntry {
    pub node_id: String,
    pub score: f64,
    pub timestamp: Instant,
}

/// TTL for cache entries in seconds
const CACHE_TTL_SECONDS: u64 = 60;

pub struct RouteSelector {
    state: Arc<GlobalState>,
    reputation_manager: RoutingReputationManager,
    /// LRU cache for route decisions with TTL
    route_cache: Arc<DashMap<RouteCacheKey, RouteCacheEntry>>,
}

impl RouteSelector {
    pub fn new(state: Arc<GlobalState>) -> Self {
        let selector = Self {
            state,
            reputation_manager: RoutingReputationManager::new(),
            route_cache: Arc::new(DashMap::new()),
        };
        selector.spawn_cache_cleanup_task();
        selector
    }

    pub fn with_reputation_manager(state: Arc<GlobalState>, reputation_manager: RoutingReputationManager) -> Self {
        let selector = Self {
            state,
            reputation_manager,
            route_cache: Arc::new(DashMap::new()),
        };
        selector.spawn_cache_cleanup_task();
        selector
    }

    /// Spawn background task to clean expired cache entries
    fn spawn_cache_cleanup_task(&self) {
        let cache = self.route_cache.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(CACHE_TTL_SECONDS));
            loop {
                interval.tick().await;
                let now = Instant::now();
                let ttl = Duration::from_secs(CACHE_TTL_SECONDS);
                cache.retain(|_, entry| now.duration_since(entry.timestamp) < ttl);
            }
        });
    }

    /// Clear the route cache (useful for testing or manual invalidation)
    pub fn clear_cache(&self) {
        self.route_cache.clear();
    }

    /// Get cache statistics for monitoring
    pub fn cache_stats(&self) -> (usize, usize) {
        let total_entries = self.route_cache.len();
        let now = Instant::now();
        let ttl = Duration::from_secs(CACHE_TTL_SECONDS);
        let valid_entries = self.route_cache
            .iter()
            .filter(|entry| now.duration_since(entry.timestamp) < ttl)
            .count();
        (valid_entries, total_entries)
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
            
            // Check cache first
            let cache_key = (task.task_id, frag_id.clone());
            let cached_result = self.route_cache.get(&cache_key);
            let now = Instant::now();
            let ttl = Duration::from_secs(CACHE_TTL_SECONDS);
            
            // Check if cache entry is valid and node still exists
            if let Some(entry) = cached_result {
                if now.duration_since(entry.timestamp) < ttl {
                    // Verify node still exists and has capacity
                    if let Some(node) = self.state.nodes.get(&entry.node_id) {
                        let frag_loc = self.state.fragments.get(frag_id)
                            .ok_or(SchedulerError::FragmentNotFound(frag_id.clone()))?;
                        let fragment_size_gb = frag_loc.size_bytes as f32 / 1_000_000_000.0;
                        
                        // Check if node still has capacity and wasn't rejected
                        if node.vram_free_gb >= fragment_size_gb 
                            && !task.rejected_nodes.contains(&node.node_id)
                            && node.peer_id.map_or(true, |pid| !self.reputation_manager.is_banned(&pid)) {
                            // Use cached route decision
                            task.assigned_node = entry.node_id.clone();
                            
                            // Link activations between pipeline nodes
                            if let Some(prev_id) = prev_task_id {
                                task.input_activation_ref = Some(crate::task::ActivationRef {
                                    location: format!("{}/{}", prev_node_id, prev_id),
                                    size_bytes: 0,
                                });
                            }
                            
                            self.state.active_tasks.insert(task.task_id, task.clone());
                            prev_node_id = entry.node_id.clone();
                            prev_task_id = Some(task.task_id);
                            self.state.allocate_task(task, &entry.node_id);
                            continue; // Move to next task, cache hit
                        }
                    }
                }
                // Cache expired or invalid, remove it
                drop(entry);
                self.route_cache.remove(&cache_key);
            }
            
            // Cache miss - compute route
            let candidates = self.state.get_nodes_with_fragment(frag_id)?;
            
            // NEW: Filter out nodes rejected due to insufficient stake
            let candidates: Vec<crate::state::NodeState> = candidates.into_iter()
                .filter(|n| !task.rejected_nodes.contains(&n.node_id))
                .collect();
            
            if candidates.is_empty() {
                return Err(SchedulerError::NoAvailableNode(frag_id.clone()));
            }
            
            let frag_loc = self.state.fragments.get(frag_id)
                .ok_or(SchedulerError::FragmentNotFound(frag_id.clone()))?;
            let fragment_size_gb = frag_loc.size_bytes as f32 / 1_000_000_000.0;

            let (best_node, best_score) = candidates.iter()
                .filter(|node| node.vram_free_gb >= fragment_size_gb)
                .filter(|node| {
                    if let Some(peer_id) = node.peer_id {
                        !self.reputation_manager.is_banned(&peer_id)
                    } else {
                        true
                    }
                })
                .map(|node| {
                    let score = self.calculate_node_score(node, &prev_node_id, task);
                    (node, score)
                })
                .min_by(|(_, score_a), (_, score_b)| {
                    score_a.partial_cmp(score_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or(SchedulerError::NoAvailableNode(frag_id.clone()))?;
            
            task.assigned_node = best_node.node_id.clone();
            
            // Update cache with new decision
            self.route_cache.insert(
                cache_key,
                RouteCacheEntry {
                    node_id: best_node.node_id.clone(),
                    score: best_score,
                    timestamp: Instant::now(),
                }
            );
            
            // Link activations between pipeline nodes
            if let Some(prev_id) = prev_task_id {
                task.input_activation_ref = Some(crate::task::ActivationRef {
                    location: format!("{}/{}", prev_node_id, prev_id),
                    size_bytes: 0,
                });
            }
            
            self.state.active_tasks.insert(task.task_id, task.clone());
            
            prev_node_id = best_node.node_id.clone();
            prev_task_id = Some(task.task_id);
            
            self.state.allocate_task(task, &best_node.node_id);
        }
        
        // Optimize pipeline order
        self.optimize_pipeline_order(task_plan);
        Ok(())
    }

    /// Calculate a node score for routing, incorporating latency, reputation, and bid price
    fn calculate_node_score(
        &self,
        node: &crate::state::NodeState,
        prev_node_id: &str,
        task: &crate::task::ComputeTask,
    ) -> f64 {
        // Get latency score
        let latency = self.calculate_latency_score(node, prev_node_id);
        let latency_ms = latency.as_millis() as f64;

        // Get reputation score (default 0.5 if no data)
        let reputation_score = if let Some(peer_id) = node.peer_id {
            self.reputation_manager.get_score(&peer_id)
        } else {
            0.5
        };

        // NEW: Consider bid price (lower price = higher score)
        let bid_score = if let Some(bid_price) = node.bid_price_per_task {
            // Normalize bid price: assume max reasonable bid is 1000 AIGEN
            let max_bid = 1000.0;
            let bid_value = bid_price.value() as f64;
            1.0 - (bid_value / max_bid).min(1.0)
        } else {
            0.5 // Default score if no bid set
        };

        // Combined score: lower latency is better, higher reputation is better, lower bid is better
        // Formula: score = (1/latency) * reputation * (1 + bid_score)
        let latency_component = 1000.0 / (latency_ms + 1.0); // +1 to avoid div by zero
        let score = latency_component * reputation_score * (1.0 + bid_score) / 2.0;

        score
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
