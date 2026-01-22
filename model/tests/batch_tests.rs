use std::collections::HashMap;
use std::sync::Arc;

use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;
use uuid::Uuid;

use blockchain_core::transaction::Transaction;
use blockchain_core::ChainState;

use model::{
    BatchError,
    BatchJobStatus,
    BatchPaymentPayload,
    BatchPriority,
    BatchQueue,
    BatchRequest,
    InferenceEngine,
    InferenceTensor,
    LocalStorage,
    ModelMetadata,
    ModelRegistry,
    SubscriptionTier,
    TierConfig,
    TierManager,
    VolumeDiscountTracker,
    DefaultPaymentProvider,
    tiers::FeatureFlag,
    validate_batch_payment,
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
        registry,
        storage,
        cache_root,
        10_000_000,
        1,
    ))
}

fn test_manager() -> TierManager {
    TierManager::new(test_configs(), Arc::new(DefaultPaymentProvider), None)
}

fn sample_payload(user_address: &str, priority: BatchPriority, model_id: &str, input_data: Vec<u8>) -> BatchPaymentPayload {
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
async fn priority_queue_orders_by_priority_when_same_schedule() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user("0x00000000000000000000000000000000000000aa", SubscriptionTier::Pro, false, now)
        .expect("subscribe");

    let input = sample_input();
    let standard_payload = sample_payload("0x00000000000000000000000000000000000000aa", BatchPriority::Standard, "model-a", input.clone());
    let batch_payload = sample_payload("0x00000000000000000000000000000000000000aa", BatchPriority::Batch, "model-a", input.clone());
    let economy_payload = sample_payload("0x00000000000000000000000000000000000000aa", BatchPriority::Economy, "model-a", input.clone());

    let standard_tx = make_tx("0x00000000000000000000000000000000000000aa", CEO_WALLET, 10, standard_payload);
    let batch_tx = make_tx("0x00000000000000000000000000000000000000aa", CEO_WALLET, 5, batch_payload);
    let economy_tx = make_tx("0x00000000000000000000000000000000000000aa", CEO_WALLET, 3, economy_payload);

    let standard_id = queue
        .validate_and_submit(&standard_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit standard");
    let batch_id = queue
        .validate_and_submit(&batch_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit batch");
    let economy_id = queue
        .validate_and_submit(&economy_tx, "model-a".to_string(), input.clone())
        .await
        .expect("submit economy");

    let schedule_time = model::now_timestamp();
    queue.reschedule_job(&standard_id, schedule_time).await.expect("reschedule");
    queue.reschedule_job(&batch_id, schedule_time).await.expect("reschedule");
    queue.reschedule_job(&economy_id, schedule_time).await.expect("reschedule");

    let first = queue.get_next_ready_job().await.expect("ready").expect("job");
    let second = queue.get_next_ready_job().await.expect("ready").expect("job");
    let third = queue.get_next_ready_job().await.expect("ready").expect("job");

    assert_eq!(first.priority, BatchPriority::Standard);
    assert_eq!(second.priority, BatchPriority::Batch);
    assert_eq!(third.priority, BatchPriority::Economy);
}

#[tokio::test]
async fn payment_validation_accepts_expected_amount() {
    reset_shutdown_for_tests();
    let input = sample_input();
    let payload = sample_payload("0x00000000000000000000000000000000000000bb", BatchPriority::Standard, "model-a", input.clone());
    let tx = make_tx("0x00000000000000000000000000000000000000bb", CEO_WALLET, 10, payload.clone());
    let parsed = validate_batch_payment(&tx, CEO_WALLET, Some(10), "model-a".to_string(), input.clone());
    assert!(parsed.is_ok());
    let invalid = validate_batch_payment(&tx, CEO_WALLET, Some(5), "model-a".to_string(), input);
    assert!(invalid.is_err());
}

#[tokio::test]
async fn job_state_transitions_update_status() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager,
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let request = BatchRequest {
        request_id: Uuid::new_v4().to_string(),
        user_address: "0x00000000000000000000000000000000000000cc".to_string(),
        priority: BatchPriority::Standard,
        submission_time: 0,
        scheduled_time: 0,
        model_id: "model-a".to_string(),
        input_data: sample_input(),
        payment_tx_hash: blockchain_core::TxHash([0u8; 32]),
    };

    let job_id = queue.submit_job(request).await.expect("submit");
    queue
        .update_job_status(&job_id, BatchJobStatus::Processing, None, None)
        .await
        .expect("processing");
    queue
        .update_job_status(&job_id, BatchJobStatus::Completed, Some(vec![1, 2, 3]), None)
        .await
        .expect("completed");
    let job = queue.get_job(&job_id).await.expect("job");
    assert_eq!(job.status, BatchJobStatus::Completed);
}

#[tokio::test]
async fn batch_jobs_wait_for_schedule() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager,
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let request = BatchRequest {
        request_id: Uuid::new_v4().to_string(),
        user_address: "0x00000000000000000000000000000000000000dd".to_string(),
        priority: BatchPriority::Batch,
        submission_time: 0,
        scheduled_time: 0,
        model_id: "model-a".to_string(),
        input_data: sample_input(),
        payment_tx_hash: blockchain_core::TxHash([0u8; 32]),
    };
    queue.submit_job(request).await.expect("submit");
    let next = queue.get_next_ready_job().await.expect("ready");
    assert!(next.is_none());
}

