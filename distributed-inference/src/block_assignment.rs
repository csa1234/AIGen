// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// LayerBlock represents a contiguous range of model layers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerBlock {
    pub block_id: u32,
    pub layer_range: (u32, u32), // (start_layer, end_layer)
    pub model_id: String,
    pub estimated_vram_gb: f32,
}

/// BlockReplica tracks a specific node hosting a block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockReplica {
    pub block_id: u32,
    pub node_id: String,
    pub is_healthy: bool,
    pub last_heartbeat: i64,
    pub load_score: f32,
}

impl BlockReplica {
    pub fn new(block_id: u32, node_id: String) -> Self {
        Self {
            block_id,
            node_id,
            is_healthy: true,
            last_heartbeat: chrono::Utc::now().timestamp(),
            load_score: 0.0,
        }
    }
}

/// BlockAssignment manages the mapping of blocks to replicas
#[derive(Clone, Debug)]
pub struct BlockAssignment {
    pub blocks: Vec<LayerBlock>,
    pub replicas: HashMap<u32, Vec<BlockReplica>>, // block_id -> replicas (K=3)
    pub replication_factor: usize,                   // Default K=3
}

impl BlockAssignment {
    /// Initialize with K replicas per block
    pub fn new(replication_factor: usize) -> Self {
        Self {
            blocks: Vec::new(),
            replicas: HashMap::new(),
            replication_factor,
        }
    }

    /// Register a new layer block
    pub fn add_block(&mut self, block: LayerBlock) {
        let block_id = block.block_id;
        self.blocks.push(block);
        self.replicas.entry(block_id).or_insert_with(Vec::new);
    }

    /// Assign node to block
    pub fn assign_replica(&mut self, block_id: u32, node_id: String) {
        let replica = BlockReplica::new(block_id, node_id);
        self.replicas
            .entry(block_id)
            .or_insert_with(Vec::new)
            .push(replica);
    }

    /// Filter by health status
    pub fn get_healthy_replicas(&self, block_id: u32) -> Vec<BlockReplica> {
        self.replicas
            .get(&block_id)
            .map(|replicas| {
                replicas
                    .iter()
                    .filter(|r| r.is_healthy)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update health on failure
    pub fn mark_replica_unhealthy(&mut self, block_id: u32, node_id: &str) {
        if let Some(replicas) = self.replicas.get_mut(&block_id) {
            for replica in replicas.iter_mut() {
                if replica.node_id == node_id {
                    replica.is_healthy = false;
                    break;
                }
            }
        }
    }

    /// Mark replica as healthy (e.g., after recovery)
    pub fn mark_replica_healthy(&mut self, block_id: u32, node_id: &str) {
        if let Some(replicas) = self.replicas.get_mut(&block_id) {
            for replica in replicas.iter_mut() {
                if replica.node_id == node_id {
                    replica.is_healthy = true;
                    replica.last_heartbeat = chrono::Utc::now().timestamp();
                    break;
                }
            }
        }
    }

    /// Update replica load score
    pub fn update_replica_load(&mut self, block_id: u32, node_id: &str, load_score: f32) {
        if let Some(replicas) = self.replicas.get_mut(&block_id) {
            for replica in replicas.iter_mut() {
                if replica.node_id == node_id {
                    replica.load_score = load_score;
                    replica.last_heartbeat = chrono::Utc::now().timestamp();
                    break;
                }
            }
        }
    }

    /// Choose replica with lowest load
    pub fn select_best_replica(&self, block_id: u32) -> Option<String> {
        let healthy = self.get_healthy_replicas(block_id);
        if healthy.is_empty() {
            return None;
        }

        // Select replica with lowest load score
        healthy
            .into_iter()
            .min_by(|a, b| a.load_score.partial_cmp(&b.load_score).unwrap())
            .map(|r| r.node_id)
    }

    /// Get all block IDs
    pub fn get_block_ids(&self) -> Vec<u32> {
        self.blocks.iter().map(|b| b.block_id).collect()
    }

    /// Get block by ID
    pub fn get_block(&self, block_id: u32) -> Option<&LayerBlock> {
        self.blocks.iter().find(|b| b.block_id == block_id)
    }

    /// Get all replicas for a block
    pub fn get_replicas(&self, block_id: u32) -> Vec<BlockReplica> {
        self.replicas.get(&block_id).cloned().unwrap_or_default()
    }

    /// Check if block has minimum required replicas
    pub fn has_minimum_replicas(&self, block_id: u32) -> bool {
        let healthy_count = self.get_healthy_replicas(block_id).len();
        healthy_count >= self.replication_factor
    }

    /// Get total number of healthy replicas across all blocks
    pub fn total_healthy_replicas(&self) -> usize {
        self.replicas
            .values()
            .flat_map(|replicas| replicas.iter().filter(|r| r.is_healthy))
            .count()
    }
}

/// Error type for block assignment operations
#[derive(Debug, Error)]
pub enum AssignmentError {
    #[error("block not found: {0}")]
    BlockNotFound(u32),
    #[error("replica not found: block={block_id}, node={node_id}")]
    ReplicaNotFound { block_id: u32, node_id: String },
    #[error("config loading failed: {0}")]
    ConfigLoad(String),
    #[error("config saving failed: {0}")]
    ConfigSave(String),
}

/// Static configuration for a replica
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplicaConfig {
    pub node_id: String,
    pub priority: u32,
}

/// Static configuration for a block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockConfig {
    pub block_id: u32,
    pub layer_range: (u32, u32),
    pub model_id: String,
    pub estimated_vram_gb: f32,
    pub replicas: Vec<ReplicaConfig>,
}

/// StaticBlockConfig for initial deployment (Phase 1)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticBlockConfig {
    pub total_blocks: u32,
    pub replication_factor: usize,
    pub blocks: Vec<BlockConfig>,
}

impl StaticBlockConfig {
    /// Load configuration from file
    pub fn from_file(path: &Path) -> Result<Self, AssignmentError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| AssignmentError::ConfigLoad(e.to_string()))?;
        let config: StaticBlockConfig =
            toml::from_str(&content).map_err(|e| AssignmentError::ConfigLoad(e.to_string()))?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file(&self, path: &Path) -> Result<(), AssignmentError> {
        let content =
            toml::to_string_pretty(self).map_err(|e| AssignmentError::ConfigSave(e.to_string()))?;
        std::fs::write(path, content).map_err(|e| AssignmentError::ConfigSave(e.to_string()))?;
        Ok(())
    }

