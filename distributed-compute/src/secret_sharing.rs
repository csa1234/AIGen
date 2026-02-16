// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Shamir's Secret Sharing module for privacy-preserving gradient aggregation
//! 
//! Implements threshold secret sharing using polynomial interpolation over GF(2^256).
//! Threshold t = N/2 + 1 ensures no single node can reconstruct another's delta.

use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for secret sharing operations
#[derive(Debug, Error)]
pub enum SecretSharingError {
    #[error("insufficient shares for reconstruction: got {0}, need {1}")]
    InsufficientShares(usize, usize),
    #[error("duplicate share indices detected")]
    DuplicateIndices,
    #[error("invalid share index: {0}")]
    InvalidIndex(u32),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("reconstruction failed: {0}")]
    ReconstructionFailed(String),
}

/// A single secret share
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretShare {
    /// Share index (distinct point on polynomial)
    pub index: u32,
    /// Share value (polynomial evaluation at index)
    pub value: Vec<u8>,
    /// Threshold required for reconstruction
    pub threshold: usize,
    /// Total number of shares generated
    pub total_shares: usize,
}

/// Bundle of shares for a complete delta vector
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeltaShareBundle {
    /// Node that generated these shares
    pub node_id: String,
    /// Round ID for this training iteration
    pub round_id: uuid::Uuid,
    /// The share index (polynomial evaluation point) for this bundle
    pub share_index: u32,
    /// The share value containing the full delta bytes
    pub share_value: Vec<u8>,
    /// Threshold required for reconstruction
    pub threshold: usize,
    /// Total number of shares generated
    pub total_shares: usize,
    /// Shape of original delta vector
    pub delta_shape: Vec<usize>,
    /// Hash of original delta for verification
    pub delta_hash: [u8; 32],
}

/// Aggregated share from multiple peers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregatedShare {
    /// Node that aggregated these shares
    pub node_id: String,
    /// Round ID
    pub round_id: uuid::Uuid,
    /// The share index from the original SecretShare
    pub share_index: u32,
    /// Aggregated value (sum of shares)
    pub aggregated_value: Vec<u8>,
    /// Number of shares aggregated
    pub share_count: usize,
}

/// Secret sharing manager
pub struct SecretSharingManager {
    /// Default threshold (t = N/2 + 1)
    threshold: usize,
    /// Total number of nodes
    total_nodes: usize,
}

impl SecretSharingManager {
    /// Create new secret sharing manager
    pub fn new(total_nodes: usize) -> Self {
        let threshold = total_nodes / 2 + 1;
        Self {
            threshold,
            total_nodes,
        }
    }

    /// Split a secret into N shares with threshold t
    /// 
    /// Uses polynomial interpolation: 
    /// - Generate random polynomial of degree t-1
    /// - Secret is the constant term (a_0)
    /// - Evaluate at N distinct points to create shares
    pub fn split_secret(&self, secret: &[u8]) -> Result<Vec<SecretShare>, SecretSharingError> {
        let mut shares: Vec<Vec<u8>> = vec![vec![0u8; secret.len()]; self.total_nodes];
        
        // For each byte position, create a separate polynomial
        for (byte_idx, &secret_byte) in secret.iter().enumerate() {
            let byte_shares = self.split_byte(secret_byte)?;
            
            // Distribute shares to each node
            for (node_idx, share_value) in byte_shares.iter().enumerate() {
                shares[node_idx][byte_idx] = *share_value;
            }
        }
        
        // Create SecretShare structs
        let mut result = Vec::with_capacity(self.total_nodes);
        for (idx, value) in shares.into_iter().enumerate() {
            result.push(SecretShare {
                index: (idx + 1) as u32, // Use 1-based indexing
                value,
                threshold: self.threshold,
                total_shares: self.total_nodes,
            });
        }
        
        Ok(result)
    }

