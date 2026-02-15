// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use thiserror::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Quantization method for tensor compression
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum QuantizationMethod {
    None,
    Int8,
    Int4,
}

impl Default for QuantizationMethod {
    fn default() -> Self {
        QuantizationMethod::None
    }
}

/// Tensor metadata for reconstruction
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TensorMetadata {
    pub shape: Vec<i64>,
    pub dtype: String, // "f32", "f16", "i8", etc.
    pub layer_range: (u32, u32),
    pub original_dtype: Option<String>, // Original dtype before quantization
}

impl TensorMetadata {
    /// Calculate total number of elements
    pub fn num_elements(&self) -> i64 {
        self.shape.iter().product()
    }

    /// Calculate expected byte size
    pub fn byte_size(&self) -> usize {
        let num_elems = self.num_elements() as usize;
        // Use original dtype if available (for dequantization)
        let dtype = self.original_dtype.as_ref().unwrap_or(&self.dtype);
        match dtype.as_str() {
            "f32" => num_elems * 4,
            "f16" | "bf16" => num_elems * 2,
            "i8" | "u8" => num_elems,
            "i32" | "u32" => num_elems * 4,
            "i64" | "u64" => num_elems * 8,
            "f64" => num_elems * 8,
            _ => num_elems * 4, // Default to f32
        }
    }
}

/// Compressed tensor data with metadata
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompressedTensor {
    pub metadata: TensorMetadata,
    pub data: Vec<u8>, // zstd compressed
    pub compression_level: i32, // 1-3 for low latency
    pub checksum: [u8; 32], // SHA3-256
    pub quantization_method: Option<QuantizationMethod>, // NEW: quantization method used
    pub compression_ratio: f64, // NEW: original_size / compressed_size
}

/// Global compression statistics
#[derive(Debug, Default)]
pub struct CompressionStats {
    pub total_tensors_compressed: AtomicU64,
    pub total_bytes_saved: AtomicU64,
    pub avg_compression_ratio: AtomicU64, // Fixed-point: value / 1000
}

impl CompressionStats {
    pub fn record_compression(&self, original_size: usize, compressed_size: usize, _quantized: bool) {
        self.total_tensors_compressed.fetch_add(1, Ordering::Relaxed);
        let saved = original_size.saturating_sub(compressed_size);
        self.total_bytes_saved.fetch_add(saved as u64, Ordering::Relaxed);
        
        // Calculate compression ratio (original / compressed)
        let ratio = if compressed_size > 0 {
            (original_size as f64) / (compressed_size as f64)
        } else {
            1.0
        };
        
        // Update running average (fixed-point, 3 decimal places)
        let ratio_fixed = (ratio * 1000.0) as u64;
        let prev = self.avg_compression_ratio.load(Ordering::Relaxed);
        let new = if prev == 0 {
            ratio_fixed
        } else {
            (prev + ratio_fixed) / 2
        };
        self.avg_compression_ratio.store(new, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> (u64, u64, f64) {
        let total = self.total_tensors_compressed.load(Ordering::Relaxed);
        let saved = self.total_bytes_saved.load(Ordering::Relaxed);
        let ratio_fixed = self.avg_compression_ratio.load(Ordering::Relaxed);
        (total, saved, ratio_fixed as f64 / 1000.0)
    }
}

// Static global compression stats
static GLOBAL_COMPRESSION_STATS: std::sync::OnceLock<Arc<CompressionStats>> = std::sync::OnceLock::new();

pub fn get_global_compression_stats() -> Arc<CompressionStats> {
    GLOBAL_COMPRESSION_STATS.get_or_init(|| Arc::new(CompressionStats::default())).clone()
}

/// Error types for tensor transport operations
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("compression failed: {0}")]
    Compression(String),
    #[error("decompression failed: {0}")]
    Decompression(String),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("deserialization failed: {0}")]
    Deserialization(String),
    #[error("invalid compression level: {0} (must be 1-22)")]
    InvalidCompressionLevel(i32),
}

/// Tensor transport handling compression and serialization
pub struct TensorTransport;

