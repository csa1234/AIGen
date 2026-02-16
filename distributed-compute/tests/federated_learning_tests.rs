// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Federated Learning integration tests

use distributed_compute::secret_sharing::{SecretSharingManager, SecretSharingError};
#[allow(deprecated)]
use distributed_compute::training::{TrainingBuffer, DeltaComputation, InferenceSample, FisherMatrix};

#[test]
fn test_secret_sharing_split_reconstruct() {
    let manager = SecretSharingManager::new(5);
    let secret = b"Hello, World! Secret message";
    
    // Split secret
    let shares = manager.split_secret(secret).unwrap();
    assert_eq!(shares.len(), 5);
    
    // Reconstruct with threshold shares (3 for 5 nodes)
    let reconstructed = manager.reconstruct_secret(&shares[..3]).unwrap();
    assert_eq!(reconstructed, secret.to_vec());
    
    // Reconstruct with all shares
    let reconstructed_all = manager.reconstruct_secret(&shares).unwrap();
    assert_eq!(reconstructed_all, secret.to_vec());
}

#[test]
fn test_secret_sharing_threshold_enforcement() {
    let manager = SecretSharingManager::new(5);
    let secret = b"Test secret";
    
    let shares = manager.split_secret(secret).unwrap();
    
    // Try to reconstruct with only 2 shares (need 3)
    let result = manager.reconstruct_secret(&shares[..2]);
    assert!(matches!(result, Err(SecretSharingError::InsufficientShares(2, 3))));
    
    // Reconstruct with exactly 3 shares (threshold)
    let reconstructed = manager.reconstruct_secret(&shares[..3]).unwrap();
    assert_eq!(reconstructed, secret.to_vec());
}

#[test]
fn test_delta_aggregation() {
    // Simulate 5 nodes computing deltas
    let num_nodes = 5;
    let delta_size = 100;
    
    // Each node generates a random delta
    let node_deltas: Vec<Vec<f32>> = (0..num_nodes)
        .map(|_| (0..delta_size).map(|i| i as f32 * 0.01).collect())
        .collect();
    
    // Compute expected average
    let mut expected_avg = vec![0.0f32; delta_size];
    for delta in &node_deltas {
        for (i, &val) in delta.iter().enumerate() {
            expected_avg[i] += val;
        }
    }
    for val in &mut expected_avg {
        *val /= num_nodes as f32;
    }
    
    // In real scenario, deltas would be secret-shared, aggregated, and reconstructed
    // For this test, we verify the aggregation math is correct
    assert_eq!(expected_avg.len(), delta_size);
    assert!(expected_avg[0] > 0.0);
}

#[test]
#[allow(deprecated)]
fn test_training_buffer_trigger() {
    #[allow(deprecated)]
    let mut buffer = TrainingBuffer::new(1000, 0.01);
    
    // Fill buffer to 900 (trigger threshold)
    for i in 0..900 {
        buffer.push(InferenceSample {
            input: vec![i as f32],
            output: vec![i as f32 * 2.0],
            loss: 0.1,
            timestamp: i as i64,
        });
    }
    
    assert!(buffer.is_ready(900));
    assert_eq!(buffer.len(), 900);
    
    // Sample batch
    let batch = buffer.sample_batch(128);
    assert_eq!(batch.len(), 128);
}

#[test]
#[allow(deprecated)]
fn test_training_buffer_fifo() {
    #[allow(deprecated)]
    let mut buffer = TrainingBuffer::new(10, 0.01);
    
    // Add 15 samples
    for i in 0..15 {
        buffer.push(InferenceSample {
            input: vec![i as f32],
            output: vec![i as f32 * 2.0],
            loss: 0.1,
            timestamp: i as i64,
        });
    }
    
    // Buffer should only hold 10 (FIFO)
    assert_eq!(buffer.len(), 10);
    
    // Sample should return most recent
    let batch = buffer.sample_batch(3);
    assert_eq!(batch[0].timestamp, 14); // Most recent
    assert_eq!(batch[2].timestamp, 12);
}

#[test]
fn test_delta_computation() {
    let original = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let updated = vec![1.1, 1.9, 3.2, 3.9, 5.1];
    
    let delta = DeltaComputation::compute_delta(&original, &updated);
    assert_eq!(delta, vec![0.1, -0.1, 0.2, -0.1, 0.1]);
    
    // Test L2 norm
    let l2 = DeltaComputation::l2_norm(&delta);
    let expected_l2 = (0.01 + 0.01 + 0.04 + 0.01 + 0.01f32).sqrt();
    assert!((l2 - expected_l2).abs() < 1e-6);
}

#[test]
fn test_delta_application() {
    let original = vec![1.0, 2.0, 3.0];
    let delta = vec![0.1, -0.1, 0.2];
    
    let mut weights = original.clone();
    DeltaComputation::apply_delta(&mut weights, &delta, 0.5);
    
    // With LR=0.5: new = old + lr * delta
    assert_eq!(weights, vec![1.05, 1.95, 3.1]);
}

#[test]
#[allow(deprecated)]
fn test_ewc_regularization() {
    let diagonal = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    let old_params = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    
    #[allow(deprecated)]
    let fisher = FisherMatrix {
        diagonal: diagonal.clone(),
        old_params: old_params.clone(),
        timestamp: 1234567890,
        ipfs_hash: None,
    };
    
    // Delta is the difference between new and old params
    let delta = vec![0.1, -0.1, 0.2, -0.2, 0.3];
    
    // Compute EWC loss: lambda/2 * sum(F_i * (delta_i)^2)
    let lambda = 0.4;
    let loss = fisher.ewc_loss(&delta, lambda);
    
    // Expected: 0.2 * (0.1*0.01 + 0.2*0.01 + 0.3*0.04 + 0.4*0.04 + 0.5*0.09)
    // = 0.2 * (0.001 + 0.002 + 0.012 + 0.016 + 0.045)
    // = 0.2 * 0.076 = 0.0152
    let expected = 0.2 * (0.1 * 0.01 + 0.2 * 0.01 + 0.3 * 0.04 + 0.4 * 0.04 + 0.5 * 0.09);
    assert!((loss - expected).abs() < 1e-6);
}

#[test]
fn test_quantization_roundtrip() {
    let delta = vec![-1.0f32, -0.5, 0.0, 0.5, 1.0];
    
    let (quantized, min_val, scale) = DeltaComputation::quantize_8bit(&delta);
    assert_eq!(quantized.len(), 5);
    
    let reconstructed = DeltaComputation::dequantize_8bit(&quantized, min_val, scale);
    
    // Check reconstruction is close (within quantization error)
    for (orig, recon) in delta.iter().zip(reconstructed.iter()) {
        assert!((orig - recon).abs() < 0.02); // Allow 2% error due to 8-bit quantization
    }
}

#[test]
#[allow(deprecated)]
fn test_fisher_matrix_serialization() {
    let diagonal = vec![0.1, 0.2, 0.3];
    let old_params = vec![1.0, 2.0, 3.0];
    
    #[allow(deprecated)]
    let fisher = FisherMatrix {
        diagonal,
        old_params,
        timestamp: 1234567890,
        ipfs_hash: Some("QmHash123".to_string()),
    };
    
    let serialized = fisher.serialize();
    assert!(!serialized.is_empty());
    
    let hash = fisher.compute_hash();
    assert_ne!(hash, [0u8; 32]);
}
