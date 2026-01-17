use std::io;

use async_trait::async_trait;
use libp2p::request_response;
use serde::{Deserialize, Serialize};

use consensus::CompressionMethod;
use futures::prelude::*;

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

#[derive(Clone, Default)]
pub struct TensorStreamCodec;

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

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, 8 * 1024 * 1024).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn write_request<T>(&mut self, _: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data).await?;
        io.flush().await
    }

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, resp: Self::Response) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&resp)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data).await?;
        io.flush().await
    }
}

pub const TENSOR_CHUNK_SIZE_BYTES: usize = 1024 * 1024;

pub fn protocol() -> String {
    "/aigen/tensor-stream/1.0.0".to_string()
}

async fn read_frame<T>(io: &mut T, max_len: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_len {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut data = vec![0u8; len];
    io.read_exact(&mut data).await?;
    Ok(data)
}

async fn write_frame<T>(io: &mut T, data: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    let len = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    io.write_all(&len.to_be_bytes()).await?;
    io.write_all(data).await?;
    Ok(())
}