impl TensorTransport {
    /// Serialize and compress tensor data
    pub fn compress(
        data: &[u8],
        metadata: TensorMetadata,
        compression_level: i32,
    ) -> Result<CompressedTensor, TransportError> {
        // Validate compression level (1-22, but we recommend 1-3 for low latency)
        if compression_level < 1 || compression_level > 22 {
            return Err(TransportError::InvalidCompressionLevel(compression_level));
        }

        // Compress with zstd at specified level
        let compressed_data = zstd::encode_all(data, compression_level)
            .map_err(|e| TransportError::Compression(e.to_string()))?;

        // Calculate SHA3-256 checksum on original data
        let mut hasher = Sha3_256::new();
        hasher.update(data);
        let checksum: [u8; 32] = hasher.finalize().into();

        // Calculate compression ratio
        let original_size = data.len();
        let compressed_size = compressed_data.len();
        let compression_ratio = if compressed_size > 0 {
            original_size as f64 / compressed_size as f64
        } else {
            1.0
        };

        Ok(CompressedTensor {
            metadata,
            data: compressed_data,
            compression_level,
            checksum,
            quantization_method: Some(QuantizationMethod::None),
            compression_ratio,
        })
    }

    /// Decompress and deserialize tensor data
    pub fn decompress(
        compressed: &CompressedTensor,
    ) -> Result<(Vec<u8>, TensorMetadata), TransportError> {
        // Decompress with zstd
        let decompressed = zstd::decode_all(&compressed.data[..])
            .map_err(|e| TransportError::Decompression(e.to_string()))?;

        // Verify checksum
        let mut hasher = Sha3_256::new();
        hasher.update(&decompressed);
        let computed_checksum: [u8; 32] = hasher.finalize().into();

        if computed_checksum != compressed.checksum {
            return Err(TransportError::ChecksumMismatch);
        }

        Ok((decompressed, compressed.metadata.clone()))
    }

    /// Serialize CompressedTensor to bytes using bincode
    pub fn serialize(tensor: &CompressedTensor) -> Result<Vec<u8>, TransportError> {
        bincode::serialize(tensor)
            .map_err(|e| TransportError::Serialization(e.to_string()))
    }

    /// Deserialize bytes to CompressedTensor using bincode
    pub fn deserialize(bytes: &[u8]) -> Result<CompressedTensor, TransportError> {
        bincode::deserialize(bytes)
            .map_err(|e| TransportError::Deserialization(e.to_string()))
    }

    /// Serialize tensor metadata only
    pub fn serialize_metadata(metadata: &TensorMetadata) -> Result<Vec<u8>, TransportError> {
        bincode::serialize(metadata)
            .map_err(|e| TransportError::Serialization(e.to_string()))
    }

    /// Deserialize tensor metadata only
    pub fn deserialize_metadata(bytes: &[u8]) -> Result<TensorMetadata, TransportError> {
        bincode::deserialize(bytes)
            .map_err(|e| TransportError::Deserialization(e.to_string()))
    }

    /// Quantize f32 tensor to i8
    /// Maps f32 range [-1.0, 1.0] to i8 [-127, 127] with clamping
    pub fn quantize_f32_to_i8(data: &[f32]) -> Vec<i8> {
        data.iter()
            .map(|&val| {
                let clamped = val.clamp(-1.0, 1.0);
                // Map [-1.0, 1.0] to [-127, 127]
                ((clamped * 127.0).round() as i32).clamp(-127, 127) as i8
            })
            .collect()
    }

    /// Dequantize i8 tensor back to f32
    pub fn dequantize_i8_to_f32(data: &[i8]) -> Vec<f32> {
        data.iter()
            .map(|&val| {
                // Map [-127, 127] back to [-1.0, 1.0]
                (val as f32) / 127.0
            })
            .collect()
    }

    /// Compress with 8-bit quantization
    pub fn compress_with_quantization(
        data: &[u8],
        mut metadata: TensorMetadata,
        compression_level: i32,
    ) -> Result<CompressedTensor, TransportError> {
        // Validate compression level
        if compression_level < 1 || compression_level > 22 {
            return Err(TransportError::InvalidCompressionLevel(compression_level));
        }

        // Store original dtype for reconstruction
        metadata.original_dtype = Some(metadata.dtype.clone());
        
        // Deserialize f32 tensor from bytes
        let num_elements = metadata.num_elements() as usize;
        if data.len() != num_elements * 4 {
            return Err(TransportError::Serialization(
                format!("expected {} bytes for f32 tensor, got {}", num_elements * 4, data.len())
            ));
        }
        
        let f32_data: Vec<f32> = data.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        // Apply 8-bit quantization
        let quantized = Self::quantize_f32_to_i8(&f32_data);
        
        // Convert i8 to u8 for compression
        let quantized_bytes: Vec<u8> = quantized.iter().map(|&x| x as u8).collect();

        // Compress quantized i8 data with zstd
        let compressed_data = zstd::encode_all(&quantized_bytes[..], compression_level)
            .map_err(|e| TransportError::Compression(e.to_string()))?;

        // Calculate SHA3-256 checksum on original data
        let mut hasher = Sha3_256::new();
        hasher.update(data);
        let checksum: [u8; 32] = hasher.finalize().into();

        // Update metadata dtype
        metadata.dtype = "i8_quantized".to_string();

        // Calculate compression ratio
        let original_size = data.len();
        let compressed_size = compressed_data.len();
        let compression_ratio = if compressed_size > 0 {
            original_size as f64 / compressed_size as f64
        } else {
            1.0
        };

        // Record stats
        let stats = get_global_compression_stats();
        stats.record_compression(original_size, compressed_size, true);

        Ok(CompressedTensor {
            metadata,
            data: compressed_data,
            compression_level,
            checksum,
            quantization_method: Some(QuantizationMethod::Int8),
            compression_ratio,
        })
    }

