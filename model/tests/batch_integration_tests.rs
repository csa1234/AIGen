// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::sync::Arc;
use uuid::Uuid;

use blockchain_core::types::{Amount, ChainId, Fee, Nonce, Timestamp, TxHash};
use blockchain_core::ChainState;
use blockchain_core::Transaction;
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;
use model::{
    now_timestamp, BatchJobStatus, BatchPaymentPayload, BatchPriority, BatchQueue, InferenceEngine,
    LocalStorage, ModelMetadata, ModelRegistry, SubscriptionTier, TierManager, VolumeDiscountTracker,
};

fn test_registry(model_id: &str, min_tier: Option<SubscriptionTier>) -> Arc<ModelRegistry> {
    let registry = Arc::new(ModelRegistry::new());
    let metadata = ModelMetadata {
        model_id: model_id.to_string(),
        name: model_id.to_string(),
        version: "1".to_string(),
        total_size: 1,
        shard_count: 1,
        verification_hashes: vec![[0u8; 32]],
        is_core_model: false,
        minimum_tier: min_tier,
        is_experimental: false,
        created_at: 1,
    };
    registry.register_model(metadata).unwrap();
    registry
}

fn test_engine(registry: Arc<ModelRegistry>) -> Arc<InferenceEngine> {
    let storage_root = std::env::temp_dir().join(format!("aigen-storage-{}", Uuid::new_v4()));
    let cache_root = std::env::temp_dir().join(format!("aigen-cache-{}", Uuid::new_v4()));
    let storage = Arc::new(LocalStorage::new(storage_root));
    Arc::new(InferenceEngine::new(
        registry, storage, cache_root, 10_000_000, 1, None, None, None,
    ))
}

fn make_tx(sender: &str, amount: u64, payload: BatchPaymentPayload) -> Transaction {
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    Transaction {
        sender: sender.to_string(),
        receiver: CEO_WALLET.to_string(),
        amount: Amount::new(amount),
        signature: ed25519_dalek::Signature::from([0u8; 64]),
        timestamp: Timestamp(now_timestamp()),
        nonce: Nonce::new(1),
        priority: false,
        tx_hash: TxHash([0u8; 32]),
        fee: Fee {
            base_fee: Amount::new(1),
            priority_fee: Amount::new(0),
            burn_amount: Amount::new(0),
        },
        chain_id: ChainId(1),
        payload: Some(payload_bytes),
        ceo_signature: None,
    }
}

#[tokio::test]
async fn submit_jobs_across_priorities_and_time_delays() {
    reset_shutdown_for_tests();
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
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

    let now = now_timestamp();
    let user = "0x0000000000000000000000000000000000000a01";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now)
        .expect("subscribe");
    let input = vec![1u8, 2, 3];

    let payload_std = BatchPaymentPayload {
        request_id: "req-std".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Standard,
        submission_time: now,
        scheduled_time: now,
        model_id: "model-a".to_string(),
        input_data: input.clone(),
    };
    let tx_std = make_tx(user, BatchPriority::Standard.base_price(), payload_std);
    let job_std = queue
        .validate_and_submit(&tx_std, "model-a".to_string(), input.clone())
        .await
        .expect("submit std");

    let payload_batch = BatchPaymentPayload {
        request_id: "req-batch".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Batch,
        submission_time: now,
        scheduled_time: now.saturating_add(BatchPriority::Batch.delay_seconds()),
        model_id: "model-a".to_string(),
        input_data: input.clone(),
    };
    let tx_batch = make_tx(user, BatchPriority::Batch.base_price(), payload_batch);
    let _ = queue
        .validate_and_submit(&tx_batch, "model-a".to_string(), input.clone())
        .await
        .expect("submit batch");

    let payload_econ = BatchPaymentPayload {
        request_id: "req-econ".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Economy,
        submission_time: now,
        scheduled_time: now.saturating_add(BatchPriority::Economy.delay_seconds()),
        model_id: "model-a".to_string(),
        input_data: input,
    };
    let tx_econ = make_tx(user, BatchPriority::Economy.base_price(), payload_econ);
    let _ = queue
        .validate_and_submit(&tx_econ, "model-a".to_string(), vec![9, 9])
        .await
        .expect("submit econ");

    let ready = queue.get_next_ready_job().await.expect("next");
    assert!(ready.is_some());
    assert_eq!(ready.unwrap().status, BatchJobStatus::Scheduled);
    let later = queue.get_next_ready_job().await.expect("next later");
    assert!(later.is_none());
    let status = queue.get_job_status(&job_std).await.expect("status");
    assert_eq!(status.priority, BatchPriority::Standard);
}

