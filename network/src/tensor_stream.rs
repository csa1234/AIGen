// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Activation streaming protocol for distributed inference.
//!
//! Handles serialization, compression, and P2P transmission of intermediate
//! activations between pipeline nodes during fragment-aware execution.

use std::io;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;

use consensus::CompressionMethod;
use crate::protocol::NetworkMessage;

/// Default chunk size for activation streaming (4MB)
pub const ACTIVATION_CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Default zstd compression level (1 for low latency)
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 1;

/// Maximum activation tensor size (1GB)
pub const MAX_ACTIVATION_SIZE: usize = 1024 * 1024 * 1024;

/// Activation tensor representing intermediate inference results
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActivationTensor {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub layer_range: (u32, u32),
    pub shape: Vec<i64>,
    pub data: Vec<f32>,
    /// SHA3-256 hash of the data for integrity verification
    pub checkpoint_hash: [u8; 32],
    pub timestamp: i64,
}

impl ActivationTensor {
    /// Create a new activation tensor with automatic checkpoint hash generation
    pub fn new(
        task_id: Uuid,
        inference_id: Uuid,
        layer_range: (u32, u32),
        shape: Vec<i64>,
        data: Vec<f32>,
    ) -> Self {
        let checkpoint_hash = Self::compute_hash(&data);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Self {
            task_id,
            inference_id,
            layer_range,
            shape,
            data,
            checkpoint_hash,
            timestamp,
        }
    }

    /// Compute SHA3-256 hash of tensor data
    pub fn compute_hash(data: &[f32]) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        
        // Convert f32 slice to bytes for hashing
        let bytes: &[u8] = bytemuck::cast_slice(data);
        hasher.update(bytes);
        
        hasher.finalize().into()
    }

    /// Verify data integrity against stored checkpoint hash
    pub fn verify_integrity(&self) -> bool {
        let computed_hash = Self::compute_hash(&self.data);
        computed_hash == self.checkpoint_hash
    }

    /// Get total number of elements in the tensor
    pub fn num_elements(&self) -> usize {
        self.data.len()
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> usize {
        self.data.len() * std::mem::size_of::<f32>()
    }
}

/// Compressed activation chunk for network transmission with optional quantization
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationChunk {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>, // compressed (and optionally quantized)
    pub compression_level: i32,
    pub checkpoint_hash: [u8; 32],
    pub shape: Vec<i64>,
    pub layer_range: (u32, u32),
    // NEW: Quantization metadata for 8-bit compression
    pub quantization_min: f32,
    pub quantization_max: f32,
    pub is_quantized: bool,
}

/// Quantize f32 data to u8 using linear scaling
/// Returns (quantized_data, min, max) for dequantization
pub fn quantize_f32_to_u8(data: &[f32]) -> (Vec<u8>, f32, f32) {
    if data.is_empty() {
        return (Vec::new(), 0.0, 0.0);
    }

    // Find min and max
    let min = data.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // Handle edge case where all values are the same
    if min == max {
        return (vec![0u8; data.len()], min, max);
    }

    // Quantize: u8 = ((f32 - min) / (max - min)) * 255
    let scale = 255.0 / (max - min);
    let quantized: Vec<u8> = data
        .iter()
        .map(|&v| {
            let normalized = (v - min) * scale;
            normalized.clamp(0.0, 255.0) as u8
        })
        .collect();

    (quantized, min, max)
}

/// Dequantize u8 data back to f32 using stored min/max
pub fn dequantize_u8_to_f32(data: &[u8], min: f32, max: f32) -> Vec<f32> {
    if data.is_empty() || min == max {
        return vec![min; data.len()];
    }

    let scale = (max - min) / 255.0;
    data.iter()
        .map(|&v| (v as f32) * scale + min)
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationRequest {
    pub task_id: Uuid,
    pub inference_id: Uuid,
    pub chunk_index: u32,
}

/// Response containing activation chunk
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivationResponse {
    pub chunk: ActivationChunk,
}

/// Stream codec for activation streaming protocol
#[derive(Clone, Default)]
pub struct ActivationStreamCodec;

const MAX_REQUEST_BYTES: u64 = 1024 * 1024; // 1MB for requests
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024; // 8MB for responses

#[async_trait]
impl request_response::Codec for ActivationStreamCodec {
    type Protocol = String;
    type Request = ActivationRequest;
    type Response = ActivationResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, MAX_REQUEST_BYTES).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, MAX_RESPONSE_BYTES).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, MAX_REQUEST_BYTES).await?;
        io.flush().await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&resp)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, MAX_RESPONSE_BYTES).await?;
        io.flush().await
    }
}

/// Protocol string for activation streaming
pub fn activation_protocol() -> String {
    "/aigen/activation-stream/1.0.0".to_string()
}

