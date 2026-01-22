use std::io;

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelShardRequest {
    pub model_id: String,
    pub shard_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelShardResponse {
    pub model_id: String,
    pub shard_index: u32,
    pub total_shards: u32,
    pub data: Vec<u8>,
    pub hash: [u8; 32],
    pub size: u64,
}

#[derive(Clone, Default)]
pub struct ModelStreamCodec;

const MAX_REQUEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 4_500_000_000;

#[async_trait]
impl request_response::Codec for ModelStreamCodec {
    type Protocol = String;
    type Request = ModelShardRequest;
    type Response = ModelShardResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, MAX_REQUEST_BYTES).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        let data = read_frame(io, MAX_RESPONSE_BYTES).await?;
        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn write_request<T>(&mut self, _: &Self::Protocol, io: &mut T, req: Self::Request) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, MAX_REQUEST_BYTES).await?;
        io.flush().await
    }

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, resp: Self::Response) -> io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&resp)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        write_frame(io, &data, MAX_RESPONSE_BYTES).await?;
        io.flush().await
    }
}

pub fn protocol() -> String {
    "/aigen/model-stream/1.0.0".to_string()
}

async fn read_frame<T>(io: &mut T, max_len: u64) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    let mut len_buf = [0u8; 8];
    io.read_exact(&mut len_buf[..]).await?;
    let len = u64::from_be_bytes(len_buf);
    if len > max_len {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
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
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    io.write_all(&len.to_be_bytes()).await?;
    io.write_all(data).await?;
    Ok(())
}