#[tokio::test]
async fn payment_validation_before_acceptance() {
    reset_shutdown_for_tests();
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
    let registry = test_registry("model-b", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );
    let user = "0x0000000000000000000000000000000000000a02";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now_timestamp())
        .expect("subscribe");
    let payload = BatchPaymentPayload {
        request_id: "req-x".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Standard,
        submission_time: now_timestamp(),
        scheduled_time: now_timestamp(),
        model_id: "model-b".to_string(),
        input_data: vec![1],
    };
    let tx = make_tx(user, BatchPriority::Standard.base_price(), payload);
    let res = queue
        .validate_and_submit(&tx, "model-b".to_string(), vec![1])
        .await;
    assert!(res.is_ok());
    let payload_bad = BatchPaymentPayload {
        request_id: "req-y".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Standard,
        submission_time: now_timestamp(),
        scheduled_time: now_timestamp(),
        model_id: "model-b".to_string(),
        input_data: vec![1],
    };
    let tx_bad = make_tx(user, 999, payload_bad);
    let res_bad = queue
        .validate_and_submit(&tx_bad, "model-b".to_string(), vec![1])
        .await;
    assert!(res_bad.is_err());
}

#[test]
fn volume_discount_unlimited_tier() {
    reset_shutdown_for_tests();
    let tracker = VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers());
    let manager = TierManager::with_default_configs(Arc::new(model::DefaultPaymentProvider));
    let user = "0x0000000000000000000000000000000000000a03";
    let now = now_timestamp();
    let _ = manager
        .subscribe_user(user, SubscriptionTier::Unlimited, true, now)
        .unwrap();
    for i in 0..110 {
        tracker.record_job(user, now + i as i64);
    }
    let discounted = tracker
        .apply_volume_discount(&manager, user, BatchPriority::Standard.base_price(), now + 200)
        .unwrap();
    assert!(discounted < BatchPriority::Standard.base_price());
}

#[tokio::test]
async fn job_state_transitions_end_to_end() {
    reset_shutdown_for_tests();
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
    let registry = test_registry("model-c", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );
    let now = now_timestamp();
    let user = "0x0000000000000000000000000000000000000a04";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now)
        .expect("subscribe");
    let payload = BatchPaymentPayload {
        request_id: "req-z".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Standard,
        submission_time: now,
        scheduled_time: now,
        model_id: "model-c".to_string(),
        input_data: vec![1, 2, 3],
    };
    let tx = make_tx(user, BatchPriority::Standard.base_price(), payload);
    let job_id = queue
        .validate_and_submit(&tx, "model-c".to_string(), vec![1, 2, 3])
        .await
        .expect("submit");
    queue
        .update_job_status(&job_id, BatchJobStatus::Processing, None, None, false)
        .await
        .expect("processing");
    queue
        .update_job_status(
            &job_id,
            BatchJobStatus::Completed,
            Some(vec![7, 7, 7]),
            None,
            false,
        )
        .await
        .expect("completed");
    let job = queue.get_job(&job_id).await.expect("job");
    assert_eq!(job.status, BatchJobStatus::Completed);
    assert_eq!(job.result, Some(vec![7, 7, 7]));
}

#[tokio::test]
async fn job_status_queries() {
    reset_shutdown_for_tests();
    let manager = Arc::new(TierManager::with_default_configs(Arc::new(
        model::DefaultPaymentProvider,
    )));
    let registry = test_registry("model-d", Some(SubscriptionTier::Pro));
    let engine = test_engine(registry);
    let chain_state = Arc::new(ChainState::new());
    let queue = BatchQueue::new(
        manager.clone(),
        engine,
        None,
        chain_state,
        VolumeDiscountTracker::new(VolumeDiscountTracker::default_tiers()),
    );
    let user = "0x0000000000000000000000000000000000000a05";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now_timestamp())
        .expect("subscribe");
    let payload = BatchPaymentPayload {
        request_id: "req-q".to_string(),
        user_address: user.to_string(),
        priority: BatchPriority::Standard,
        submission_time: now_timestamp(),
        scheduled_time: now_timestamp(),
        model_id: "model-d".to_string(),
        input_data: vec![9],
    };
    let tx = make_tx(user, BatchPriority::Standard.base_price(), payload);
    let job_id = queue
        .validate_and_submit(&tx, "model-d".to_string(), vec![9])
        .await
        .expect("submit");
    let info = queue.get_job_status(&job_id).await.expect("info");
    assert_eq!(info.user_address, user);
    assert_eq!(info.priority, BatchPriority::Standard);
}