    /// Split a single byte into shares using polynomial over GF(256)
    fn split_byte(&self, secret: u8) -> Result<Vec<u8>, SecretSharingError> {
        // Generate random polynomial coefficients (degree t-1)
        let mut coefficients = vec![0u8; self.threshold];
        coefficients[0] = secret; // Secret is the constant term
        
        // Fill remaining coefficients with random values
        let mut rng = rand::thread_rng();
        for coef in coefficients.iter_mut().skip(1) {
            let mut buf = [0u8; 1];
            rng.fill_bytes(&mut buf);
            *coef = buf[0];
        }
        
        // Evaluate polynomial at N distinct points
        let mut shares = Vec::with_capacity(self.total_nodes);
        for x in 1..=self.total_nodes {
            let x_u8 = x as u8;
            let y = self.evaluate_polynomial(&coefficients, x_u8);
            shares.push(y);
        }
        
        Ok(shares)
    }

    /// Evaluate polynomial at point x using Horner's method
    /// Polynomial: f(x) = a_0 + a_1*x + a_2*x^2 + ... + a_{t-1}*x^{t-1}
    fn evaluate_polynomial(&self, coefficients: &[u8], x: u8) -> u8 {
        let mut result = 0u8;
        let x_powers = self.compute_powers(x, coefficients.len());
        
        for (i, &coef) in coefficients.iter().enumerate() {
            result = gf256_add(result, gf256_mul(coef, x_powers[i]));
        }
        
        result
    }

    /// Compute powers of x: [x^0, x^1, x^2, ..., x^{n-1}]
    fn compute_powers(&self, x: u8, n: usize) -> Vec<u8> {
        let mut powers = vec![1u8; n];
        for i in 1..n {
            powers[i] = gf256_mul(powers[i - 1], x);
        }
        powers
    }

    /// Reconstruct secret from at least t shares using Lagrange interpolation
    pub fn reconstruct_secret(&self, shares: &[SecretShare]) -> Result<Vec<u8>, SecretSharingError> {
        if shares.len() < self.threshold {
            return Err(SecretSharingError::InsufficientShares(
                shares.len(),
                self.threshold,
            ));
        }
        
        // Check for duplicate indices
        let mut indices = std::collections::HashSet::new();
        for share in shares {
            if !indices.insert(share.index) {
                return Err(SecretSharingError::DuplicateIndices);
            }
        }
        
        // Get the length of secret from first share
        let secret_len = shares[0].value.len();
        let mut secret = vec![0u8; secret_len];
        
        // Reconstruct each byte position
        for byte_idx in 0..secret_len {
            let byte_shares: Vec<(u8, u8)> = shares
                .iter()
                .map(|s| (s.index as u8, s.value[byte_idx]))
                .collect();
            
            secret[byte_idx] = self.lagrange_interpolate_at_zero(&byte_shares)?;
        }
        
        Ok(secret)
    }

    /// Lagrange interpolation to find f(0) from shares
    fn lagrange_interpolate_at_zero(&self, shares: &[(u8, u8)]) -> Result<u8, SecretSharingError> {
        let mut result = 0u8;
        
        for (i, &(x_i, y_i)) in shares.iter().enumerate() {
            let mut lagrange_basis = 1u8;
            
            for (j, &(x_j, _)) in shares.iter().enumerate() {
                if i != j {
                    // Compute L_i(0) = product_{j!=i} (0 - x_j) / (x_i - x_j)
                    //             = product_{j!=i} (-x_j) / (x_i - x_j)
                    let numerator = gf256_neg(x_j);
                    let denominator = gf256_sub(x_i, x_j);
                    
                    if denominator == 0 {
                        return Err(SecretSharingError::ReconstructionFailed(
                            "Division by zero in Lagrange interpolation".to_string(),
                        ));
                    }
                    
                    lagrange_basis = gf256_mul(lagrange_basis, gf256_div(numerator, denominator)?);
                }
            }
            
            result = gf256_add(result, gf256_mul(y_i, lagrange_basis));
        }
        
        Ok(result)
    }