/// Compress activation data using zstd (legacy, non-quantized)
pub fn compress_activation(data: &[f32], level: i32) -> Result<Vec<u8>, ActivationStreamError> {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    zstd::encode_all(bytes, level)
        .map_err(|e| ActivationStreamError::Compression(e.to_string()))
}

/// Decompress activation data (legacy, non-quantized)
pub fn decompress_activation(compressed: &[u8], expected_elements: usize) -> Result<Vec<f32>, ActivationStreamError> {
    let decompressed = zstd::decode_all(compressed)
        .map_err(|e| ActivationStreamError::Decompression(e.to_string()))?;
    
    if decompressed.len() != expected_elements * std::mem::size_of::<f32>() {
        return Err(ActivationStreamError::SizeMismatch {
            expected: expected_elements * std::mem::size_of::<f32>(),
            actual: decompressed.len(),
        });
    }
    
    let floats: &[f32] = bytemuck::cast_slice(&decompressed);
    Ok(floats.to_vec())
}

/// Compress activation data using 8-bit quantization followed by zstd
/// This provides ~75% bandwidth reduction (f32->u8 = 4x, plus compression)
pub fn compress_activation_quantized(data: &[f32], level: i32) -> Result<(Vec<u8>, f32, f32), ActivationStreamError> {
    // Step 1: Quantize f32 -> u8 (4x size reduction)
    let (quantized, min, max) = quantize_f32_to_u8(data);

    // Step 2: Apply zstd compression on u8 data
    let compressed = zstd::encode_all(&quantized, level)
        .map_err(|e| ActivationStreamError::Compression(e.to_string()))?;

    Ok((compressed, min, max))
}

/// Decompress activation data with 8-bit quantization
pub fn decompress_activation_quantized(
    compressed: &[u8],
    min: f32,
    max: f32,
    expected_elements: usize,
) -> Result<Vec<f32>, ActivationStreamError> {
    // Step 1: Decompress zstd
    let decompressed = zstd::decode_all(compressed)
        .map_err(|e| ActivationStreamError::Decompression(e.to_string()))?;

    // Step 2: Verify size
    if decompressed.len() != expected_elements {
        return Err(ActivationStreamError::SizeMismatch {
            expected: expected_elements,
            actual: decompressed.len(),
        });
    }

    // Step 3: Dequantize u8 -> f32
    Ok(dequantize_u8_to_f32(&decompressed, min, max))
}

/// Split activation tensor into chunks for streaming using quantization + compression
pub fn chunk_activation(
    activation: &ActivationTensor,
    chunk_size: usize,
    compression_level: i32,
) -> Result<Vec<ActivationChunk>, ActivationStreamError> {
    // Use quantized compression for better bandwidth efficiency
    let (compressed, min, max) = compress_activation_quantized(&activation.data, compression_level)?;
    let compressed_total = compressed.len();
    let num_chunks = (compressed_total + chunk_size - 1) / chunk_size;

    if num_chunks > u32::MAX as usize {
        return Err(ActivationStreamError::TooManyChunks(num_chunks));
    }

    let compressed_chunk_size = (compressed_total + num_chunks - 1) / num_chunks;

    let mut chunks = Vec::with_capacity(num_chunks);

    for i in 0..num_chunks {
        let start = i * compressed_chunk_size;
        let end = ((i + 1) * compressed_chunk_size).min(compressed_total);
        let chunk_data = compressed[start..end].to_vec();

        chunks.push(ActivationChunk {
            task_id: activation.task_id,
            inference_id: activation.inference_id,
            chunk_index: i as u32,
            total_chunks: num_chunks as u32,
            data: chunk_data,
            compression_level,
            checkpoint_hash: activation.checkpoint_hash,
            shape: activation.shape.clone(),
            layer_range: activation.layer_range,
            // NEW: Quantization metadata
            quantization_min: min,
            quantization_max: max,
            is_quantized: true,
        });
    }

    Ok(chunks)
}

