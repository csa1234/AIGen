// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::path::{Path, PathBuf};
use std::sync::Arc;

use genesis::shutdown::reset_shutdown_for_tests;
use genesis::{emergency_shutdown, GenesisConfig, CEO_WALLET};
use model::{
    split_model_file, InferenceEngine, InferenceTensor, LocalStorage, ModelMetadata, ModelRegistry,
    StorageBackend, SubscriptionTier, TierManager,
};
use tokio::fs;
use prost::Message;
use ed25519_dalek::Signer;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

fn temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("aigen-e2e-{}", prefix))
}

struct TestHarness {
    root: PathBuf,
    registry: Arc<ModelRegistry>,
    storage: Arc<dyn StorageBackend>,
}

impl TestHarness {
    fn new(prefix: &str) -> Self {
        let root = temp_dir(prefix);
        let registry = Arc::new(ModelRegistry::new());
        let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(root.clone()));
        Self {
            root,
            registry,
            storage,
        }
    }

    fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    async fn add_identity_model(&self, model_id: &str, is_core: bool) -> u64 {
        let model_dir = self.root.join("source").join(model_id);
        let model_path = model_dir.join("model.onnx");
        let shard_dir = model_dir.join("shards");
        fs::create_dir_all(&model_dir).await.expect("mkdir");
        write_identity_model(&model_path).await;
        let shards = split_model_file(&model_path, &shard_dir, model_id)
            .await
            .expect("split");
        let metadata = ModelMetadata {
            model_id: model_id.to_string(),
            name: model_id.to_string(),
            version: "1".to_string(),
            total_size: fs::metadata(&model_path).await.expect("md").len(),
            shard_count: shards.len() as u32,
            verification_hashes: shards.iter().map(|s| s.hash).collect(),
            is_core_model: is_core,
            minimum_tier: None,
            is_experimental: false,
            created_at: model::now_timestamp(),
        };
        self.registry.register_model(metadata).unwrap();
        for shard in shards.iter() {
            self.registry.register_shard(shard.clone()).unwrap();
        }
        for shard in shards.iter() {
            let shard_path =
                shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
            self.storage.upload_shard(shard, &shard_path).await.unwrap();
        }
        fs::metadata(&model_path).await.expect("md").len()
    }
}

#[tokio::test]
async fn complete_node_like_flow_with_identity_model() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("e2e_complete");
    let size = harness.add_identity_model("core-identity", true).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
    );
    let model = engine
        .cache()
        .get_or_load("core-identity")
        .await
        .expect("load");
    assert_eq!(model.model_id, "core-identity");
    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.5],
    };
    let out = engine
        .run_inference("core-identity", vec![input], "0xeeee")
        .await
        .expect("infer");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data, vec![0.5]);
}

#[tokio::test]
async fn multi_model_lru_eviction_and_switching() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("e2e_switch");
    let sa = harness.add_identity_model("model-a", true).await;
    let sb = harness.add_identity_model("model-b", false).await;
    let limit = (sa.max(sb) as usize) + 1;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        limit,
        1,
        None,
    );
    engine.cache().get_or_load("model-a").await.unwrap();
    engine.cache().get_or_load("model-b").await.unwrap();
    let metrics = engine.metrics();
    assert_eq!(
        metrics
            .current_models_loaded
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn subscription_and_quota_deduction_on_inference() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("e2e_sub_infer");
    let size = harness.add_identity_model("id", false).await;
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
    );
    let user = "0xaaaa0000000000000000000000000000000000aa";
    let now = model::now_timestamp();
    manager
        .subscribe_user(user, SubscriptionTier::Basic, false, now)
        .unwrap();
    let before = manager.check_quota_readonly(user, now).unwrap();
    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.25],
    };
    let _ = engine
        .run_inference("id", vec![input], user)
        .await
        .expect("infer");
    let after = manager.record_request(user, None, now).unwrap();
    assert!(after.allowed);
    assert!(after.remaining < before.remaining);
}

#[tokio::test]
async fn batch_then_inference_flow() {
    reset_shutdown_for_tests();
    let harness = TestHarness::new("e2e_batch");
    let size = harness.add_identity_model("model-x", false).await;
    let engine = Arc::new(InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
    ));
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
    let chain_state = Arc::new(blockchain_core::ChainState::new());
    let queue = model::BatchQueue::new(
        manager.clone(),
        engine.clone(),
        None,
        chain_state,
        model::VolumeDiscountTracker::new(model::VolumeDiscountTracker::default_tiers()),
    );
    let user = "0xaaaa0000000000000000000000000000000000bb";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, model::now_timestamp())
        .unwrap();
    let payload = model::BatchPaymentPayload {
        request_id: "req-1".to_string(),
        user_address: user.to_string(),
        priority: model::BatchPriority::Standard,
        submission_time: model::now_timestamp(),
        scheduled_time: model::now_timestamp(),
        model_id: "model-x".to_string(),
        input_data: vec![1, 2, 3],
    };
    let tx = blockchain_core::Transaction::new_default_chain(
        user.to_string(),
        CEO_WALLET.to_string(),
        model::BatchPriority::Standard.base_price(),
        model::now_timestamp(),
        1,
        false,
        Some(serde_json::to_vec(&payload).unwrap()),
    )
    .unwrap();
    let job_id = queue
        .validate_and_submit(&tx, "model-x".to_string(), vec![1, 2, 3])
        .await
        .unwrap();
    queue
        .update_job_status(
            &job_id,
            model::BatchJobStatus::Completed,
            Some(vec![7]),
            None,
            false,
        )
        .await
        .unwrap();
    let job = queue.get_job(&job_id).await.unwrap();
    assert_eq!(job.status, model::BatchJobStatus::Completed);
}

