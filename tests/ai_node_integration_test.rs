use std::sync::Arc;
use std::time::Instant;

use genesis::shutdown::reset_shutdown_for_tests;
use model::{LocalStorage, ModelMetadata, ModelRegistry};
use network::{ModelShardResponse, ModelSyncManager, NetworkMessage};
use tokio::sync::mpsc;
use tokio::fs;
use futures::StreamExt;
use std::path::{Path, PathBuf};
use prost::Message;

#[tokio::test]
async fn node_init_core_model_download_and_load() {
    reset_shutdown_for_tests();
    ensure_ort_available().await;
    let registry = Arc::new(ModelRegistry::new());
    let storage_root = std::env::temp_dir().join("ai_node_core");
    let storage = Arc::new(LocalStorage::new(storage_root.clone()));
    let (publish_tx, mut publish_rx) = mpsc::channel::<NetworkMessage>(64);
    let (request_tx, mut request_rx) = mpsc::channel::<network::model_sync::ModelShardRequestEnvelope>(64);
    let (event_tx, _event_rx) = mpsc::channel::<network::NetworkEvent>(64);
    let metrics = network::metrics::NetworkMetrics::default();
    let manager = ModelSyncManager::new(
        registry.clone(),
        storage.clone(),
        Arc::new(network::reputation::PeerReputationManager::new(0.2, std::time::Duration::from_secs(60))),
        Arc::new(metrics),
        publish_tx,
        request_tx,
        event_tx,
        "node-1".to_string(),
    );
    let core_id = "core-identity";
    let model_dir = storage_root.join("source").join(core_id);
    let model_path = model_dir.join("model.onnx");
    let shard_dir = model_dir.join("shards");
    fs::create_dir_all(&model_dir).await.expect("model dir");
    write_identity_model(&model_path).await;
    let shards = model::split_model_file(&model_path, &shard_dir, core_id)
        .await
        .expect("split");
    let meta = ModelMetadata {
        model_id: core_id.to_string(),
        name: "core-identity".to_string(),
        version: "1".to_string(),
        total_size: fs::metadata(&model_path).await.expect("meta").len(),
        shard_count: shards.len() as u32,
        verification_hashes: shards.iter().map(|s| s.hash).collect(),
        is_core_model: true,
        minimum_tier: None,
        is_experimental: false,
        created_at: model::now_timestamp(),
    };
    registry.register_model(meta).expect("register core");
    manager.query_model_shards(core_id).await.unwrap();
    let _q = publish_rx.recv().await.expect("query sent");
    manager
        .request_shard_download(core_id.to_string(), 0)
        .expect("queue");
    manager.process_download_queue().await.unwrap();
    let envelope = request_rx.recv().await.expect("req");
    let first = shards.first().expect("first shard").clone();
    let shard_path = shard_dir.join(format!("{}_shard_{}.bin", core_id, first.shard_index));
    let data = tokio::fs::read(&shard_path).await.expect("read shard");
    let resp = ModelShardResponse {
        model_id: core_id.to_string(),
        shard_index: envelope.request.shard_index,
        total_shards: shards.len() as u32,
        data: data.clone(),
        hash: blockchain_core::hash_data(&data),
        size: data.len() as u64,
    };
    manager.handle_received_shard(envelope.peer, resp).await.unwrap();
    manager.announce_local_shards().await.unwrap();
    let _a = publish_rx.recv().await.expect("announce sent");
    let start = Instant::now();
    let engine = model::InferenceEngine::new(
        registry.clone(),
        storage.clone(),
        std::env::temp_dir().join("ai_node_cache"),
        16 * 1024 * 1024,
        1,
        None,
    );
    let res = engine.preload_core_model().await;
    let elapsed = start.elapsed();
    assert!(elapsed.as_secs() <= 30);
    assert!(res.unwrap().is_some());
    let input = model::InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.33],
    };
    let out = engine
        .run_inference(core_id, vec![input], "0xdeadbeef")
        .await
        .expect("infer");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data, vec![0.33]);
}

#[tokio::test]
async fn worker_mode_fails_when_core_model_unavailable() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let exists = registry.model_exists("missing").unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn non_worker_mode_starts_without_core_model() {
    reset_shutdown_for_tests();
    let registry = Arc::new(ModelRegistry::new());
    let storage = Arc::new(LocalStorage::new(std::env::temp_dir().join("ai_node_no_core")));
    let engine = model::InferenceEngine::new(
        registry.clone(),
        storage,
        std::env::temp_dir().join("ai_node_cache_no_core"),
        16 * 1024 * 1024,
        1,
        None,
    );
    assert!(engine.get_metrics().total_models_loaded == 0);
}

#[tokio::test]
async fn redundancy_check_at_least_five_nodes_store_shards() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let meta = ModelMetadata {
        model_id: "core".to_string(),
        name: "core".to_string(),
        version: "1".to_string(),
        total_size: 1,
        shard_count: 2,
        verification_hashes: vec![[0u8; 32]; 2],
        is_core_model: true,
        minimum_tier: None,
        is_experimental: false,
        created_at: model::now_timestamp(),
    };
    registry.register_model(meta).unwrap();
    for idx in 0..2 {
        registry
            .register_shard(model::ModelShard {
                model_id: "core".to_string(),
                shard_index: idx,
                total_shards: 2,
                hash: [0u8; 32],
                size: 1,
                ipfs_cid: None,
                http_urls: Vec::new(),
                locations: Vec::new(),
            })
            .unwrap();
        for n in 0..5 {
            let loc = model::ShardLocation {
                node_id: format!("node-{}", n),
                backend_type: "local".to_string(),
                location_uri: format!("mem://{}", n),
                last_verified: model::now_timestamp(),
                is_healthy: true,
            };
            let _ = registry.register_shard_location("core", idx, loc);
        }
    }
    for idx in 0..2 {
        let locs = registry.get_shard_locations("core", idx).unwrap();
        assert!(locs.len() >= 5);
    }
}

static ORT_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

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
        if !response.status().is_success() {
            panic!("failed to download onnxruntime: status={}", response.status());
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