/// Reconstruct activation tensor from chunks (supports both quantized and non-quantized)
pub fn reconstruct_activation(chunks: &[ActivationChunk]) -> Result<ActivationTensor, ActivationStreamError> {
    if chunks.is_empty() {
        return Err(ActivationStreamError::NoChunks);
    }

    // Verify all chunks belong to same activation
    let first = &chunks[0];
    for chunk in chunks.iter().skip(1) {
        if chunk.task_id != first.task_id || chunk.inference_id != first.inference_id {
            return Err(ActivationStreamError::MismatchedChunks);
        }
        if chunk.checkpoint_hash != first.checkpoint_hash {
            return Err(ActivationStreamError::HashMismatch);
        }
    }

    // Sort chunks by index
    let mut sorted_chunks = chunks.to_vec();
    sorted_chunks.sort_by_key(|c| c.chunk_index);

    // Verify we have all chunks
    let total_chunks = first.total_chunks as usize;
    if sorted_chunks.len() != total_chunks {
        return Err(ActivationStreamError::MissingChunks {
            expected: total_chunks,
            actual: sorted_chunks.len(),
        });
    }

    // Concatenate compressed data
    let mut compressed = Vec::new();
    for chunk in &sorted_chunks {
        compressed.extend_from_slice(&chunk.data);
    }

    // Calculate expected elements from shape
    let expected_elements: i64 = first.shape.iter().product();
    let expected_elements = expected_elements as usize;

    // Decompress (with or without quantization)
    let data = if first.is_quantized {
        decompress_activation_quantized(&compressed, first.quantization_min, first.quantization_max, expected_elements)?
    } else {
        decompress_activation(&compressed, expected_elements)?
    };

    // Verify integrity
    let computed_hash = ActivationTensor::compute_hash(&data);
    if computed_hash != first.checkpoint_hash {
        return Err(ActivationStreamError::IntegrityCheckFailed);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(ActivationTensor {
        task_id: first.task_id,
        inference_id: first.inference_id,
        layer_range: first.layer_range,
        shape: first.shape.clone(),
        data,
        checkpoint_hash: first.checkpoint_hash,
        timestamp,
    })
}

/// Activation streaming service for managing send/receive operations
#[derive(Clone)]
pub struct ActivationStream {
    pending_activations: Arc<RwLock<std::collections::HashMap<Uuid, Vec<ActivationChunk>>>>,
    completed_activations: Arc<RwLock<std::collections::HashMap<Uuid, ActivationTensor>>>,
    sender: mpsc::Sender<ActivationChunk>,
    receiver: Arc<Mutex<mpsc::Receiver<ActivationChunk>>>,
}

impl ActivationStream {
    /// Create a new activation stream service
    pub fn new(buffer_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buffer_size);
        Self {
            pending_activations: Arc::new(RwLock::new(std::collections::HashMap::new())),
            completed_activations: Arc::new(RwLock::new(std::collections::HashMap::new())),
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    /// Send an activation tensor by chunking and streaming
    pub async fn send_activation(
        &self,
        activation: &ActivationTensor,
        compression_level: i32,
    ) -> Result<u32, ActivationStreamError> {
        let chunks = chunk_activation(activation, ACTIVATION_CHUNK_SIZE, compression_level)?;
        let num_chunks = chunks.len() as u32;

        for chunk in chunks {
            self.sender.send(chunk).await
                .map_err(|_| ActivationStreamError::SendFailed)?;
        }

        Ok(num_chunks)
    }

    /// Receive an activation tensor by collecting chunks
    pub async fn receive_activation(
        &self,
        task_id: Uuid,
        timeout_ms: u64,
    ) -> Result<ActivationTensor, ActivationStreamError> {
        let start = Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            if start.elapsed() > timeout {
                return Err(ActivationStreamError::Timeout);
            }

            // Check if already completed
            if let Some(activation) = self.completed_activations.read().await.get(&task_id) {
                return Ok(activation.clone());
            }

            // Try to receive chunks
            match tokio::time::timeout(std::time::Duration::from_millis(100), self.receiver.lock().await.recv()).await {
                Ok(Some(chunk)) if chunk.task_id == task_id => {
                    let mut pending = self.pending_activations.write().await;
                    let entry = pending.entry(task_id).or_default();
                    entry.push(chunk);

                    // Check if we have all chunks
                    if let Some(first) = entry.first() {
                        if entry.len() == first.total_chunks as usize {
                            // Reconstruct
                            let chunks = pending.remove(&task_id).unwrap();
                            let activation = reconstruct_activation(&chunks)?;
                            self.completed_activations.write().await.insert(task_id, activation.clone());
                            return Ok(activation);
                        }
                    }
                }
                Ok(Some(_)) => {} // Different task, ignore
                Ok(None) => return Err(ActivationStreamError::ChannelClosed),
                Err(_) => {} // Timeout, continue loop
            }
        }
    }

    /// Get the sender channel for sending chunks
    pub fn sender(&self) -> mpsc::Sender<ActivationChunk> {
        self.sender.clone()
    }

    /// Cleanup completed activation
    pub async fn cleanup_activation(&self, task_id: Uuid) {
        self.completed_activations.write().await.remove(&task_id);
        self.pending_activations.write().await.remove(&task_id);
    }

    /// Enqueue an activation tensor directly (for reconstructed activations)
    pub async fn enqueue_activation(&self, activation: ActivationTensor) -> Result<(), ActivationStreamError> {
        self.completed_activations.write().await.insert(activation.task_id, activation);
        Ok(())
    }

    /// Forward chunks from the internal receiver to a network publisher
    /// This should be spawned as a task to continuously forward chunks
    pub async fn forward_to_network<F, Fut>(&self, mut publish_fn: F) -> Result<(), ActivationStreamError>
    where
        F: FnMut(NetworkMessage) -> Fut,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    {
        let receiver = self.receiver.clone();
        let mut rx = receiver.lock().await;
        
        while let Some(chunk) = rx.recv().await {
            let network_msg = NetworkMessage::ActivationChunk {
                task_id: chunk.task_id,
                inference_id: chunk.inference_id,
                chunk_index: chunk.chunk_index,
                total_chunks: chunk.total_chunks,
                data: chunk.data,
                compression_level: chunk.compression_level,
                checkpoint_hash: chunk.checkpoint_hash,
                shape: chunk.shape,
                layer_range: chunk.layer_range,
                // NEW: Include quantization metadata in network message
                quantization_min: chunk.quantization_min,
                quantization_max: chunk.quantization_max,
                is_quantized: chunk.is_quantized,
            };
            
            if let Err(_e) = publish_fn(network_msg).await {
                // Log error but continue processing other chunks
                eprintln!("failed to forward activation chunk to network: task_id={}", chunk.task_id);
            }
        }
        
        Ok(())
    }
}

/// Errors that can occur during activation streaming
#[derive(Debug)]
pub enum ActivationStreamError {
    Compression(String),
    Decompression(String),
    SizeMismatch { expected: usize, actual: usize },
    TooManyChunks(usize),
    NoChunks,
    MismatchedChunks,
    HashMismatch,
    MissingChunks { expected: usize, actual: usize },
    IntegrityCheckFailed,
    SendFailed,
    ReceiveFailed,
    ChannelClosed,
    Timeout,
    Io(std::io::Error),
}

impl std::fmt::Display for ActivationStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compression(e) => write!(f, "compression failed: {}", e),
            Self::Decompression(e) => write!(f, "decompression failed: {}", e),
            Self::SizeMismatch { expected, actual } => {
                write!(f, "size mismatch: expected {} bytes, got {}", expected, actual)
            }
            Self::TooManyChunks(n) => write!(f, "too many chunks: {}", n),
            Self::NoChunks => write!(f, "no chunks provided"),
            Self::MismatchedChunks => write!(f, "chunks belong to different activations"),
            Self::HashMismatch => write!(f, "chunk hash mismatch"),
            Self::MissingChunks { expected, actual } => {
                write!(f, "missing chunks: expected {}, got {}", expected, actual)
            }
            Self::IntegrityCheckFailed => write!(f, "integrity check failed"),
            Self::SendFailed => write!(f, "failed to send chunk"),
            Self::ReceiveFailed => write!(f, "failed to receive chunk"),
            Self::ChannelClosed => write!(f, "channel closed"),
            Self::Timeout => write!(f, "receive timeout"),
            Self::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

impl std::error::Error for ActivationStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ActivationStreamError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// Helper functions for frame reading/writing
async fn read_frame<T>(io: &mut T, max_len: u64) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len_buf = [0u8; 8];
    io.read_exact(&mut len_buf[..]).await?;
    let len = u64::from_be_bytes(len_buf);
    if len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let len_usize = usize::try_from(len)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    let mut data = vec![0u8; len_usize];
    io.read_exact(&mut data).await?;
    Ok(data)
}