#[tokio::test]
async fn redundancy_check_three_replication() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let meta = ModelMetadata {
        model_id: "m".to_string(),
        name: "m".to_string(),
        version: "1".to_string(),
        total_size: 1,
        shard_count: 2,
        verification_hashes: vec![[0u8; 32]; 2],
        is_core_model: false,
        minimum_tier: None,
        is_experimental: false,
        created_at: model::now_timestamp(),
    };
    registry.register_model(meta).unwrap();
    for idx in 0..2 {
        registry
            .register_shard(model::ModelShard {
                model_id: "m".to_string(),
                shard_index: idx,
                total_shards: 2,
                hash: [0u8; 32],
                size: 1,
                ipfs_cid: None,
                http_urls: Vec::new(),
                locations: Vec::new(),
            })
            .unwrap();
        for n in 0..3 {
            let loc = model::ShardLocation {
                node_id: format!("node-{}", n),
                backend_type: "local".to_string(),
                location_uri: format!("mem://node-{}/{}", n, idx),
                last_verified: model::now_timestamp(),
                is_healthy: true,
            };
            let _ = registry.register_shard_location("m", idx, loc);
        }
    }
    for idx in 0..2 {
        let locs = registry.get_shard_locations("m", idx).unwrap();
        assert!(locs.len() >= 3);
    }
}

#[tokio::test]
async fn shutdown_during_inference_graceful() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("e2e_shutdown");
    let size = harness.add_identity_model("z", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
    );
    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.1],
    };
    let fut = engine.run_inference("z", vec![input], "0xaaaa");
    let cmd = build_shutdown_command("e2e shutdown");
    emergency_shutdown(cmd).expect("shutdown");
    let res = fut.await;
    assert!(res.is_err());
    reset_shutdown_for_tests();
}

async fn write_identity_model(path: &Path) {
    let tensor_type = onnx::TypeProtoTensor {
        elem_type: Some(1),
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
        producer_name: Some("aigen-e2e".to_string()),
        producer_version: Some("1".to_string()),
        graph: Some(graph),
    };
    let mut buffer = Vec::new();
    model.encode(&mut buffer).expect("encode");
    fs::write(path, buffer).await.expect("write model");
}

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

fn build_shutdown_command(reason: &str) -> genesis::types::ShutdownCommand {
    let timestamp = 42;
    let nonce = 7;
    let network_magic = GenesisConfig::default().network_magic;
    let message = format!(
        "shutdown:{}:{}:{}:{}",
        network_magic, timestamp, nonce, reason
    );
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ]);
    let signature = signing_key.sign(message.as_bytes());
    genesis::types::ShutdownCommand {
        timestamp,
        reason: reason.to_string(),
        ceo_signature: genesis::types::CeoSignature(signature),
        nonce,
        network_magic,
    }
}

static ORT_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
async fn ensure_ort_available() {
    ORT_INIT
        .get_or_init(|| async {
            if let Ok(path) = std::env::var("ORT_DYLIB_PATH") {
                let candidate = PathBuf::from(path);
                if candidate.exists() {
                    if let Ok(builder) = ort::init_from(candidate) {
                        builder.commit();
                    }
                    return;
                }
            }
            let root = std::env::temp_dir().join("aigen_ort_runtime").join("1.23.2");
            let dll_path = root.join("onnxruntime.dll");
            if !dll_path.exists() {
                download_and_extract_ort_1232(&root).await;
            }
            std::env::set_var("ORT_DYLIB_PATH", &dll_path);
            let builder = ort::init_from(&dll_path).expect("init ort");
            builder.commit();
        })
        .await;
}

async fn download_and_extract_ort_1232(out_dir: &Path) {
    const URL: &str = "https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/onnxruntime-win-x64-1.23.2.zip";
    let zip_path = out_dir.join("onnxruntime-win-x64-1.23.2.zip");
    fs::create_dir_all(out_dir).await.expect("create ort cache dir");
    if fs::metadata(&zip_path).await.is_err() {
        let response = reqwest::get(URL).await.expect("download onnxruntime");
        assert!(response.status().is_success());
        let mut file = fs::File::create(&zip_path).await.expect("create zip file");
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.expect("download chunk");
            file.write_all(&chunk).await.expect("write chunk");
        }
        file.flush().await.expect("flush zip");
    }
    let out_dir = out_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&zip_path).expect("open zip");
        let mut archive = zip::ZipArchive::new(file).expect("read zip");
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("zip entry");
            let entry_name = entry.name().to_string();
            let file_name = if let Some(name) = std::path::Path::new(&entry_name).file_name() {
                name.to_string_lossy().to_string()
            } else {
                continue;
            };
            if !file_name.to_lowercase().ends_with(".dll") {
                continue;
            }
            let out_path = out_dir.join(&file_name);
            let mut out_file = std::fs::File::create(&out_path).expect("create dll");
            std::io::copy(&mut entry, &mut out_file).expect("extract dll");
        }
    })
    .await
    .expect("extract join");
}
