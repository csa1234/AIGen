// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Comprehensive network metrics for distributed compute system.
//!
//! Tracks per-node, per-fragment, and global metrics for VRAM pool
//! visualization and performance monitoring.

use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

// Scale factor for fixed-point representation (2 decimal places)
const FP_SCALE: f32 = 100.0;

/// Helper to convert f32 to fixed-point u32
fn f32_to_fp(value: f32) -> u32 {
    (value * FP_SCALE) as u32
}

/// Helper to convert fixed-point u32 to f32
fn fp_to_f32(value: u32) -> f32 {
    value as f32 / FP_SCALE
}

/// Per-node metrics for tracking performance and activity
#[derive(Debug)]
pub struct NodeMetrics {
    pub node_id: String,
    pub tasks_completed: AtomicU64,
    pub tasks_failed: AtomicU64,
    pub total_compute_time_ms: AtomicU64,
    pub avg_latency_ms: AtomicU32, // Fixed-point: actual = value / 100.0
    pub bandwidth_sent_bytes: AtomicU64,
    pub bandwidth_received_bytes: AtomicU64,
    pub uptime_seconds: AtomicU64,
    pub last_heartbeat: AtomicU64,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            tasks_completed: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            total_compute_time_ms: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(0),
            bandwidth_sent_bytes: AtomicU64::new(0),
            bandwidth_received_bytes: AtomicU64::new(0),
            uptime_seconds: AtomicU64::new(0),
            last_heartbeat: AtomicU64::new(0),
        }
    }
}

impl Clone for NodeMetrics {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id.clone(),
            tasks_completed: AtomicU64::new(self.tasks_completed.load(Ordering::Relaxed)),
            tasks_failed: AtomicU64::new(self.tasks_failed.load(Ordering::Relaxed)),
            total_compute_time_ms: AtomicU64::new(self.total_compute_time_ms.load(Ordering::Relaxed)),
            avg_latency_ms: AtomicU32::new(self.avg_latency_ms.load(Ordering::Relaxed)),
            bandwidth_sent_bytes: AtomicU64::new(self.bandwidth_sent_bytes.load(Ordering::Relaxed)),
            bandwidth_received_bytes: AtomicU64::new(self.bandwidth_received_bytes.load(Ordering::Relaxed)),
            uptime_seconds: AtomicU64::new(self.uptime_seconds.load(Ordering::Relaxed)),
            last_heartbeat: AtomicU64::new(self.last_heartbeat.load(Ordering::Relaxed)),
        }
    }
}

impl NodeMetrics {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            ..Default::default()
        }
    }

    pub fn record_task_completion(&self, compute_time_ms: u64) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
        self.total_compute_time_ms.fetch_add(compute_time_ms, Ordering::Relaxed);
    }

    pub fn record_task_failure(&self) {
        self.tasks_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_bandwidth(&self, sent_bytes: u64, received_bytes: u64) {
        self.bandwidth_sent_bytes.fetch_add(sent_bytes, Ordering::Relaxed);
        self.bandwidth_received_bytes.fetch_add(received_bytes, Ordering::Relaxed);
    }

    pub fn update_latency(&self, latency_ms: f32) {
        // Simple exponential moving average using fixed-point
        let prev_fp = self.avg_latency_ms.load(Ordering::Relaxed);
        let prev = fp_to_f32(prev_fp);
        let new = if prev == 0.0 {
            latency_ms
        } else {
            (prev + latency_ms) / 2.0
        };
        self.avg_latency_ms.store(f32_to_fp(new), Ordering::Relaxed);
    }

    /// Get latency as f32
    pub fn get_latency(&self) -> f32 {
        fp_to_f32(self.avg_latency_ms.load(Ordering::Relaxed))
    }
}

/// Per-fragment metrics for tracking access patterns
#[derive(Debug)]
pub struct FragmentMetrics {
    pub fragment_id: String,
    pub access_count: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub avg_load_time_ms: AtomicU32, // Fixed-point: actual = value / 100.0
    pub total_size_bytes: AtomicU64,
}

