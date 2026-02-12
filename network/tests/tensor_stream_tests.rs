// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Tensor streaming tests for activation transmission

use network::tensor_stream::{
    ActivationTensor, ActivationChunk, compress_activation, decompress_activation,
    chunk_activation, reconstruct_activation, ACTIVATION_CHUNK_SIZE, DEFAULT_COMPRESSION_LEVEL,
    ActivationStreamError,
};
use uuid::Uuid;

#[test]
fn test_activation_tensor_creation() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 768i64];
    let data = vec![0.5f32; 768];

    let tensor = ActivationTensor::new(
        task_id,
        inference_id,
        layer_range,
        shape.clone(),
        data.clone(),
    );

    assert_eq!(tensor.task_id, task_id);
    assert_eq!(tensor.inference_id, inference_id);
    assert_eq!(tensor.layer_range, layer_range);
    assert_eq!(tensor.shape, shape);
    assert_eq!(tensor.data, data);
    assert!(tensor.verify_integrity());
}

#[test]
fn test_activation_tensor_hash() {
    let data = vec![1.0f32, 2.0f32, 3.0f32, 4.0f32];
    let hash = ActivationTensor::compute_hash(&data);
    
    assert_eq!(hash.len(), 32);
    
    // Same data should produce same hash
    let hash2 = ActivationTensor::compute_hash(&data);
    assert_eq!(hash, hash2);
}

#[test]
fn test_activation_tensor_integrity() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 4i64];
    let data = vec![0.1f32, 0.2f32, 0.3f32, 0.4f32];

    let mut tensor = ActivationTensor::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    assert!(tensor.verify_integrity());

    // Corrupt the data
    tensor.data[0] = 999.0f32;
    assert!(!tensor.verify_integrity());
}

#[test]
fn test_activation_compression() {
    let data = vec![1.0f32, 2.0f32, 3.0f32, 4.0f32, 5.0f32];
    
    let compressed = compress_activation(&data, 1).unwrap();
    let decompressed = decompress_activation(&compressed, data.len()).unwrap();
    
    assert_eq!(data, decompressed);
}

#[test]
fn test_activation_compression_large() {
    // Test with larger data
    let data: Vec<f32> = (0..10000).map(|i| i as f32 * 0.001).collect();
    
    let compressed = compress_activation(&data, 1).unwrap();
    let decompressed = decompress_activation(&compressed, data.len()).unwrap();
    
    assert_eq!(data, decompressed);
    
    // Verify compression actually reduces size for repetitive data
    let repetitive = vec![1.0f32; 10000];
    let compressed_rep = compress_activation(&repetitive, 1).unwrap();
    assert!(compressed_rep.len() < repetitive.len() * std::mem::size_of::<f32>());
}

#[test]
fn test_activation_chunking() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 1024i64];
    let data = vec![0.5f32; 1024];

    let tensor = ActivationTensor::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    // Chunk with small chunk size for testing
    let chunks = chunk_activation(&tensor, 1024, 1).unwrap();
    
    assert!(!chunks.is_empty());
    assert_eq!(chunks[0].task_id, task_id);
    assert_eq!(chunks[0].inference_id, inference_id);
    assert_eq!(chunks[0].layer_range, layer_range);
    assert_eq!(chunks[0].checkpoint_hash, tensor.checkpoint_hash);
    
    // Verify all chunks have correct total_chunks
    let total = chunks[0].total_chunks;
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i as u32);
        assert_eq!(chunk.total_chunks, total);
    }
}

#[test]
fn test_activation_reconstruction() {
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 512i64];
    let data: Vec<f32> = (0..512).map(|i| i as f32 * 0.01).collect();

    let original = ActivationTensor::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    // Chunk and reconstruct
    let chunks = chunk_activation(&original, 1024, 1).unwrap();
    let reconstructed = reconstruct_activation(&chunks).unwrap();

    assert_eq!(reconstructed.task_id, original.task_id);
    assert_eq!(reconstructed.inference_id, original.inference_id);
    assert_eq!(reconstructed.layer_range, original.layer_range);
    assert_eq!(reconstructed.shape, original.shape);
    assert_eq!(reconstructed.data, original.data);
    assert_eq!(reconstructed.checkpoint_hash, original.checkpoint_hash);
    assert!(reconstructed.verify_integrity());
}

