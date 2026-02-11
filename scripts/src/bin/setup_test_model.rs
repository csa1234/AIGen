// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Setup script to generate a test ONNX identity model, split it into shards,
//! and write a manifest file that the node can use to register the model at startup.
//!
//! Usage:
//!   cargo run --bin setup-test-model -- --model-id mistral-7b --data-dir ./data
//!
//! This creates:
//!   <data-dir>/models/<model-id>/model.onnx       - The identity ONNX model
//!   <data-dir>/models/<model-id>/shards/           - Shard files
//!   <data-dir>/models/<model-id>/manifest.json     - Metadata for node registration

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use prost::Message;

use model::{ModelMetadata, ModelRegistry, ModelShard, ShardLocation};

#[derive(Parser)]
#[command(name = "setup-test-model")]
#[command(about = "Generate a test ONNX identity model for AIGEN node")]
struct Args {
    /// Model ID to use for registration
    #[arg(long, default_value = "mistral-7b")]
    model_id: String,

    /// Human-readable model name
    #[arg(long, default_value = "Mistral-7B-Test-Identity")]
    model_name: String,

    /// Data directory (models will be stored under <data-dir>/models/<model-id>/)
    #[arg(long, default_value = "./data")]
    data_dir: PathBuf,

    /// Model version
    #[arg(long, default_value = "1.0.0")]
    version: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    tokio::runtime::Runtime::new()?.block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let model_dir = args.data_dir.join("models").join(&args.model_id);
    let model_path = model_dir.join("model.onnx");
    let shard_dir = model_dir.join("shards");
    
    // Storage paths (LocalStorage expects <root>/models/<model_id>/shards/shard_<index>.bin)
    let storage_model_dir = args.data_dir.join("models").join(&args.model_id);
    let storage_shard_dir = storage_model_dir.join("shards");

    println!("Setting up test model:");
    println!("  model_id:   {}", args.model_id);
    println!("  model_name: {}", args.model_name);
    println!("  model_dir:  {}", model_dir.display());

    // Create directories
    tokio::fs::create_dir_all(&model_dir)
        .await
        .context("failed to create model directory")?;
    tokio::fs::create_dir_all(&shard_dir)
        .await
        .context("failed to create shard directory")?;

    // Step 1: Generate identity ONNX model
    println!("\n[1/3] Generating identity ONNX model...");
    write_identity_model(&model_path).await;
    let model_size = tokio::fs::metadata(&model_path)
        .await
        .context("failed to read model metadata")?
        .len();
    println!("  Created: {} ({} bytes)", model_path.display(), model_size);

    // Step 2: Split into shards
    println!("\n[2/3] Splitting model into shards...");
    let shards = model::split_model_file(&model_path, &shard_dir, &args.model_id)
        .await
        .context("failed to split model into shards")?;
    println!("  Created {} shard(s)", shards.len());

    // Step 3: Register model with ModelRegistry
    println!("\n[3/4] Registering model with registry...");
    let registry = ModelRegistry::new();

    let verification_hashes: Vec<[u8; 32]> = shards.iter().map(|s| s.hash).collect();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let metadata = ModelMetadata {
        model_id: args.model_id.clone(),
        name: args.model_name.clone(),
        version: args.version.clone(),
        total_size: model_size,
        shard_count: shards.len() as u32,
        verification_hashes: verification_hashes.clone(),
        is_core_model: true,
        minimum_tier: None,
        is_experimental: false,
        created_at: now,
    };

    registry.register_model(metadata)
        .context("failed to register model")?;
    println!("  Registered model: {}", args.model_id);

    // Register shards
    for shard in &shards {
        let model_shard = ModelShard {
            model_id: args.model_id.clone(),
            shard_index: shard.shard_index,
            total_shards: shard.total_shards,
            hash: shard.hash,
            size: shard.size,
            ipfs_cid: None,
            http_urls: Vec::new(),
            locations: vec![ShardLocation {
                node_id: "setup-node".to_string(),
                backend_type: "local".to_string(),
                location_uri: format!("file://{}", storage_shard_dir.display()),
                last_verified: now,
                is_healthy: true,
            }],
        };
        registry.register_shard(model_shard)
            .context(format!("failed to register shard {}", shard.shard_index))?;
    }
    println!("  Registered {} shard(s)", shards.len());

    // Set as core model
    registry.set_core_model(&args.model_id)
        .context("failed to set core model")?;
    println!("  Set {} as core model", args.model_id);

    // Step 4: Write manifest file for node registration
    println!("\n[4/4] Writing manifest file...");