impl Default for FragmentMetrics {
    fn default() -> Self {
        Self {
            fragment_id: String::new(),
            access_count: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            avg_load_time_ms: AtomicU32::new(0),
            total_size_bytes: AtomicU64::new(0),
        }
    }
}

impl Clone for FragmentMetrics {
    fn clone(&self) -> Self {
        Self {
            fragment_id: self.fragment_id.clone(),
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
            cache_hits: AtomicU64::new(self.cache_hits.load(Ordering::Relaxed)),
            cache_misses: AtomicU64::new(self.cache_misses.load(Ordering::Relaxed)),
            avg_load_time_ms: AtomicU32::new(self.avg_load_time_ms.load(Ordering::Relaxed)),
            total_size_bytes: AtomicU64::new(self.total_size_bytes.load(Ordering::Relaxed)),
        }
    }
}

impl FragmentMetrics {
    pub fn new(fragment_id: String, size_bytes: u64) -> Self {
        Self {
            fragment_id,
            total_size_bytes: AtomicU64::new(size_bytes),
            ..Default::default()
        }
    }

    pub fn record_access(&self, cache_hit: bool, load_time_ms: u64) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
        if cache_hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
            // Update average load time using fixed-point
            let prev_fp = self.avg_load_time_ms.load(Ordering::Relaxed);
            let prev = fp_to_f32(prev_fp);
            let new = if prev == 0.0 {
                load_time_ms as f32
            } else {
                (prev + load_time_ms as f32) / 2.0
            };
            self.avg_load_time_ms.store(f32_to_fp(new), Ordering::Relaxed);
        }
    }

    /// Get load time as f32
    pub fn get_load_time(&self) -> f32 {
        fp_to_f32(self.avg_load_time_ms.load(Ordering::Relaxed))
    }
}

/// Global metrics for system-wide statistics
#[derive(Debug)]
pub struct GlobalMetrics {
    pub total_inferences: AtomicU64,
    pub total_tasks: AtomicU64,
    pub active_nodes: AtomicU32,
    pub total_vram_gb: AtomicU32, // Fixed-point: actual = value / 100.0
    pub allocated_vram_gb: AtomicU32, // Fixed-point: actual = value / 100.0
    pub avg_pipeline_depth: AtomicU32, // Fixed-point
    pub avg_throughput_tasks_per_sec: AtomicU32, // Fixed-point
    pub compression_ratio: AtomicU32, // Fixed-point: compressed_size / original_size * 100
    pub avg_latency_ms: AtomicU32, // Fixed-point: actual = value / 100.0
}

impl Default for GlobalMetrics {
    fn default() -> Self {
        Self {
            total_inferences: AtomicU64::new(0),
            total_tasks: AtomicU64::new(0),
            active_nodes: AtomicU32::new(0),
            total_vram_gb: AtomicU32::new(0),
            allocated_vram_gb: AtomicU32::new(0),
            avg_pipeline_depth: AtomicU32::new(0),
            avg_throughput_tasks_per_sec: AtomicU32::new(0),
            compression_ratio: AtomicU32::new(0),
            avg_latency_ms: AtomicU32::new(0),
        }
    }
}

impl Clone for GlobalMetrics {
    fn clone(&self) -> Self {
        Self {
            total_inferences: AtomicU64::new(self.total_inferences.load(Ordering::Relaxed)),
            total_tasks: AtomicU64::new(self.total_tasks.load(Ordering::Relaxed)),
            active_nodes: AtomicU32::new(self.active_nodes.load(Ordering::Relaxed)),
            total_vram_gb: AtomicU32::new(self.total_vram_gb.load(Ordering::Relaxed)),
            allocated_vram_gb: AtomicU32::new(self.allocated_vram_gb.load(Ordering::Relaxed)),
            avg_pipeline_depth: AtomicU32::new(self.avg_pipeline_depth.load(Ordering::Relaxed)),
            avg_throughput_tasks_per_sec: AtomicU32::new(self.avg_throughput_tasks_per_sec.load(Ordering::Relaxed)),
            compression_ratio: AtomicU32::new(self.compression_ratio.load(Ordering::Relaxed)),
            avg_latency_ms: AtomicU32::new(self.avg_latency_ms.load(Ordering::Relaxed)),
        }
    }
}