#[test]
fn test_activation_reconstruction_empty_chunks() {
    let result = reconstruct_activation(&[]);
    assert!(matches!(result, Err(ActivationStreamError::NoChunks)));
}

#[test]
fn test_activation_reconstruction_mismatched_chunks() {
    let task_id1 = Uuid::new_v4();
    let task_id2 = Uuid::new_v4();
    let inference_id = Uuid::new_v4();

    let chunk1 = ActivationChunk {
        task_id: task_id1,
        inference_id,
        chunk_index: 0,
        total_chunks: 2,
        data: vec![1, 2, 3],
        compression_level: 1,
        checkpoint_hash: [0u8; 32],
        shape: vec![1, 3],
        layer_range: (0, 10),
    };

    let chunk2 = ActivationChunk {
        task_id: task_id2, // Different task_id
        inference_id,
        chunk_index: 1,
        total_chunks: 2,
        data: vec![4, 5, 6],
        compression_level: 1,
        checkpoint_hash: [0u8; 32],
        shape: vec![1, 3],
        layer_range: (0, 10),
    };

    let result = reconstruct_activation(&[chunk1, chunk2]);
    assert!(matches!(result, Err(ActivationStreamError::MismatchedChunks)));
}

#[test]
fn test_compression_efficiency() {
    // Measure compression ratio for activation data
    let data = vec![0.5f32; 10000];
    
    let original_size = data.len() * std::mem::size_of::<f32>();
    let compressed = compress_activation(&data, 1).unwrap();
    let compressed_size = compressed.len();
    
    let ratio = compressed_size as f32 / original_size as f32;
    println!("Compression ratio: {:.2}% ({} -> {} bytes)", 
        ratio * 100.0, original_size, compressed_size);
    
    // For repetitive data, should achieve good compression
    assert!(ratio < 0.5, "Compression ratio should be < 50% for repetitive data");
}

#[tokio::test]
async fn test_activation_stream_send_receive() {
    use network::tensor_stream::ActivationStream;
    
    let stream = ActivationStream::new(100);
    
    let task_id = Uuid::new_v4();
    let inference_id = Uuid::new_v4();
    let layer_range = (0u32, 10u32);
    let shape = vec![1i64, 256i64];
    let data = vec![0.5f32; 256];

    let tensor = ActivationTensor::new(
        task_id,
        inference_id,
        layer_range,
        shape,
        data,
    );

    // Send activation
    let num_chunks = stream.send_activation(&tensor, 1).await.unwrap();
    assert!(num_chunks > 0);

    // Receive activation
    let received = stream.receive_activation(task_id, 5000).await.unwrap();
    
    assert_eq!(received.task_id, tensor.task_id);
    assert_eq!(received.inference_id, tensor.inference_id);
    assert_eq!(received.data, tensor.data);
    assert!(received.verify_integrity());
}

#[tokio::test]
async fn test_activation_stream_timeout() {
    use network::tensor_stream::ActivationStream;
    
    let stream = ActivationStream::new(100);
    let task_id = Uuid::new_v4();

    // Try to receive non-existent activation with short timeout
    let result = stream.receive_activation(task_id, 100).await;
    assert!(matches!(result, Err(ActivationStreamError::Timeout)));
}

#[test]
fn test_activation_chunk_size() {
    // Verify default chunk size is 4MB
    assert_eq!(ACTIVATION_CHUNK_SIZE, 4 * 1024 * 1024);
}

#[test]
fn test_default_compression_level() {
    // Verify default compression level is 1 for low latency
    assert_eq!(DEFAULT_COMPRESSION_LEVEL, 1);
}

#[test]
fn test_activation_stream_error_display() {
    let err = ActivationStreamError::Compression("zstd failed".to_string());
    assert!(err.to_string().contains("zstd failed"));

    let err = ActivationStreamError::SizeMismatch { expected: 100, actual: 50 };
    assert!(err.to_string().contains("expected 100 bytes, got 50"));

    let err = ActivationStreamError::MissingChunks { expected: 5, actual: 3 };
    assert!(err.to_string().contains("expected 5, got 3"));
}