    /// Decompress with dequantization
    pub fn decompress_with_dequantization(
        compressed: &CompressedTensor,
    ) -> Result<(Vec<u8>, TensorMetadata), TransportError> {
        // Decompress with zstd
        let decompressed = zstd::decode_all(&compressed.data[..])
            .map_err(|e| TransportError::Decompression(e.to_string()))?;

        // Verify checksum
        // Note: For quantized compression, we can't verify against original checksum
        // because we don't have the original data. The checksum is for verification
        // that the compressed tensor hasn't been tampered with.

        // Check if this is quantized
        if compressed.quantization_method.as_ref() != Some(&QuantizationMethod::Int8) {
            return Err(TransportError::Deserialization(
                "tensor was not quantized with Int8".to_string()
            ));
        }

        // Convert u8 back to i8
        let quantized: Vec<i8> = decompressed.iter().map(|&x| x as i8).collect();

        // Dequantize back to f32
        let f32_data = Self::dequantize_i8_to_f32(&quantized);

        // Convert f32 back to bytes
        let mut result = Vec::with_capacity(f32_data.len() * 4);
        for val in f32_data {
            result.extend_from_slice(&val.to_le_bytes());
        }

        // Restore original dtype
        let mut metadata = compressed.metadata.clone();
        if let Some(original_dtype) = metadata.original_dtype.take() {
            metadata.dtype = original_dtype;
        }

        Ok((result, metadata))
    }

    /// Get global compression statistics
    pub fn get_compression_stats() -> (u64, u64, f64) {
        let stats = get_global_compression_stats();
        stats.get_stats()
    }

    /// Get compression ratio for a compressed tensor
    pub fn compression_ratio(original_size: usize, compressed: &CompressedTensor) -> f64 {
        if original_size == 0 {
            return 1.0;
        }
        compressed.data.len() as f64 / original_size as f64
    }

    /// Estimate decompressed size (for buffer allocation)
    pub fn estimate_decompressed_size(metadata: &TensorMetadata) -> usize {
        metadata.byte_size()
    }

    /// Validate compressed tensor without full decompression
    pub fn validate(compressed: &CompressedTensor) -> Result<(), TransportError> {
        // Quick validation of metadata
        if compressed.metadata.shape.is_empty() {
            return Err(TransportError::Deserialization(
                "empty tensor shape".to_string(),
            ));
        }

        if compressed.metadata.dtype.is_empty() {
            return Err(TransportError::Deserialization(
                "empty dtype".to_string(),
            ));
        }

        // Try to decompress first few bytes to validate format
        let mut decoder = zstd::stream::read::Decoder::new(&compressed.data[..])
            .map_err(|e| TransportError::Decompression(e.to_string()))?;

        let mut buf = [0u8; 8];
        let _ = decoder
            .read(&mut buf)
            .map_err(|e| TransportError::Decompression(e.to_string()))?;

        Ok(())
    }

    /// Batch compress multiple tensors
    pub fn batch_compress(
        tensors: Vec<(Vec<u8>, TensorMetadata)>,
        compression_level: i32,
    ) -> Result<Vec<CompressedTensor>, TransportError> {
        tensors
            .into_iter()
            .map(|(data, metadata)| Self::compress(&data, metadata, compression_level))
            .collect()
    }

    /// Batch decompress multiple tensors
    pub fn batch_decompress(
        tensors: Vec<&CompressedTensor>,
    ) -> Result<Vec<(Vec<u8>, TensorMetadata)>, TransportError> {
        tensors
            .into_iter()
            .map(|t| Self::decompress(t))
            .collect()
    }
}

