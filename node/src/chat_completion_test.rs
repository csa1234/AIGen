// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

#[cfg(test)]
mod chat_completion_tests {
    use std::sync::Arc;
    use std::path::{Path, PathBuf};
    use tokio;
    use tokio::sync::Mutex;
    use blockchain_core::Blockchain;
    use model::{TierManager, InferenceEngine, ModelRegistry, BatchQueue, DefaultPaymentProvider, LocalStorage, VolumeDiscountTracker, AdManager, AdConfig, ModelMetadata, StorageBackend, split_model_file};
    use crate::rpc::types::*;
    use crate::rpc::model::ModelRpcMethods;
    use crate::rpc::model::ModelRpcServer;
    use blockchain_core::types::Amount;
    use blockchain_core::generate_keypair;
    use blockchain_core::Transaction;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use futures::StreamExt;
    use prost::Message;

    async fn setup_test_node() -> ModelRpcMethods {
        ensure_ort_available().await;
        let payment_provider = Arc::new(DefaultPaymentProvider);
        let tier_manager = Arc::new(TierManager::with_default_configs(payment_provider));
        let model_registry = Arc::new(ModelRegistry::new());
        let storage_root = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!("node_chat_models_{nanos}"))
        };
        let storage = Arc::new(LocalStorage::new(storage_root.clone()));
        let ad_manager = Arc::new(AdManager::new(
            tier_manager.clone(),
            AdConfig {
                enabled: true,
                injection_rate: 1.0,
                max_ad_length: 500,
                max_total_output_length: None,
                upgrade_prompt_threshold: 4,
                free_tier_only: true,
                templates: Some(model::default_ad_templates()),
                template_file_path: None,
            },
        ));
        let inference_engine = Arc::new(InferenceEngine::new(
            model_registry.clone(),
            storage.clone(),
            storage_root.join("cache"),
            64 * 1024 * 1024,
            1,
            Some(ad_manager),
            None,
            None,
        ));
        let batch_queue = Arc::new(BatchQueue::new(
            tier_manager.clone(),
            inference_engine.clone(),
            None,
            Arc::new(blockchain_core::ChainState::new()),
            VolumeDiscountTracker::new(vec![]),
        ));
        let blockchain = Arc::new(Mutex::new(Blockchain::new()));
        let model_id = "identity-chat";
        register_identity_model_input_ids(
            &model_registry,
            storage.as_ref(),
            &storage_root,
            model_id,
        )
        .await;
        ModelRpcMethods::new(
            blockchain,
            model_registry,
            tier_manager,
            batch_queue,
            inference_engine,
        )
    }

    async fn create_payment_tx_in_mempool(
        bc: &Arc<Mutex<Blockchain>>,
        model_id: &str,
        max_tokens: u32,
    ) -> (String, TransactionResponse) {
        let (public_key, signing_key) = generate_keypair();
        let sender = blockchain_core::derive_address_from_pubkey(&public_key);
        let receiver = genesis::CEO_WALLET.to_string();
        {
            let chain = bc.lock().await;
            chain
                .state
                .set_balance(
                    sender.clone(),
                    blockchain_core::types::Balance::zero()
                        .safe_add(Amount::new(1_000_000))
                        .unwrap(),
                );
        }
        let payload = ChatPaymentPayload {
            user_address: sender.clone(),
            model_id: model_id.to_string(),
            max_tokens,
        };
        let payload_bytes = serde_json::to_vec(&payload).unwrap();
        let tx = Transaction::new_default_chain(
            sender.clone(),
            receiver,
            model::CHAT_PRICE_PER_1K_TOKENS,
            1234567890,
            0,
            true,
            Some(payload_bytes),
        )
        .unwrap()
        .sign(&signing_key);
        {
            let mut chain = bc.lock().await;
            chain.add_transaction_to_pool(tx.clone()).unwrap();
        }
        let resp = TransactionResponse {
            sender: tx.sender.clone(),
            sender_public_key: Some(hex::encode(public_key.to_bytes())),
            receiver: tx.receiver.clone(),
            amount: tx.amount.value(),
            signature: hex::encode(tx.signature.to_bytes()),
            timestamp: tx.timestamp.0,
            nonce: tx.nonce.value(),
            priority: tx.priority,
            tx_hash: hex::encode(tx.tx_hash.0),
            fee_base: tx.fee.base_fee.value(),
            fee_priority: tx.fee.priority_fee.value(),
            fee_burn: tx.fee.burn_amount.value(),
            chain_id: tx.chain_id.value(),
            payload: tx.payload.as_ref().map(|p| hex::encode(p)),
            ceo_signature: None,
        };
        (sender, resp)
    }

    #[tokio::test]
    async fn test_chat_completion_with_subscription() {
        let rpc = setup_test_node().await;
        let user_address = "0x0000000000000000000000000000000000000abc".to_string();
        let now = model::now_timestamp();
        rpc.tier_manager
            .subscribe_user(&user_address, model::tiers::SubscriptionTier::Basic, false, now)
            .unwrap();
        let prompt = "Hello, how are you?";
        let request = ChatCompletionRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            model_id: "identity-chat".to_string(),
            stream: false,
            max_tokens: Some(100),
            temperature: None,
            user_address: Some(user_address.clone()),
            transaction: None,
        };
        let result = rpc.chat_completion(request).await.unwrap();
        let content = result.choices[0].message.content.clone();
        assert!(content.contains("user: Hello, how are you?"));
    }

    #[tokio::test]
    async fn test_chat_completion_pay_per_use() {
        let rpc = setup_test_node().await;
        let model_id = "identity-chat".to_string();
        let (sender, tx) = create_payment_tx_in_mempool(&rpc.blockchain, &model_id, 1000).await;
        rpc.tier_manager
            .subscribe_user(&sender, model::tiers::SubscriptionTier::Free, false, model::now_timestamp())
            .unwrap();
        let request = ChatCompletionRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Explain blockchain in simple terms".to_string(),
            }],
            model_id,
            stream: false,
            max_tokens: Some(1000),
            temperature: None,
            user_address: Some(sender.clone()),
            transaction: Some(tx),
        };
        let result = rpc.chat_completion(request).await.unwrap();
        assert!(!result.choices[0].message.content.is_empty());
    }

    #[tokio::test]
    async fn test_chat_completion_free_tier_ad_injection() {
        let rpc = setup_test_node().await;
        let user_address = "0x0000000000000000000000000000000000000fff".to_string();
        let now = model::now_timestamp();
        rpc.tier_manager
            .subscribe_user(&user_address, model::tiers::SubscriptionTier::Free, false, now)
            .unwrap();
        let request = ChatCompletionRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Say hi".to_string(),
            }],
            model_id: "identity-chat".to_string(),
            stream: false,
            max_tokens: Some(64),
            temperature: None,
            user_address: Some(user_address.clone()),
            transaction: None,
        };
        let result = rpc.chat_completion(request).await.unwrap();
        assert!(result.ad_injected);
    }

    #[tokio::test]
    async fn test_chat_completion_quota_exceeded() {
        let rpc = setup_test_node().await;
        let user_address = "0x0000000000000000000000000000000000000eee".to_string();
        let now = model::now_timestamp();
        rpc.tier_manager
            .subscribe_user(&user_address, model::tiers::SubscriptionTier::Free, false, now)
            .unwrap();
        for _ in 0..model::default_tier_configs()
            .get(&model::tiers::SubscriptionTier::Free)
            .unwrap()
            .request_limit
            + 1
        {
            let req = ChatCompletionRequest {
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "Ping".to_string(),
                }],
                model_id: "identity-chat".to_string(),
                stream: false,
                max_tokens: Some(8),
                temperature: None,
                user_address: Some(user_address.clone()),
                transaction: None,
            };
            let _ = rpc.chat_completion(req).await;
        }
        let final_req = ChatCompletionRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Should fail".to_string(),
            }],
            model_id: "identity-chat".to_string(),
            stream: false,
            max_tokens: Some(8),
            temperature: None,
            user_address: Some(user_address.clone()),
            transaction: None,
        };
        let err = rpc.chat_completion(final_req).await.unwrap_err();
        match err {
            RpcError::InsufficientTier | RpcError::QuotaExceeded | RpcError::PaymentRequired => {}
            other => panic!("unexpected error: {:?}", other),
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

    async fn register_identity_model_input_ids(
        registry: &Arc<ModelRegistry>,
        storage: &dyn StorageBackend,
        root: &Path,
        model_id: &str,
    ) {
        let model_dir = root.join("source").join(model_id);
        let model_path = model_dir.join("model.onnx");
        let shard_dir = model_dir.join("shards");
        fs::create_dir_all(&model_dir).await.unwrap();
        write_identity_model_with_names(&model_path, "input_ids", "output").await;
        let shards = split_model_file(&model_path, &shard_dir, model_id)
            .await
            .unwrap();
        let meta = ModelMetadata {
            model_id: model_id.to_string(),
            name: "identity-chat".to_string(),
            version: "1".to_string(),
            total_size: fs::metadata(&model_path).await.unwrap().len(),
            shard_count: shards.len() as u32,
            verification_hashes: shards.iter().map(|s| s.hash).collect(),
            is_core_model: false,
            minimum_tier: None,
            is_experimental: false,
            created_at: model::now_timestamp(),
        };
        registry.register_model(meta).unwrap();
        for shard in shards.iter() {
            registry.register_shard(shard.clone()).unwrap();
        }
        for shard in shards.iter() {
            let path = shard_dir.join(format!("{}_shard_{}.bin", model_id, shard.shard_index));
            storage.upload_shard(shard, &path).await.unwrap();
        }
    }

    async fn write_identity_model_with_names(path: &Path, input_name: &str, output_name: &str) {
        let tensor_type = onnx::TypeProtoTensor {
            elem_type: Some(1),
            shape: Some(onnx::TensorShapeProto {
                dim: vec![
                    onnx::TensorShapeDim {
                        dim_value: Some(1),
                        dim_param: None,
                    },
                    onnx::TensorShapeDim {
                        dim_value: None,
                        dim_param: Some("N".to_string()),
                    },
                ],
            }),
        };
        let input = onnx::ValueInfoProto {
            name: Some(input_name.to_string()),
            r#type: Some(onnx::TypeProto {
                tensor_type: Some(tensor_type.clone()),
            }),
        };
        let output = onnx::ValueInfoProto {
            name: Some(output_name.to_string()),
            r#type: Some(onnx::TypeProto {
                tensor_type: Some(tensor_type),
            }),
        };
        let node = onnx::NodeProto {
            input: vec![input_name.to_string()],
            output: vec![output_name.to_string()],
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
}