    /// Convert to BlockAssignment
    pub fn to_block_assignment(&self) -> BlockAssignment {
        let mut assignment = BlockAssignment::new(self.replication_factor);

        for block_config in &self.blocks {
            let layer_block = LayerBlock {
                block_id: block_config.block_id,
                layer_range: block_config.layer_range,
                model_id: block_config.model_id.clone(),
                estimated_vram_gb: block_config.estimated_vram_gb,
            };
            assignment.add_block(layer_block);

            for replica_config in &block_config.replicas {
                assignment.assign_replica(block_config.block_id, replica_config.node_id.clone());
            }
        }

        assignment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_assignment_creation() {
        let mut assignment = BlockAssignment::new(3);

        assignment.add_block(LayerBlock {
            block_id: 0,
            layer_range: (0, 9),
            model_id: "mistral-7b".to_string(),
            estimated_vram_gb: 2.5,
        });

        assert_eq!(assignment.blocks.len(), 1);
        assert_eq!(assignment.get_block_ids(), vec![0]);
    }

    #[test]
    fn test_replica_assignment() {
        let mut assignment = BlockAssignment::new(3);

        assignment.add_block(LayerBlock {
            block_id: 0,
            layer_range: (0, 9),
            model_id: "mistral-7b".to_string(),
            estimated_vram_gb: 2.5,
        });

        assignment.assign_replica(0, "node-a".to_string());
        assignment.assign_replica(0, "node-b".to_string());
        assignment.assign_replica(0, "node-c".to_string());

        let replicas = assignment.get_replicas(0);
        assert_eq!(replicas.len(), 3);
        assert!(replicas.iter().all(|r| r.is_healthy));
    }

    #[test]
    fn test_healthy_replicas() {
        let mut assignment = BlockAssignment::new(3);

        assignment.add_block(LayerBlock {
            block_id: 0,
            layer_range: (0, 9),
            model_id: "mistral-7b".to_string(),
            estimated_vram_gb: 2.5,
        });

        assignment.assign_replica(0, "node-a".to_string());
        assignment.assign_replica(0, "node-b".to_string());
        assignment.assign_replica(0, "node-c".to_string());

        assignment.mark_replica_unhealthy(0, "node-b");

        let healthy = assignment.get_healthy_replicas(0);
        assert_eq!(healthy.len(), 2);
        assert!(!healthy.iter().any(|r| r.node_id == "node-b"));
    }

    #[test]
    fn test_best_replica_selection() {
        let mut assignment = BlockAssignment::new(3);

        assignment.add_block(LayerBlock {
            block_id: 0,
            layer_range: (0, 9),
            model_id: "mistral-7b".to_string(),
            estimated_vram_gb: 2.5,
        });

        assignment.assign_replica(0, "node-a".to_string());
        assignment.assign_replica(0, "node-b".to_string());
        assignment.assign_replica(0, "node-c".to_string());

        assignment.update_replica_load(0, "node-a", 0.8);
        assignment.update_replica_load(0, "node-b", 0.3);
        assignment.update_replica_load(0, "node-c", 0.5);

        let best = assignment.select_best_replica(0);
        assert_eq!(best, Some("node-b".to_string()));
    }

    #[test]
    fn test_static_config_conversion() {
        let static_config = StaticBlockConfig {
            total_blocks: 2,
            replication_factor: 2,
            blocks: vec![
                BlockConfig {
                    block_id: 0,
                    layer_range: (0, 9),
                    model_id: "mistral-7b".to_string(),
                    estimated_vram_gb: 2.5,
                    replicas: vec![
                        ReplicaConfig {
                            node_id: "node-a".to_string(),
                            priority: 1,
                        },
                        ReplicaConfig {
                            node_id: "node-b".to_string(),
                            priority: 2,
                        },
                    ],
                },
                BlockConfig {
                    block_id: 1,
                    layer_range: (10, 19),
                    model_id: "mistral-7b".to_string(),
                    estimated_vram_gb: 2.5,
                    replicas: vec![
                        ReplicaConfig {
                            node_id: "node-c".to_string(),
                            priority: 1,
                        },
                        ReplicaConfig {
                            node_id: "node-d".to_string(),
                            priority: 2,
                        },
                    ],
                },
            ],
        };

        let assignment = static_config.to_block_assignment();
        assert_eq!(assignment.blocks.len(), 2);
        assert_eq!(assignment.get_replicas(0).len(), 2);
        assert_eq!(assignment.get_replicas(1).len(), 2);
        assert!(assignment.has_minimum_replicas(0));
        assert!(assignment.has_minimum_replicas(1));
    }
}
