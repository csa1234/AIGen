// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use ed25519_dalek::{Signature, Signer, SigningKey};
use futures::future::join_all;
use futures::StreamExt;
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::{emergency_shutdown, CeoSignature, GenesisConfig, ShutdownCommand};
use model::{
    deterministic_inference_with_engine, outputs_match, split_model_file, InferenceEngine,
    InferenceError, InferenceOutput, InferenceTensor, LocalStorage, ModelMetadata, ModelRegistry,
    StorageBackend, VerificationCache,
};
use prost::Message;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;

const CEO_SECRET_KEY_HEX: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const TEST_USER: &str = "0x00000000000000000000000000000000000000aa";

fn signing_key() -> SigningKey {
    let bytes = hex::decode(CEO_SECRET_KEY_HEX).expect("valid hex");
    let seed: [u8; 32] = bytes.as_slice().try_into().expect("valid seed length");
    SigningKey::from_bytes(&seed)
}

#[tokio::test]
async fn test_verification_cache_roundtrip() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("verification_cache");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );
    let cache = VerificationCache::new(8);

    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.42],
    };
    let expected = engine
        .run_inference("identity", vec![input.clone()], TEST_USER)
        .await
        .expect("run inference");
    let outputs =
        deterministic_inference_with_engine(&engine, "identity", vec![input.clone()], Some(&cache))
            .await
            .expect("deterministic inference");

    assert!(outputs_match(&expected, &outputs, 1e-6));
    assert_eq!(cache.len(), 1);

    let outputs_again =
        deterministic_inference_with_engine(&engine, "identity", vec![input], Some(&cache))
            .await
            .expect("deterministic inference (cached)");
    assert!(outputs_match(&expected, &outputs_again, 1e-6));
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_verification_cache_eviction() {
    let cache = VerificationCache::new(2);
    let output = vec![InferenceOutput {
        name: "output".to_string(),
        shape: vec![1],
        data: vec![1.0],
    }];

    let key_a = [1u8; 32];
    let key_b = [2u8; 32];
    let key_c = [3u8; 32];

    cache.insert(key_a, output.clone());
    cache.insert(key_b, output.clone());
    cache.insert(key_c, output.clone());

    assert_eq!(cache.len(), 2);
    assert!(cache.get(&key_a).is_none());
    assert!(cache.get(&key_b).is_some());
    assert!(cache.get(&key_c).is_some());
}

static ORT_INIT: OnceCell<()> = OnceCell::const_new();

async fn ensure_ort_available() {
    ORT_INIT
        .get_or_init(|| async {
            if let Ok(path) = std::env::var("ORT_DYLIB_PATH") {
                let candidate = PathBuf::from(path);
                if candidate.exists() {
                    let _ = ort::init_from(candidate).and_then(|builder| {
                        builder.commit();
                        Ok(())
                    });
                    return;
                }
            }

            let root = std::env::temp_dir()
                .join("aigen_ort_runtime")
                .join("1.23.2");
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
    fs::create_dir_all(out_dir)
        .await
        .expect("create ort cache dir");

    if fs::metadata(&zip_path).await.is_err() {
        let response = reqwest::get(URL).await.expect("download onnxruntime");
        if !response.status().is_success() {
            panic!(
                "failed to download onnxruntime: status={}",
                response.status()
            );
        }

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
            let file_name = match std::path::Path::new(&entry_name).file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                _ => continue,
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

fn signed_shutdown_command(nonce: u64) -> ShutdownCommand {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("valid time")
        .as_secs() as i64;
    let network_magic = GenesisConfig::default().network_magic;
    let placeholder = Signature::from_bytes(&[0u8; 64]);
    let mut command = ShutdownCommand {
        timestamp,
        reason: "test".to_string(),
        ceo_signature: CeoSignature(placeholder),
        nonce,
        network_magic,
    };
    let sig = signing_key().sign(&command.message_to_sign());
    command.ceo_signature = CeoSignature(sig);
    command
}

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("aigen_{prefix}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_secs() as i64
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
        fs::create_dir_all(&model_dir)
            .await
            .expect("create model dir");
        write_identity_model(&model_path).await;

        let shards = split_model_file(&model_path, &shard_dir, model_id)
            .await
            .expect("split");
        let metadata = ModelMetadata {
            model_id: model_id.to_string(),
            name: format!("Test {model_id}"),
            version: "1".to_string(),
            total_size: fs::metadata(&model_path).await.expect("metadata").len(),
            shard_count: shards.len() as u32,
            verification_hashes: shards.iter().map(|shard| shard.hash).collect(),
            is_core_model: is_core,
            minimum_tier: None,
            is_experimental: false,
            created_at: now_timestamp(),
        };
        self.registry
            .register_model(metadata)
            .expect("register model");

        for shard in shards.iter() {
            self.registry
                .register_shard(shard.clone())
                .expect("register shard");
        }

        for shard in shards.iter() {
            let shard_path =
                shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
            self.storage
                .upload_shard(shard, &shard_path)
                .await
                .expect("upload shard");
        }

        fs::metadata(&model_path).await.expect("metadata").len()
    }
}

#[tokio::test]
async fn test_load_model_from_shards() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_load");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );

    let model = engine
        .cache()
        .get_or_load("identity")
        .await
        .expect("load model");
    assert_eq!(model.model_id, "identity");
    assert_eq!(model.input_names, vec!["input".to_string()]);
    assert_eq!(model.output_names, vec!["output".to_string()]);
}

#[tokio::test]
async fn test_inference_execution() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_run");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );

    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.25],
    };
    let outputs = engine
        .run_inference("identity", vec![input], TEST_USER)
        .await
        .expect("run inference");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].data, vec![0.25]);
}