impl GlobalMetrics {
    pub fn update_vram_stats(&self, total_gb: f32, allocated_gb: f32) {
        self.total_vram_gb.store(f32_to_fp(total_gb), Ordering::Relaxed);
        self.allocated_vram_gb.store(f32_to_fp(allocated_gb), Ordering::Relaxed);
    }

    pub fn record_inference(&self, task_count: u64) {
        self.total_inferences.fetch_add(1, Ordering::Relaxed);
        self.total_tasks.fetch_add(task_count, Ordering::Relaxed);
    }

    pub fn update_compression_ratio(&self, ratio: f32) {
        self.compression_ratio.store(f32_to_fp(ratio), Ordering::Relaxed);
    }

    /// Update latency using exponential moving average
    pub fn update_latency(&self, latency_ms: f32) {
        let prev_fp = self.avg_latency_ms.load(Ordering::Relaxed);
        let prev = fp_to_f32(prev_fp);
        let new = if prev == 0.0 {
            latency_ms
        } else {
            (prev + latency_ms) / 2.0
        };
        self.avg_latency_ms.store(f32_to_fp(new), Ordering::Relaxed);
    }

    /// Get VRAM values as f32
    pub fn get_vram_stats(&self) -> (f32, f32) {
        (
            fp_to_f32(self.total_vram_gb.load(Ordering::Relaxed)),
            fp_to_f32(self.allocated_vram_gb.load(Ordering::Relaxed)),
        )
    }

    /// Get compression ratio as f32
    pub fn get_compression_ratio(&self) -> f32 {
        fp_to_f32(self.compression_ratio.load(Ordering::Relaxed))
    }

    /// Get latency as f32
    pub fn get_latency(&self) -> f32 {
        fp_to_f32(self.avg_latency_ms.load(Ordering::Relaxed))
    }
}

/// Metrics for checkpoint operations
#[derive(Debug, Default)]
pub struct CheckpointMetrics {
    pub checkpoints_saved: AtomicU64,
    pub checkpoints_loaded: AtomicU64,
    pub checkpoint_save_time_ms: AtomicU64,
    pub checkpoint_load_time_ms: AtomicU64,
    pub checkpoint_storage_bytes: AtomicU64,
}

impl CheckpointMetrics {
    pub fn record_save(&self, duration: std::time::Duration, size_bytes: usize) {
        self.checkpoints_saved.fetch_add(1, Ordering::Relaxed);
        self.checkpoint_save_time_ms.fetch_add(
            duration.as_millis() as u64,
            Ordering::Relaxed,
        );
        self.checkpoint_storage_bytes.fetch_add(
            size_bytes as u64,
            Ordering::Relaxed,
        );
    }

