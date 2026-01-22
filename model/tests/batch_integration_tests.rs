use std::collections::HashMap;
use std::sync::Arc;

use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;
use uuid::Uuid;

use blockchain_core::transaction::Transaction;
use blockchain_core::ChainState;

use model::{
    tiers::FeatureFlag, BatchError, BatchJobStatus, BatchPaymentPayload, BatchPriority, BatchQueue,
    BatchWorker, DefaultPaymentProvider, InferenceEngine, InferenceTensor, LocalStorage,
    ModelMetadata, ModelRegistry, SubscriptionTier, TierConfig, TierManager, VolumeDiscountTracker,
};

fn test_configs() -> HashMap<SubscriptionTier, TierConfig> {
    let mut configs = HashMap::new();
    configs.insert(
        SubscriptionTier::Free,
        TierConfig {
            tier: SubscriptionTier::Free,
            monthly_cost: 0,
            request_limit: 5,
            window_seconds: 10,
            billing_cycle_seconds: 10,
            grace_period_seconds: 5,
            max_context_size: 1024,
            features: vec![FeatureFlag::BasicInference],
            allowed_models: vec![],
            minimum_tier_for_access: None,
        },
    );
    configs.insert(
        SubscriptionTier::Pro,
        TierConfig {
            tier: SubscriptionTier::Pro,
            monthly_cost: 50,
            request_limit: 20,
            window_seconds: 10,
            billing_cycle_seconds: 10,
            grace_period_seconds: 5,
            max_context_size: 4096,
            features: vec![
                FeatureFlag::BasicInference,
                FeatureFlag::AdvancedInference,
                FeatureFlag::BatchProcessing,
            ],
            allowed_models: vec![],
            minimum_tier_for_access: Some(SubscriptionTier::Pro),
        },
    );
    configs.insert(
        SubscriptionTier::Unlimited,
        TierConfig {
            tier: SubscriptionTier::Unlimited,
            monthly_cost: 0,
            request_limit: u64::MAX,
            window_seconds: 1,
            billing_cycle_seconds: 10,
            grace_period_seconds: 5,
            max_context_size: 8192,
            features: vec![
                FeatureFlag::BasicInference,
                FeatureFlag::AdvancedInference,
                FeatureFlag::ModelTraining,
                FeatureFlag::BatchProcessing,
                FeatureFlag::PriorityQueue,
                FeatureFlag::ApiAccess,
                FeatureFlag::CustomModels,
            ],
            allowed_models: vec![],
            minimum_tier_for_access: None,
        },
    );
    configs
}

fn sample_metadata(model_id: &str, minimum_tier: Option<SubscriptionTier>) -> ModelMetadata {
    ModelMetadata {
        model_id: model_id.to_string(),
        name: format!("Model {model_id}"),
        version: "1".to_string(),
        total_size: 1024,
        shard_count: 1,
        verification_hashes: vec![[0u8; 32]],
        is_core_model: false,
        minimum_tier,
        is_experimental: false,
        created_at: 1,
    }
}

fn test_registry(model_id: &str, minimum_tier: Option<SubscriptionTier>) -> Arc<ModelRegistry> {
    let registry = Arc::new(ModelRegistry::new());
    registry
        .register_model(sample_metadata(model_id, minimum_tier))
        .expect("register model");
    registry
}

fn test_engine(registry: Arc<ModelRegistry>) -> Arc<InferenceEngine> {
    let storage_root = std::env::temp_dir().join(format!("aigen-storage-{}", Uuid::new_v4()));
    let cache_root = std::env::temp_dir().join(format!("aigen-cache-{}", Uuid::new_v4()));
    let storage = Arc::new(LocalStorage::new(storage_root));
    Arc::new(InferenceEngine::new(
        registry, storage, cache_root, 10_000_000, 1, None,
    ))
}

fn test_manager() -> TierManager {
    TierManager::new(test_configs(), Arc::new(DefaultPaymentProvider), None)
}