#[tokio::test]
async fn volume_discounts_apply_at_thresholds() {
    reset_shutdown_for_tests();
    let tracker = VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers());
    let user = "0x00000000000000000000000000000000000000ee";
    let now = model::now_timestamp();
    for _ in 0..100 {
        tracker.record_job(user, now);
    }
    let discount = tracker.calculate_discount(user, now);
    assert!((discount - 0.10).abs() < 1e-6);
}

#[tokio::test]
async fn tier_restrictions_block_free_tier() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user("0x00000000000000000000000000000000000000ff", SubscriptionTier::Free, false, now)
        .expect("subscribe");

    let input = sample_input();
    let payload = sample_payload("0x00000000000000000000000000000000000000ff", BatchPriority::Standard, "model-a", input.clone());
    let tx = make_tx("0x00000000000000000000000000000000000000ff", CEO_WALLET, 10, payload);
    let res = queue.validate_and_submit(&tx, "model-a".to_string(), input).await;
    assert!(matches!(res, Err(BatchError::InsufficientTier)));
}

#[tokio::test]
async fn concurrent_submissions_keep_all_jobs() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = test_registry("model-a", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = Arc::new(BatchQueue::new(
        manager.clone(),
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    ));

    let now = model::now_timestamp();
    manager
        .subscribe_user("0x0000000000000000000000000000000000000100", SubscriptionTier::Pro, false, now)
        .expect("subscribe");

    let mut handles = Vec::new();
    for _ in 0..5 {
        let queue = queue.clone();
        handles.push(tokio::spawn(async move {
            let request = BatchRequest {
                request_id: Uuid::new_v4().to_string(),
                user_address: "0x0000000000000000000000000000000000000100".to_string(),
                priority: BatchPriority::Standard,
                submission_time: 0,
                scheduled_time: 0,
                model_id: "model-a".to_string(),
                input_data: sample_input(),
                payment_tx_hash: blockchain_core::TxHash([0u8; 32]),
            };
            queue.submit_job(request).await.expect("submit");
        }));
    }
    for handle in handles {
        handle.await.expect("join");
    }

    let stats = queue.get_queue_stats().await;
    let count = stats.jobs_by_priority.get(&BatchPriority::Standard).cloned().unwrap_or(0);
    assert!(count >= 5);
}

#[tokio::test]
async fn error_when_model_missing() {
    reset_shutdown_for_tests();
    let manager = Arc::new(test_manager());
    let registry = Arc::new(ModelRegistry::new());
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );

    let now = model::now_timestamp();
    manager
        .subscribe_user("0x0000000000000000000000000000000000000101", SubscriptionTier::Pro, false, now)
        .expect("subscribe");

    let input = sample_input();
    let payload = sample_payload("0x0000000000000000000000000000000000000101", BatchPriority::Standard, "missing", input.clone());
    let tx = make_tx("0x0000000000000000000000000000000000000101", CEO_WALLET, 10, payload);
    let res = queue.validate_and_submit(&tx, "missing".to_string(), input).await;
    assert!(matches!(res, Err(BatchError::ModelNotFound)));
}