#[tokio::test]
async fn test_cache_hit_miss() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_cache_hit");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );

    engine
        .cache()
        .get_or_load("identity")
        .await
        .expect("load model");
    engine
        .cache()
        .get_or_load("identity")
        .await
        .expect("load model");

    let metrics = engine.metrics();
    assert_eq!(
        metrics
            .cache_misses
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        metrics
            .cache_hits
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn test_lru_eviction() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_lru");
    let size_a = harness.add_identity_model("model_a", false).await;
    let size_b = harness.add_identity_model("model_b", false).await;
    let limit = (size_a.max(size_b) as usize) + 1;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        limit,
        1,
        None,
        None,
        None,
    );

    engine
        .cache()
        .get_or_load("model_a")
        .await
        .expect("load model A");
    engine
        .cache()
        .get_or_load("model_b")
        .await
        .expect("load model B");

    let metrics = engine.metrics();
    assert!(
        metrics
            .cache_evictions
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
    assert_eq!(
        metrics
            .current_models_loaded
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn test_core_model_protection() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_core");
    let size_a = harness.add_identity_model("model_a", true).await;
    let size_b = harness.add_identity_model("model_b", false).await;
    let limit = (size_a.max(size_b) as usize) + 1;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        limit,
        1,
        None,
        None,
        None,
    );

    engine
        .cache()
        .get_or_load("model_a")
        .await
        .expect("load core model");
    engine
        .cache()
        .get_or_load("model_b")
        .await
        .expect("load secondary model");

    let core = engine.cache().get_core_model().expect("core model");
    assert_eq!(core, Some("model_a".to_string()));
    let metrics = engine.metrics();
    assert_eq!(
        metrics
            .current_models_loaded
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn test_concurrent_inference() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_concurrent");
    let size = harness.add_identity_model("identity", false).await;
    let engine = Arc::new(InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    ));

    let mut futures = Vec::new();
    for _ in 0..4 {
        let engine = engine.clone();
        futures.push(async move {
            let input = InferenceTensor {
                name: "input".to_string(),
                shape: vec![1],
                data: vec![1.0],
            };
            engine
                .run_inference("identity", vec![input], TEST_USER)
                .await
        });
    }

    let results = join_all(futures).await;
    for result in results {
        let outputs = result.expect("inference");
        assert_eq!(outputs[0].data, vec![1.0]);
    }
}

#[tokio::test]
async fn test_shutdown_blocks_inference() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_shutdown");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );

    let command = signed_shutdown_command(1);
    emergency_shutdown(command).expect("shutdown");

    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.5],
    };
    let result = engine
        .run_inference("identity", vec![input], TEST_USER)
        .await;
    assert!(matches!(result, Err(InferenceError::Shutdown)));
    reset_shutdown_for_tests();
}

#[tokio::test]
async fn test_metrics_tracking() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let harness = TestHarness::new("inference_metrics");
    let size = harness.add_identity_model("identity", false).await;
    let engine = InferenceEngine::new(
        harness.registry.clone(),
        harness.storage.clone(),
        harness.cache_dir(),
        (size as usize) * 2,
        1,
        None,
        None,
        None,
    );

    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.75],
    };
    engine
        .run_inference("identity", vec![input], TEST_USER)
        .await
        .expect("run inference");

    let stats = engine.get_metrics();
    assert_eq!(stats.total_inference_runs, 1);
    assert_eq!(stats.total_inference_failures, 0);
    assert!(stats.cache_misses >= 1);
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
        producer_name: Some("aigen-test".to_string()),
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