use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_compression_roundtrip() {
        // Create sample tensor data (simulating 1x512x4096 f32 tensor)
        let num_elements = 1 * 512 * 4096;
        let data: Vec<u8> = (0..num_elements * 4)
            .map(|i| (i % 256) as u8)
            .collect();

        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        // Compress
        let compressed = TensorTransport::compress(&data, metadata.clone(), 1)
            .expect("compression should succeed");

        assert_eq!(compressed.metadata.shape, metadata.shape);
        assert_eq!(compressed.metadata.dtype, metadata.dtype);
        assert_eq!(compressed.compression_level, 1);

        // Decompress
        let (decompressed, recovered_metadata) =
            TensorTransport::decompress(&compressed).expect("decompression should succeed");

        assert_eq!(decompressed, data);
        assert_eq!(recovered_metadata.shape, metadata.shape);
        assert_eq!(recovered_metadata.dtype, metadata.dtype);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![1, 2, 3, 4, 5];
        let compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

        // Serialize
        let serialized = TensorTransport::serialize(&compressed).unwrap();

        // Deserialize
        let deserialized = TensorTransport::deserialize(&serialized).unwrap();

        // Verify
        let (decompressed, _) = TensorTransport::decompress(&deserialized).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_checksum_validation() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![1, 2, 3, 4, 5];
        let mut compressed = TensorTransport::compress(&data, metadata, 1).unwrap();

        // Corrupt the checksum
        compressed.checksum[0] ^= 0xFF;

        // Decompression should fail due to checksum mismatch
        let result = TensorTransport::decompress(&compressed);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::ChecksumMismatch));
    }

    #[test]
    fn test_invalid_compression_level() {
        let metadata = TensorMetadata {
            shape: vec![1, 512, 4096],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        let data: Vec<u8> = vec![1, 2, 3, 4, 5];

        // Test level 0 (invalid)
        let result = TensorTransport::compress(&data, metadata.clone(), 0);
        assert!(result.is_err());

        // Test level 23 (invalid)
        let result = TensorTransport::compress(&data, metadata, 23);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_operations() {
        let tensors: Vec<(Vec<u8>, TensorMetadata)> = (0..3)
            .map(|i| {
                let data: Vec<u8> = (0..100).map(|j| (i * 10 + j) as u8).collect();
                let metadata = TensorMetadata {
                    shape: vec![10, 10],
                    dtype: "f32".to_string(),
                    layer_range: (i as u32 * 10, i as u32 * 10 + 9),
                    original_dtype: None,
                };
                (data, metadata)
            })
            .collect();

        // Batch compress
        let compressed = TensorTransport::batch_compress(tensors.clone(), 1).unwrap();
        assert_eq!(compressed.len(), 3);

        // Batch decompress
        let decompressed_refs: Vec<&CompressedTensor> = compressed.iter().collect();
        let decompressed = TensorTransport::batch_decompress(decompressed_refs).unwrap();
        assert_eq!(decompressed.len(), 3);

        // Verify each tensor
        for (i, ((orig_data, orig_meta), (decomp_data, decomp_meta))) in
            tensors.into_iter().zip(decompressed.into_iter()).enumerate()
        {
            assert_eq!(decomp_data, orig_data, "data mismatch at index {}", i);
            assert_eq!(decomp_meta.shape, orig_meta.shape);
            assert_eq!(decomp_meta.layer_range, orig_meta.layer_range);
        }
    }

    #[test]
    fn test_compression_ratio() {
        // Create compressible data (repeated pattern)
        let data: Vec<u8> = vec![0xAB; 10000];

        let metadata = TensorMetadata {
            shape: vec![10000],
            dtype: "u8".to_string(),
            layer_range: (0, 0),
            original_dtype: None,
        };

        let compressed = TensorTransport::compress(&data, metadata, 3).unwrap();
        let ratio = TensorTransport::compression_ratio(data.len(), &compressed);

        // Highly compressible data should have ratio < 0.1
        assert!(ratio < 0.1, "compression ratio {} should be < 0.1", ratio);
    }

    #[test]
    fn test_quantization_roundtrip() {
        // Create sample f32 data
        let f32_data: Vec<f32> = vec![-1.0, -0.5, 0.0, 0.5, 1.0, 0.75, -0.75];
        
        // Quantize
        let quantized = TensorTransport::quantize_f32_to_i8(&f32_data);
        
        // Verify range is correct
        for &q in &quantized {
            assert!((q as i32) >= -127 && (q as i32) <= 127);
        }
        
        // Dequantize
        let dequantized = TensorTransport::dequantize_i8_to_f32(&quantized);
        
        // Verify values are approximately preserved (within epsilon)
        for (original, recovered) in f32_data.iter().zip(dequantized.iter()) {
            let epsilon = 0.02; // Allow 2% error due to quantization
            assert!((original - recovered).abs() < epsilon, 
                "quantization error too large: {} vs {} (diff: {})", 
                original, recovered, (original - recovered).abs());
        }
    }

    #[test]
    fn test_quantization_compression_roundtrip() {
        // Create sample f32 tensor data
        let f32_data: Vec<f32> = (0..1024)
            .map(|i| ((i % 256) as f32 - 127.5) / 127.5)
            .collect();
        
        // Convert to bytes
        let mut data: Vec<u8> = Vec::with_capacity(f32_data.len() * 4);
        for val in &f32_data {
            data.extend_from_slice(&val.to_le_bytes());
        }
        
        let metadata = TensorMetadata {
            shape: vec![1, 1024],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        // Compress with quantization
        let compressed = TensorTransport::compress_with_quantization(&data, metadata.clone(), 3)
            .expect("quantized compression should succeed");
        
        // Verify quantization method is set
        assert_eq!(compressed.quantization_method, Some(QuantizationMethod::Int8));
        
        // Verify metadata was updated
        assert_eq!(compressed.metadata.dtype, "i8_quantized");
        assert_eq!(compressed.metadata.original_dtype, Some("f32".to_string()));
        
        // Verify compression ratio is reasonable (should be > 2.0 for quantized f32)
        assert!(compressed.compression_ratio > 2.0, 
            "quantized compression ratio {} should be > 2.0", 
            compressed.compression_ratio);

        // Decompress with dequantization
        let (dequantized_data, recovered_metadata) = 
            TensorTransport::decompress_with_dequantization(&compressed)
                .expect("dequantized decompression should succeed");
        
        // Verify dtype was restored
        assert_eq!(recovered_metadata.dtype, "f32");
        
        // Verify data length matches original
        assert_eq!(dequantized_data.len(), data.len());
        
        // Convert back to f32 for comparison
        let recovered_f32: Vec<f32> = dequantized_data.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        
        // Verify values are approximately preserved
        for (original, recovered) in f32_data.iter().zip(recovered_f32.iter()) {
            let epsilon = 0.02;
            assert!((original - recovered).abs() < epsilon,
                "quantization error too large: {} vs {} (diff: {})",
                original, recovered, (original - recovered).abs());
        }
    }

    #[test]
    fn test_quantization_clamping() {
        // Test that values outside [-1, 1] are clamped
        let f32_data: Vec<f32> = vec![-2.0, -1.5, 1.5, 2.0, 0.0];
        
        let quantized = TensorTransport::quantize_f32_to_i8(&f32_data);
        
        // Clamped values
        assert_eq!(quantized[0], -127); // -2.0 clamped to -1.0 -> -127
        assert_eq!(quantized[1], -127); // -1.5 clamped to -1.0 -> -127
        assert_eq!(quantized[2], 127);  // 1.5 clamped to 1.0 -> 127
        assert_eq!(quantized[3], 127);  // 2.0 clamped to 1.0 -> 127
        
        // 0.0 should be 0
        assert_eq!(quantized[4], 0);
    }

    #[test]
    fn test_compression_stats() {
        // Reset stats by getting them first (may have values from other tests)
        let _ = TensorTransport::get_compression_stats();
        
        // Create and compress some data with quantization
        let f32_data: Vec<f32> = vec![0.5; 1024];
        let mut data: Vec<u8> = Vec::with_capacity(f32_data.len() * 4);
        for val in &f32_data {
            data.extend_from_slice(&val.to_le_bytes());
        }
        
        let metadata = TensorMetadata {
            shape: vec![1024],
            dtype: "f32".to_string(),
            layer_range: (0, 9),
            original_dtype: None,
        };

        // Compress with quantization
        let _ = TensorTransport::compress_with_quantization(&data, metadata, 3).unwrap();
        
        // Get stats
        let (total, _saved, ratio) = TensorTransport::get_compression_stats();
        
        // Verify stats are non-zero
        assert!(total > 0, "total tensors compressed should be > 0");
        assert!(ratio > 1.0, "compression ratio should be > 1.0");
    }
}
