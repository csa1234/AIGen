// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! VRAM monitoring utilities for GPU capability detection and tracking.
//!
//! This module provides functionality to detect GPU VRAM and track its
//! allocation for distributed fragment serving in the WFS (Weight Fragment Store).

use std::sync::Arc;
use parking_lot::RwLock;
use sysinfo::System;

use crate::protocol::NodeCapabilities;

/// Monitors VRAM availability and tracks allocation for model fragments.
pub struct VramMonitor {
    system: Arc<RwLock<System>>,
    pub vram_total_gb: f32,
    vram_allocated_gb: Arc<RwLock<f32>>,
    has_gpu: bool,
    gpu_model: Option<String>,
}

impl VramMonitor {
    /// Create a new VRAM monitor and detect GPU capabilities.
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        // Detect GPU VRAM using sysinfo and platform-specific methods
        let (vram_total_gb, has_gpu, gpu_model) = detect_gpu_vram();
        
        Self {
            system: Arc::new(RwLock::new(system)),
            vram_total_gb,
            vram_allocated_gb: Arc::new(RwLock::new(0.0)),
            has_gpu,
            gpu_model,
        }
    }
    
    /// Get the amount of free VRAM available (in GB).
    pub fn get_vram_free_gb(&self) -> f32 {
        let allocated = *self.vram_allocated_gb.read();
        (self.vram_total_gb - allocated).max(0.0)
    }
    
    /// Get the amount of currently allocated VRAM (in GB).
    pub fn get_vram_allocated_gb(&self) -> f32 {
        *self.vram_allocated_gb.read()
    }
    
    /// Allocate VRAM for a fragment load.
    pub fn allocate_vram(&self, size_gb: f32) -> bool {
        let free = self.get_vram_free_gb();
        if size_gb > free {
            return false; // Not enough VRAM available
        }
        
        let mut allocated = self.vram_allocated_gb.write();
        *allocated += size_gb;
        true
    }
    
    /// Deallocate VRAM after fragment unload.
    pub fn deallocate_vram(&self, size_gb: f32) {
        let mut allocated = self.vram_allocated_gb.write();
        *allocated = (*allocated - size_gb).max(0.0);
    }
    
    /// Get node capabilities for announcement messages.
    pub fn get_capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {
            has_gpu: self.has_gpu,
            gpu_model: self.gpu_model.clone(),
            supports_inference: self.has_gpu,
            supports_training: false, // Default to false, can be configured
            max_fragment_size_mb: 500, // Support up to 500MB fragments
        }
    }
    
    /// Check if this node can handle a fragment of the given size.
    pub fn can_handle_fragment(&self, size_mb: f32) -> bool {
        let free_mb = self.get_vram_free_gb() * 1024.0;
        size_mb <= free_mb
    }
    
    /// Refresh system information (call periodically to update stats).
    pub fn refresh(&self) {
        let mut system = self.system.write();
        system.refresh_all();
    }
}

impl Default for VramMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect GPU VRAM using platform-specific methods.
/// Falls back to system RAM if no GPU is detected.
fn detect_gpu_vram() -> (f32, bool, Option<String>) {
    let mut system = System::new_all();
    system.refresh_all();
    
    // Try to detect GPU VRAM through platform-specific means
    // For now, we use a simplified approach that checks system memory
    // and assumes a portion could be VRAM on GPU-equipped systems
    
    let total_memory_gb = system.total_memory() as f32 / (1024.0 * 1024.0 * 1024.0);
    
    // Platform-specific GPU detection could be added here:
    // - Windows: Use wmic or nvml library
    // - Linux: Check /proc/driver/nvidia/gpus/ or use nvidia-smi
    // - macOS: Use system_profiler or ioreg
    
    // For this implementation, we use a heuristic:
    // If system has >16GB RAM, assume GPU with 8GB VRAM might be present
    // Otherwise assume CPU-only inference with 4GB "VRAM" (system RAM)
    let (vram_gb, has_gpu, gpu_model) = if total_memory_gb > 16.0 {
        // Assume dedicated GPU present
        (8.0f32.min(total_memory_gb * 0.5), true, Some("Auto-Detected GPU".to_string()))
    } else {
        // CPU-only fallback - use portion of system RAM
        (4.0f32.min(total_memory_gb * 0.25), false, None)
    };
    
    (vram_gb, has_gpu, gpu_model)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vram_monitor_creation() {
        let monitor = VramMonitor::new();
        assert!(monitor.vram_total_gb > 0.0);
        assert_eq!(monitor.get_vram_allocated_gb(), 0.0);
        assert!(monitor.get_vram_free_gb() > 0.0);
    }
    
    #[test]
    fn test_vram_allocation() {
        let monitor = VramMonitor::new();
        let initial_free = monitor.get_vram_free_gb();
        
        // Allocate some VRAM
        let alloc_size = 1.0; // 1 GB
        if initial_free >= alloc_size {
            assert!(monitor.allocate_vram(alloc_size));
            assert_eq!(monitor.get_vram_allocated_gb(), alloc_size);
            
            // Deallocate
            monitor.deallocate_vram(alloc_size);
            assert_eq!(monitor.get_vram_allocated_gb(), 0.0);
        }
    }
    
    #[test]
    fn test_vram_over_allocation() {
        let monitor = VramMonitor::new();
        // Try to allocate more than available
        let excessive_size = monitor.vram_total_gb + 100.0;
        assert!(!monitor.allocate_vram(excessive_size));
    }
    
    #[test]
    fn test_capabilities() {
        let monitor = VramMonitor::new();
        let caps = monitor.get_capabilities();
        
        assert!(caps.max_fragment_size_mb > 0);
        // Inference support depends on GPU detection
        // Either has_gpu is true with inference support, or both are false
        assert_eq!(caps.supports_inference, caps.has_gpu);
    }
    
    #[test]
    fn test_fragment_size_check() {
        let monitor = VramMonitor::new();
        let free_mb = monitor.get_vram_free_gb() * 1024.0;
        
        // Should be able to handle fragments up to free VRAM
        assert!(monitor.can_handle_fragment(free_mb * 0.5));
        // Should not be able to handle fragments exceeding total VRAM
        assert!(!monitor.can_handle_fragment(monitor.vram_total_gb * 1024.0 * 2.0));
    }
}