fn sample_payload(
    user_address: &str,
    priority: BatchPriority,
    model_id: &str,
    input_data: Vec<u8>,
) -> BatchPaymentPayload {
    BatchPaymentPayload {
        request_id: Uuid::new_v4().to_string(),
        user_address: user_address.to_string(),
        priority,
        submission_time: 0,
        scheduled_time: 0,
        model_id: model_id.to_string(),
        input_data,
    }
}

fn sample_input() -> Vec<u8> {
    let inputs = vec![InferenceTensor {
        name: "input".to_string(),
        shape: vec![1],
        data: vec![0.5],
    }];
    serde_json::to_vec(&inputs).expect("serialize inputs")
}

fn make_tx(sender: &str, receiver: &str, amount: u64, payload: BatchPaymentPayload) -> Transaction {
    let payload_bytes = serde_json::to_vec(&payload).expect("payload");
    Transaction::new_default_chain(
        sender.to_string(),
        receiver.to_string(),
        amount,
        100,
        0,
        false,
        Some(payload_bytes),
    )
    .expect("tx")
}

#[tokio::test]
async fn full_workflow_updates_job_status() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user(
            "0x00000000000000000000000000000000000001aa",
            SubscriptionTier::Pro,
            false,
            now,
        )
        .expect("subscribe");

    let input = sample_input();
    let payload = sample_payload(
        "0x00000000000000000000000000000000000001aa",
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let tx = make_tx(
        "0x00000000000000000000000000000000000001aa",
        CEO_WALLET,
        10,
        payload,
    );
    let job_id = queue
        .validate_and_submit(&tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit");

    queue
        .reschedule_job(&job_id, model::now_timestamp())
        .await
        .expect("reschedule");
    let job = queue
        .get_next_ready_job()
        .await
        .expect("ready")
        .expect("job");
    queue
        .update_job_status(
            &job.request_id,
            BatchJobStatus::Processing,
            None,
            None,
            false,
        )
        .await
        .expect("processing");
    queue
        .update_job_status(
            &job.request_id,
            BatchJobStatus::Completed,
            Some(vec![9]),
            None,
            false,
        )
        .await
        .expect("completed");

    let fetched = queue.get_job(&job_id).await.expect("job");
    assert_eq!(fetched.status, BatchJobStatus::Completed);
}

#[tokio::test]
async fn multiple_priorities_follow_order() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user(
            "0x00000000000000000000000000000000000001bb",
            SubscriptionTier::Pro,
            false,
            now,
        )
        .expect("subscribe");

    let input = sample_input();
    let standard_payload = sample_payload(
        "0x00000000000000000000000000000000000001bb",
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let batch_payload = sample_payload(
        "0x00000000000000000000000000000000000001bb",
        BatchPriority::Batch,
        "model-a",
        input.clone(),
    );
    let economy_payload = sample_payload(
        "0x00000000000000000000000000000000000001bb",
        BatchPriority::Economy,
        "model-a",
        input.clone(),
    );

    let standard_tx = make_tx(
        "0x00000000000000000000000000000000000001bb",
        CEO_WALLET,
        10,
        standard_payload,
    );
    let batch_tx = make_tx(
        "0x00000000000000000000000000000000000001bb",
        CEO_WALLET,
        5,
        batch_payload,
    );
    let economy_tx = make_tx(
        "0x00000000000000000000000000000000000001bb",
        CEO_WALLET,
        3,
        economy_payload,
    );

    let standard_id = queue
        .validate_and_submit(&standard_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit");
    let batch_id = queue
        .validate_and_submit(&batch_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit");
    let economy_id = queue
        .validate_and_submit(&economy_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit");

    let schedule_time = model::now_timestamp();
    queue
        .reschedule_job(&standard_id, schedule_time)
        .await
        .expect("reschedule");
    queue
        .reschedule_job(&batch_id, schedule_time)
        .await
        .expect("reschedule");
    queue
        .reschedule_job(&economy_id, schedule_time)
        .await
        .expect("reschedule");

    let first = queue
        .get_next_ready_job()
        .await
        .expect("ready")
        .expect("job");
    let second = queue
        .get_next_ready_job()
        .await
        .expect("ready")
        .expect("job");
    let third = queue
        .get_next_ready_job()
        .await
        .expect("ready")
        .expect("job");

    assert_eq!(first.priority, BatchPriority::Standard);
    assert_eq!(second.priority, BatchPriority::Batch);
    assert_eq!(third.priority, BatchPriority::Economy);
}

#[tokio::test]
async fn volume_discounts_lower_expected_payment() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user(
            "0x00000000000000000000000000000000000001cc",
            SubscriptionTier::Unlimited,
            false,
            now,
        )
        .expect("subscribe");

    let tracker = queue.discount_tracker();
    for _ in 0..120 {
        tracker.record_job("0x00000000000000000000000000000000000001cc", now);
    }

    let input = sample_input();
    let payload = sample_payload(
        "0x00000000000000000000000000000000000001cc",
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let tx = make_tx(
        "0x00000000000000000000000000000000000001cc",
        CEO_WALLET,
        9,
        payload,
    );
    let res = queue
        .validate_and_submit(&tx, "model-a".to_string(), input)
        .await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn tier_enforcement_blocks_free_allows_pro() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user(
            "0x00000000000000000000000000000000000001dd",
            SubscriptionTier::Free,
            false,
            now,
        )
        .expect("subscribe");
    manager
        .subscribe_user(
            "0x00000000000000000000000000000000000001ee",
            SubscriptionTier::Pro,
            false,
            now,
        )
        .expect("subscribe");

    let input = sample_input();
    let free_payload = sample_payload(
        "0x00000000000000000000000000000000000001dd",
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let free_tx = make_tx(
        "0x00000000000000000000000000000000000001dd",
        CEO_WALLET,
        10,
        free_payload,
    );
    let free_res = queue
        .validate_and_submit(&free_tx, "model-a".to_string(), input.clone())
        .await;
    assert!(matches!(free_res, Err(BatchError::InsufficientTier)));

    let pro_payload = sample_payload(
        "0x00000000000000000000000000000000000001ee",
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let pro_tx = make_tx(
        "0x00000000000000000000000000000000000001ee",
        CEO_WALLET,
        10,
        pro_payload,
    );
    let pro_res = queue
        .validate_and_submit(&pro_tx, "model-a".to_string(), input)
        .await;
    assert!(pro_res.is_ok());
}

#[tokio::test]
async fn chain_state_persists_job_updates() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state.clone(),
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    let user_address = "0x00000000000000000000000000000000000001ff";
    manager
        .subscribe_user(user_address, SubscriptionTier::Pro, false, now)
        .expect("subscribe");
    let input = sample_input();
    let payload = sample_payload(
        user_address,
        BatchPriority::Standard,
        "model-a",
        input.clone(),
    );
    let tx = make_tx(user_address, CEO_WALLET, 10, payload);
    let job_id = queue
        .validate_and_submit(&tx, "model-a".to_string(), input)
        .await
        .expect("submit");
    queue
        .update_job_status(
            &job_id,
            BatchJobStatus::Completed,
            Some(vec![4]),
            None,
            false,
        )
        .await
        .expect("complete");

    let state = chain_state.get_batch_job(&job_id).expect("state");
    assert_eq!(state.status, BatchJobStatus::Completed.as_u8());
}

#[tokio::test]
async fn worker_shutdown_stops_loop() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = Arc::new(BatchQueue::new(
        manager,
        engine.clone(),
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    ));

    let (tx, rx) = tokio::sync::watch::channel(false);
    let worker = BatchWorker::new(queue, engine, rx, Some(10));
    let handle = worker.start();
    tx.send(true).expect("signal");
    handle.await.expect("join");
}