    pub fn record_load(&self, duration: std::time::Duration) {
        self.checkpoints_loaded.fetch_add(1, Ordering::Relaxed);
        self.checkpoint_load_time_ms.fetch_add(
            duration.as_millis() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn get_stats(&self) -> CheckpointStats {
        let saved = self.checkpoints_saved.load(Ordering::Relaxed);
        let loaded = self.checkpoints_loaded.load(Ordering::Relaxed);

        CheckpointStats {
            total_saved: saved,
            total_loaded: loaded,
            avg_save_time_ms: if saved > 0 {
                self.checkpoint_save_time_ms.load(Ordering::Relaxed) / saved
            } else { 0 },
            avg_load_time_ms: if loaded > 0 {
                self.checkpoint_load_time_ms.load(Ordering::Relaxed) / loaded
            } else { 0 },
            total_storage_bytes: self.checkpoint_storage_bytes.load(Ordering::Relaxed),
        }
    }
}

pub struct CheckpointStats {
    pub total_saved: u64,
    pub total_loaded: u64,
    pub avg_save_time_ms: u64,
    pub avg_load_time_ms: u64,
    pub total_storage_bytes: u64,
}

/// Metrics for failover operations
#[derive(Debug, Default)]
pub struct FailoverMetrics {
    pub total_failovers: AtomicU64,
    pub total_recovery_time_ms: AtomicU64,
    pub total_inferences_recovered: AtomicU64,
    pub total_failover_failures: AtomicU64,
}

impl FailoverMetrics {
    pub fn record_failover(&self, recovery_time: std::time::Duration, inferences_count: usize) {
        self.total_failovers.fetch_add(1, Ordering::Relaxed);
        self.total_recovery_time_ms.fetch_add(
            recovery_time.as_millis() as u64,
            Ordering::Relaxed,
        );
        self.total_inferences_recovered.fetch_add(
            inferences_count as u64,
            Ordering::Relaxed,
        );
    }

    pub fn record_failover_failure(&self) {
        self.total_failover_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> FailoverStats {
        let total = self.total_failovers.load(Ordering::Relaxed);
        let total_time = self.total_recovery_time_ms.load(Ordering::Relaxed);

        FailoverStats {
            total_failovers: total,
            avg_recovery_time_ms: if total > 0 { total_time / total } else { 0 },
            total_inferences_recovered: self.total_inferences_recovered.load(Ordering::Relaxed),
            total_failover_failures: self.total_failover_failures.load(Ordering::Relaxed),
        }
    }
}

pub struct FailoverStats {
    pub total_failovers: u64,
    pub avg_recovery_time_ms: u64,
    pub total_inferences_recovered: u64,
    pub total_failover_failures: u64,
}

/// Per-block latency metrics for distributed inference
#[derive(Debug, Default, Clone)]
pub struct BlockLatencyMetrics {
    pub block_id: u32,
    pub avg_compute_time_ms: f32,
    pub avg_transfer_time_ms: f32,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
}

impl BlockLatencyMetrics {
    pub fn new(block_id: u32) -> Self {
        Self {
            block_id,
            ..Default::default()
        }
    }

    pub fn record_compute_time(&mut self, compute_time_ms: u64) {
        // Simple moving average
        if self.tasks_completed == 0 {
            self.avg_compute_time_ms = compute_time_ms as f32;
        } else {
            self.avg_compute_time_ms = (self.avg_compute_time_ms * self.tasks_completed as f32
                + compute_time_ms as f32) / (self.tasks_completed + 1) as f32;
        }
        self.tasks_completed += 1;
    }

    pub fn record_transfer_time(&mut self, transfer_time_ms: u64) {
        // Simple moving average
        if self.tasks_completed == 0 {
            self.avg_transfer_time_ms = transfer_time_ms as f32;
        } else {
            self.avg_transfer_time_ms = (self.avg_transfer_time_ms * self.tasks_completed as f32
                + transfer_time_ms as f32) / (self.tasks_completed + 1) as f32;
        }
    }

    pub fn record_failure(&mut self) {
        self.tasks_failed += 1;
    }
}

impl Clone for CheckpointStats {
    fn clone(&self) -> Self {
        Self {
            total_saved: self.total_saved,
            total_loaded: self.total_loaded,
            avg_save_time_ms: self.avg_save_time_ms,
            avg_load_time_ms: self.avg_load_time_ms,
            total_storage_bytes: self.total_storage_bytes,
        }
    }
}

impl Clone for FailoverStats {
    fn clone(&self) -> Self {
        Self {
            total_failovers: self.total_failovers,
            avg_recovery_time_ms: self.avg_recovery_time_ms,
            total_inferences_recovered: self.total_inferences_recovered,
            total_failover_failures: self.total_failover_failures,
        }
    }
}

/// Comprehensive network metrics for distributed compute
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    // Per-node metrics
    pub node_metrics: Arc<RwLock<HashMap<String, NodeMetrics>>>,
    // Per-fragment metrics
    pub fragment_metrics: Arc<RwLock<HashMap<String, FragmentMetrics>>>,
    // Per-block latency metrics for distributed inference
    pub per_block_latency: Arc<RwLock<HashMap<u32, BlockLatencyMetrics>>>,
    // Global metrics
    pub global: Arc<RwLock<GlobalMetrics>>,
    // Legacy metrics for backward compatibility
    pub peers_connected: AtomicU64,
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub reputation_bans: AtomicU64,
    pub model_shards_sent: AtomicU64,
    pub model_shards_received: AtomicU64,
    pub model_bytes_sent: AtomicU64,
    pub model_bytes_received: AtomicU64,
    pub model_transfer_failures: AtomicU64,
    pub model_announcements_sent: AtomicU64,
    pub model_announcements_received: AtomicU64,
    // Checkpoint and failover metrics
    pub checkpoint_metrics: Arc<CheckpointMetrics>,
    pub failover_metrics: Arc<FailoverMetrics>,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record task completion for a node
    pub fn record_task_completion(&self, node_id: &str, compute_time_ms: u64) {
        let mut metrics = self.node_metrics.write();
        let node_metrics = metrics.entry(node_id.to_string()).or_insert_with(|| {
            NodeMetrics::new(node_id.to_string())
        });
        node_metrics.record_task_completion(compute_time_ms);
    }

    /// Record task failure for a node
    pub fn record_task_failure(&self, node_id: &str) {
        let mut metrics = self.node_metrics.write();
        let node_metrics = metrics.entry(node_id.to_string()).or_insert_with(|| {
            NodeMetrics::new(node_id.to_string())
        });
        node_metrics.record_task_failure();
    }

    /// Record fragment access (cache hit/miss)
    pub fn record_fragment_access(&self, fragment_id: &str, cache_hit: bool, load_time_ms: u64, size_bytes: u64) {
        let mut metrics = self.fragment_metrics.write();
        let fragment_metrics = metrics.entry(fragment_id.to_string()).or_insert_with(|| {
            FragmentMetrics::new(fragment_id.to_string(), size_bytes)
        });
        fragment_metrics.record_access(cache_hit, load_time_ms);
    }

    /// Record bandwidth usage for a node
    pub fn record_bandwidth(&self, node_id: &str, sent_bytes: u64, received_bytes: u64) {
        let mut metrics = self.node_metrics.write();
        let node_metrics = metrics.entry(node_id.to_string()).or_insert_with(|| {
            NodeMetrics::new(node_id.to_string())
        });
        node_metrics.record_bandwidth(sent_bytes, received_bytes);
        
        // Also update global counters
        self.bytes_sent.fetch_add(sent_bytes, Ordering::Relaxed);
        self.bytes_received.fetch_add(received_bytes, Ordering::Relaxed);
    }

    /// Update global VRAM statistics
    pub fn update_global_stats(&self, active_nodes: u32, total_vram: f32, allocated_vram: f32) {
        let mut global = self.global.write();
        global.active_nodes.store(active_nodes, Ordering::Relaxed);
        global.update_vram_stats(total_vram, allocated_vram);
    }

    /// Record inference completion
    pub fn record_inference(&self, task_count: u64) {
        let global = self.global.write();
        global.record_inference(task_count);
    }

    /// Update compression ratio (quantization + zstd)
    pub fn update_compression_ratio(&self, ratio: f32) {
        let global = self.global.write();
        global.update_compression_ratio(ratio);
    }

    /// Get metrics for a specific node
    pub fn get_node_metrics(&self, node_id: &str) -> Option<NodeMetrics> {
        self.node_metrics.read().get(node_id).cloned()
    }

    /// Get all node metrics
    pub fn get_all_node_metrics(&self) -> Vec<NodeMetrics> {
        self.node_metrics.read().values().cloned().collect()
    }

    /// Get metrics for a specific fragment
    pub fn get_fragment_metrics(&self, fragment_id: &str) -> Option<FragmentMetrics> {
        self.fragment_metrics.read().get(fragment_id).cloned()
    }

    /// Get global metrics
    pub fn get_global_metrics(&self) -> GlobalMetrics {
        self.global.read().clone()
    }

    /// Get checkpoint statistics
    pub fn get_checkpoint_stats(&self) -> CheckpointStats {
        self.checkpoint_metrics.get_stats()
    }

    /// Get failover statistics
    pub fn get_failover_stats(&self) -> FailoverStats {
        self.failover_metrics.get_stats()
    }

    /// Record checkpoint save
    pub fn record_checkpoint_save(&self, duration: std::time::Duration, size_bytes: usize) {
        self.checkpoint_metrics.record_save(duration, size_bytes);
    }

    /// Record checkpoint load
    pub fn record_checkpoint_load(&self, duration: std::time::Duration) {
        self.checkpoint_metrics.record_load(duration);
    }

    /// Record failover
    pub fn record_failover(&self, recovery_time: std::time::Duration, inferences_count: usize) {
        self.failover_metrics.record_failover(recovery_time, inferences_count);
    }

    /// Record failover failure
    pub fn record_failover_failure(&self) {
        self.failover_metrics.record_failover_failure();
    }

    // NEW: Per-block latency tracking for distributed inference
    
    /// Record block task completion with compute and transfer times
    pub fn record_block_task_completion(
        &self,
        block_id: u32,
        compute_time_ms: u64,
        transfer_time_ms: u64,
    ) {
        let mut metrics = self.per_block_latency.write();
        let block_metrics = metrics.entry(block_id).or_insert_with(|| {
            BlockLatencyMetrics::new(block_id)
        });
        block_metrics.record_compute_time(compute_time_ms);
        block_metrics.record_transfer_time(transfer_time_ms);
    }

    /// Record block task failure
    pub fn record_block_task_failure(&self, block_id: u32) {
        let mut metrics = self.per_block_latency.write();
        let block_metrics = metrics.entry(block_id).or_insert_with(|| {
            BlockLatencyMetrics::new(block_id)
        });
        block_metrics.record_failure();
    }

    /// Record tensor compression statistics
    pub fn record_tensor_compression(
        &self,
        original_size_bytes: usize,
        compressed_size_bytes: usize,
    ) {
        if original_size_bytes > 0 {
            let ratio = original_size_bytes as f32 / compressed_size_bytes as f32;
            let global = self.global.write();
            // Update compression ratio with moving average
            let current_ratio = fp_to_f32(global.compression_ratio.load(Ordering::Relaxed));
            let total_inferences = global.total_inferences.load(Ordering::Relaxed).max(1);
            let new_ratio = (current_ratio * (total_inferences - 1) as f32 + ratio) / total_inferences as f32;
            global.compression_ratio.store(f32_to_fp(new_ratio), Ordering::Relaxed);
        }
    }

    /// Get block latency metrics for a specific block
    pub fn get_block_latency_metrics(&self, block_id: u32) -> Option<BlockLatencyMetrics> {
        self.per_block_latency.read().get(&block_id).cloned()
    }

    /// Get all block latency metrics
    pub fn get_all_block_latency_metrics(&self) -> Vec<BlockLatencyMetrics> {
        self.per_block_latency.read().values().cloned().collect()
    }

    // Legacy metric methods for backward compatibility
    pub fn inc_peers_connected(&self) {
        self.peers_connected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_peers_connected(&self) {
        self.peers_connected.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_reputation_bans(&self) {
        self.reputation_bans.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_shards_sent(&self) {
        self.model_shards_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_shards_received(&self) {
        self.model_shards_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_model_bytes_sent(&self, bytes: u64) {
        self.model_bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_model_bytes_received(&self, bytes: u64) {
        self.model_bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_model_transfer_failures(&self) {
        self.model_transfer_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_announcements_sent(&self) {
        self.model_announcements_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_announcements_received(&self) {
        self.model_announcements_received.fetch_add(1, Ordering::Relaxed);
    }
}

pub type SharedNetworkMetrics = Arc<NetworkMetrics>;