    /// Split a delta vector (model weight deltas) into encrypted share bundles
    /// 
    /// Each bundle contains a single SecretShare with the FULL delta byte vector,
    /// not broken down per-byte. This allows proper reconstruction.
    pub fn split_delta(
        &self,
        node_id: String,
        round_id: uuid::Uuid,
        delta: &[f32],
    ) -> Result<Vec<DeltaShareBundle>, SecretSharingError> {
        // Serialize delta to bytes
        let delta_bytes = self.serialize_delta(delta)?;
        
        // Compute hash for verification
        let delta_hash = blockchain_core::crypto::hash_data(&delta_bytes);
        
        // Split the full delta bytes into shares
        // Each share returned contains the FULL delta byte vector
        let shares = self.split_secret(&delta_bytes)?;
        
        // Create one bundle per node, each containing the full share value
        let mut bundles = Vec::with_capacity(self.total_nodes);
        for (_node_idx, share) in shares.into_iter().enumerate() {
            bundles.push(DeltaShareBundle {
                node_id: node_id.clone(),
                round_id,
                share_index: share.index,
                share_value: share.value,
                threshold: share.threshold,
                total_shares: share.total_shares,
                delta_shape: vec![delta.len()],
                delta_hash,
            });
        }
        
        Ok(bundles)
    }

    /// Aggregate received shares locally before sending to aggregator
    /// Each node sums all shares it received (one from each peer)
    /// 
    /// Each DeltaShareBundle contains a single share_value with the full delta bytes.
    /// We sum these values position-wise across all bundles.
    pub fn aggregate_shares(
        &self,
        node_id: String,
        round_id: uuid::Uuid,
        received_shares: &[DeltaShareBundle],
    ) -> Result<AggregatedShare, SecretSharingError> {
        if received_shares.is_empty() {
            return Err(SecretSharingError::InsufficientShares(0, 1));
        }
        
        // Validate all bundles have identical share_value lengths
        let expected_len = received_shares[0].share_value.len();
        for bundle in received_shares.iter().skip(1) {
            if bundle.share_value.len() != expected_len {
                return Err(SecretSharingError::ReconstructionFailed(
                    format!("Inconsistent share lengths: expected {}, got {}", 
                        expected_len, bundle.share_value.len()),
                ));
            }
        }
        
        // Sum all share values position-wise (GF(256) addition is XOR)
        let mut aggregated = vec![0u8; expected_len];
        for bundle in received_shares {
            for (j, &byte_val) in bundle.share_value.iter().enumerate() {
                aggregated[j] = gf256_add(aggregated[j], byte_val);
            }
        }
        
        // Use the share_index from the first bundle (all should be the same for this node)
        let share_index = received_shares[0].share_index;
        
        Ok(AggregatedShare {
            node_id,
            round_id,
            share_index,
            aggregated_value: aggregated,
            share_count: received_shares.len(),
        })
    }

    /// Serialize delta vector to bytes
    fn serialize_delta(&self, delta: &[f32]) -> Result<Vec<u8>, SecretSharingError> {
        let mut bytes = Vec::with_capacity(delta.len() * 4);
        for &val in delta {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        Ok(bytes)
    }

    /// Deserialize bytes back to delta vector
    pub fn deserialize_delta(&self, bytes: &[u8]) -> Result<Vec<f32>, SecretSharingError> {
        if bytes.len() % 4 != 0 {
            return Err(SecretSharingError::Serialization(
                "Invalid byte length for f32 delta".to_string(),
            ));
        }
        
        let num_floats = bytes.len() / 4;
        let mut delta = Vec::with_capacity(num_floats);
        
        for i in 0..num_floats {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&bytes[i * 4..(i + 1) * 4]);
            delta.push(f32::from_le_bytes(buf));
        }
        
        Ok(delta)
    }

    /// Get threshold
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Get total nodes
    pub fn total_nodes(&self) -> usize {
        self.total_nodes
    }
}

// GF(2^8) arithmetic operations using AES irreducible polynomial
// AES polynomial: x^8 + x^4 + x^3 + x + 1 = 0x11B

const AES_IRREDUCIBLE: u16 = 0x11B;

/// Addition in GF(256) is XOR
fn gf256_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Subtraction in GF(256) is the same as addition
fn gf256_sub(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Negation in GF(256) is identity
fn gf256_neg(a: u8) -> u8 {
    a
}

/// Multiplication in GF(256) using Russian peasant algorithm
fn gf256_mul(a: u8, b: u8) -> u8 {
    let mut result = 0u8;
    let mut a = a;
    let mut b = b;
    
    while b != 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        
        let high_bit_set = a & 0x80 != 0;
        a <<= 1;
        if high_bit_set {
            a ^= 0x1B; // XOR with 0x1B (lower 8 bits of 0x11B)
        }
        
        b >>= 1;
    }
    
    result
}