    let verification_hashes: Vec<String> = shards.iter().map(|s| hex::encode(s.hash)).collect();
    let shard_infos: Vec<serde_json::Value> = shards
        .iter()
        .map(|s| {
            serde_json::json!({
                "shard_index": s.shard_index,
                "total_shards": s.total_shards,
                "hash": hex::encode(s.hash),
                "size": s.size,
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "model_id": args.model_id,
        "name": args.model_name,
        "version": args.version,
        "total_size": model_size,
        "shard_count": shards.len(),
        "verification_hashes": verification_hashes,
        "is_core_model": true,
        "minimum_tier": null,
        "is_experimental": false,
        "created_at": now,
        "shards": shard_infos,
        "model_path": model_path.display().to_string(),
        "shard_dir": storage_shard_dir.display().to_string(),
    });

    let manifest_path = model_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    tokio::fs::write(&manifest_path, &manifest_json)
        .await
        .context("failed to write manifest")?;
    println!("  Created: {}", manifest_path.display());

    // Copy shards to the storage location (LocalStorage expects them at <root>/models/<model_id>/shards/shard_<index>.bin)
    for shard in &shards {
        let src = shard_dir.join(format!(
            "{}_shard_{}.bin",
            args.model_id, shard.shard_index
        ));
        let dst = storage_shard_dir.join(format!("shard_{}.bin", shard.shard_index));
        tokio::fs::create_dir_all(&storage_shard_dir)
            .await
            .context("failed to create storage shard directory")?;
        tokio::fs::copy(&src, &dst)
            .await
            .context("failed to copy shard to storage")?;
    }

    println!("\n✅ Test model setup complete!");
    println!("\nModel registered and set as core: {}", args.model_id);
    println!("\nTo use this model, ensure your node/config.toml has:");
    println!("  [model]");
    println!("  core_model_id = \"{}\"", args.model_id);
    println!("  model_storage_path = \"{}\"", args.data_dir.join("models").display());

    Ok(())
}

/// Write a minimal ONNX identity model (echoes input to output).
async fn write_identity_model(path: &Path) {
    let tensor_type = onnx::TypeProtoTensor {
        elem_type: Some(1), // FLOAT
        shape: Some(onnx::TensorShapeProto {
            dim: vec![onnx::TensorShapeDim {
                dim_value: Some(1),
                dim_param: None,
            }],
        }),
    };
    let input = onnx::ValueInfoProto {
        name: Some("input".to_string()),
        r#type: Some(onnx::TypeProto {
            tensor_type: Some(tensor_type.clone()),
        }),
    };
    let output = onnx::ValueInfoProto {
        name: Some("output".to_string()),
        r#type: Some(onnx::TypeProto {
            tensor_type: Some(tensor_type),
        }),
    };
    let node = onnx::NodeProto {
        input: vec!["input".to_string()],
        output: vec!["output".to_string()],
        name: Some("identity".to_string()),
        op_type: Some("Identity".to_string()),
    };
    let graph = onnx::GraphProto {
        node: vec![node],
        name: Some("identity_graph".to_string()),
        input: vec![input],
        output: vec![output],
    };
    let model = onnx::ModelProto {
        ir_version: Some(8),
        opset_import: vec![onnx::OperatorSetIdProto {
            domain: Some("".to_string()),
            version: Some(13),
        }],
        producer_name: Some("aigen-setup".to_string()),
        producer_version: Some("1".to_string()),
        graph: Some(graph),
    };
    let mut buffer = Vec::new();
    model.encode(&mut buffer).expect("encode ONNX model");
    tokio::fs::write(path, buffer)
        .await
        .expect("write ONNX model file");
}

/// Minimal ONNX protobuf definitions (matching the ONNX spec).
mod onnx {
    use prost::Message;

    #[derive(Clone, PartialEq, Message)]
    pub struct ModelProto {
        #[prost(int64, optional, tag = "1")]
        pub ir_version: Option<i64>,
        #[prost(string, optional, tag = "2")]
        pub producer_name: Option<String>,
        #[prost(string, optional, tag = "3")]
        pub producer_version: Option<String>,
        #[prost(message, optional, tag = "7")]
        pub graph: Option<GraphProto>,
        #[prost(message, repeated, tag = "8")]
        pub opset_import: Vec<OperatorSetIdProto>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct OperatorSetIdProto {
        #[prost(string, optional, tag = "1")]
        pub domain: Option<String>,
        #[prost(int64, optional, tag = "2")]
        pub version: Option<i64>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct GraphProto {
        #[prost(message, repeated, tag = "1")]
        pub node: Vec<NodeProto>,
        #[prost(string, optional, tag = "2")]
        pub name: Option<String>,
        #[prost(message, repeated, tag = "11")]
        pub input: Vec<ValueInfoProto>,
        #[prost(message, repeated, tag = "12")]
        pub output: Vec<ValueInfoProto>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct NodeProto {
        #[prost(string, repeated, tag = "1")]
        pub input: Vec<String>,
        #[prost(string, repeated, tag = "2")]
        pub output: Vec<String>,
        #[prost(string, optional, tag = "3")]
        pub name: Option<String>,
        #[prost(string, optional, tag = "4")]
        pub op_type: Option<String>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct ValueInfoProto {
        #[prost(string, optional, tag = "1")]
        pub name: Option<String>,
        #[prost(message, optional, tag = "2")]
        pub r#type: Option<TypeProto>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct TypeProto {
        #[prost(message, optional, tag = "1")]
        pub tensor_type: Option<TypeProtoTensor>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct TypeProtoTensor {
        #[prost(int32, optional, tag = "1")]
        pub elem_type: Option<i32>,
        #[prost(message, optional, tag = "2")]
        pub shape: Option<TensorShapeProto>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct TensorShapeProto {
        #[prost(message, repeated, tag = "1")]
        pub dim: Vec<TensorShapeDim>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct TensorShapeDim {
        #[prost(int64, optional, tag = "1")]
        pub dim_value: Option<i64>,
        #[prost(string, optional, tag = "2")]
        pub dim_param: Option<String>,
    }
}
