use consensus::poi::verify_inference;
use consensus::{
    set_inference_verification_config, CompressionMethod, ComputationMetadata,
    InferenceVerificationConfig,
};
use futures::StreamExt;
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;
use model::split_model_file;
use model::{
    set_global_inference_engine, InferenceEngine, InferenceTensor, LocalStorage, ModelMetadata,
    ModelRegistry, StorageBackend,
};
use prost::Message;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;

const MODEL_ID: &str = "identity";

static ENGINE_INIT: OnceCell<Arc<InferenceEngine>> = OnceCell::const_new();
static ORT_INIT: OnceCell<()> = OnceCell::const_new();

#[tokio::test]
async fn test_inference_verification_success() {
    let engine = setup_engine().await;
    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.25],
    };
    let expected = engine
        .run_inference(MODEL_ID, vec![input.clone()], CEO_WALLET)
        .await
        .expect("run inference");

    let input_bytes = bincode::serialize(&vec![input]).expect("serialize input");
    let output_bytes = bincode::serialize(&expected).expect("serialize output");

    let metadata = ComputationMetadata {
        rows: 0,
        cols: 0,
        inner: 0,
        iterations: 0,
        model_id: MODEL_ID.to_string(),
        compression_method: CompressionMethod::None,
        original_size: 0,
    };

    let ok = verify_inference(&input_bytes, &output_bytes, &metadata).expect("verify inference");
    assert!(ok);
}

#[tokio::test]
async fn test_inference_verification_failure() {
    let engine = setup_engine().await;
    let input = InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.25],
    };
    let mut expected = engine
        .run_inference(MODEL_ID, vec![input.clone()], CEO_WALLET)
        .await
        .expect("run inference");
    expected[0].data[0] += 1.0;

    let input_bytes = bincode::serialize(&vec![input]).expect("serialize input");
    let output_bytes = bincode::serialize(&expected).expect("serialize output");

    let metadata = ComputationMetadata {
        rows: 0,
        cols: 0,
        inner: 0,
        iterations: 0,
        model_id: MODEL_ID.to_string(),
        compression_method: CompressionMethod::None,
        original_size: 0,
    };

    let ok = verify_inference(&input_bytes, &output_bytes, &metadata).expect("verify inference");
    assert!(!ok);
}

async fn setup_engine() -> Arc<InferenceEngine> {
    ENGINE_INIT
        .get_or_init(|| async {
            reset_shutdown_for_tests();
            ensure_ort_available().await;
            let harness = TestHarness::new("consensus_verification");
            let size = harness.add_identity_model(MODEL_ID, false).await;
            let engine = Arc::new(InferenceEngine::new(
                harness.registry.clone(),
                harness.storage.clone(),
                harness.cache_dir(),
                (size as usize) * 2,
                1,
                None,
            ));
            let _ = set_global_inference_engine(engine.clone());
            let _ = set_inference_verification_config(InferenceVerificationConfig {
                cache_capacity: 16,
                epsilon: 1e-6,
                timeout_ms: 10_000,
            });
            engine
        })
        .await
        .clone()
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