async fn write_frame<T>(io: &mut T, data: &[u8], max_len: u64) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    let len = data.len() as u64;
    if len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    io.write_all(&len.to_be_bytes()).await?;
    io.write_all(data).await?;
    Ok(())
}

/// Legacy tensor streaming types (kept for backward compatibility)

/// Legacy tensor request/response types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensorRequest {
    pub poi_proof_hash: [u8; 32],
    pub chunk_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensorResponse {
    pub request_id: u64,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
    pub compression: CompressionMethod,
    pub original_size: u64,
}

/// Legacy tensor stream codec
#[derive(Clone, Default)]
pub struct TensorStreamCodec;

pub const TENSOR_CHUNK_SIZE_BYTES: usize = 64 * 1024; // 64KB chunk

#[async_trait]
impl request_response::Codec for TensorStreamCodec {
    type Protocol = String;
    type Request = TensorRequest;
    type Response = TensorResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, 4 * 1024 * 1024).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, 8 * 1024 * 1024).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, 4 * 1024 * 1024).await?;
        io.flush().await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&resp)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, 8 * 1024 * 1024).await?;
        io.flush().await
    }
}

/// Legacy protocol string
pub fn protocol() -> String {
    "/aigen/tensor-stream/1.0.0".to_string()
}