/// Division in GF(256): a / b = a * b^{-1}
fn gf256_div(a: u8, b: u8) -> Result<u8, SecretSharingError> {
    if b == 0 {
        return Err(SecretSharingError::ReconstructionFailed(
            "Division by zero".to_string(),
        ));
    }
    
    let inv = gf256_inverse(b);
    Ok(gf256_mul(a, inv))
}

/// Multiplicative inverse using extended Euclidean algorithm
fn gf256_inverse(a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    
    // Extended Euclidean algorithm for GF(256)
    let mut old_r = a as u16;
    let mut r = AES_IRREDUCIBLE;
    let mut old_s = 1u16;
    let mut s = 0u16;
    
    while r != 0 {
        let quotient = old_r / r;
        let temp_r = r;
        r = old_r ^ gf256_mul_poly(quotient, r);
        old_r = temp_r;
        
        let temp_s = s;
        s = old_s ^ gf256_mul_poly(quotient, s);
        old_s = temp_s;
    }
    
    (old_s & 0xFF) as u8
}

/// Polynomial multiplication for extended Euclidean
fn gf256_mul_poly(a: u16, b: u16) -> u16 {
    let mut result = 0u16;
    let mut a = a;
    let mut b = b;
    
    while b != 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        
        let high_bit_set = a & 0x8000 != 0;
        a <<= 1;
        if high_bit_set {
            a ^= AES_IRREDUCIBLE;
        }
        
        b >>= 1;
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_reconstruct_secret() {
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
    fn test_insufficient_shares_fails() {
        let manager = SecretSharingManager::new(5);
        let secret = b"Test";
        
        let shares = manager.split_secret(secret).unwrap();
        
        // Try to reconstruct with only 2 shares (need 3)
        let result = manager.reconstruct_secret(&shares[..2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_duplicate_indices_fails() {
        let manager = SecretSharingManager::new(5);
        let secret = b"Test";
        
        let mut shares = manager.split_secret(secret).unwrap();
        // Make two shares have same index
        shares[1].index = shares[0].index;
        
        let result = manager.reconstruct_secret(&shares[..3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_gf256_arithmetic() {
        // Test addition/subtraction (XOR)
        assert_eq!(gf256_add(0x5A, 0x3C), 0x66);
        assert_eq!(gf256_sub(0x5A, 0x3C), 0x66);
        
        // Test multiplication
        let prod = gf256_mul(0x57, 0x83);
        assert_ne!(prod, 0);
        
        // Test inverse
        let inv = gf256_inverse(0x57);
        assert_eq!(gf256_mul(0x57, inv), 1);
    }

    #[test]
    fn test_delta_serialization() {
        let manager = SecretSharingManager::new(3);
        let delta = vec![1.0f32, 2.0, 3.0, -1.5, 0.001];
        
        let bytes = manager.serialize_delta(&delta).unwrap();
        let reconstructed = manager.deserialize_delta(&bytes).unwrap();
        
        assert_eq!(delta, reconstructed);
    }

    #[test]
    fn test_split_delta() {
        let manager = SecretSharingManager::new(3);
        let delta = vec![1.0f32, 2.0, 3.0];
        let node_id = "node1".to_string();
        let round_id = uuid::Uuid::new_v4();
        
        let bundles = manager.split_delta(node_id, round_id, &delta).unwrap();
        assert_eq!(bundles.len(), 3);
        
        // Each bundle should have ONE share_value containing the full delta bytes
        let expected_bytes = delta.len() * 4; // 4 bytes per f32
        assert_eq!(bundles[0].share_value.len(), expected_bytes);
        
        // Verify each bundle has a unique share_index
        let indices: std::collections::HashSet<u32> = bundles.iter().map(|b| b.share_index).collect();
        assert_eq!(indices.len(), 3);
    }
}
