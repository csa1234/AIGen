use std::sync::Arc;

use blockchain_core::types::{Amount, ChainId, Fee, Nonce, Timestamp, TxHash};
use blockchain_core::ChainState;
use blockchain_core::Transaction;
use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;
use model::{default_tier_configs, now_timestamp, SubscriptionTier, TierManager};
use model::tiers::SubscriptionPayload;
use model::{ModelMetadata, ModelRegistry};

fn make_tx(sender: &str, receiver: &str, amount: u64, payload: SubscriptionPayload) -> Transaction {
    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    Transaction {
        sender: sender.to_string(),
        receiver: receiver.to_string(),
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

#[test]
fn tier_transitions_workflow() {
    reset_shutdown_for_tests();
    let chain_state = Arc::new(ChainState::new());
    let manager = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    );
    let user = "0x0000000000000000000000000000000000000aaa";
    let now = now_timestamp();
    let sub_free = manager
        .subscribe_user(user, SubscriptionTier::Free, false, now)
        .expect("free");
    assert_eq!(sub_free.tier, SubscriptionTier::Free);
    let sub_basic = manager
        .subscribe_user(user, SubscriptionTier::Basic, false, now)
        .expect("basic");
    assert_eq!(sub_basic.tier, SubscriptionTier::Basic);
    let sub_pro = manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now)
        .expect("pro");
    assert_eq!(sub_pro.tier, SubscriptionTier::Pro);
    let sub_unl = manager
        .subscribe_user(user, SubscriptionTier::Unlimited, true, now)
        .expect("unlimited");
    assert_eq!(sub_unl.tier, SubscriptionTier::Unlimited);
}

#[test]
fn subscription_purchase_with_token_payment_validation() {
    reset_shutdown_for_tests();
    let chain_state = Arc::new(ChainState::new());
    let manager = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    );
    let user = "0x0000000000000000000000000000000000000bbb";
    let now = now_timestamp();
    let payload = SubscriptionPayload {
        tier: SubscriptionTier::Basic,
        duration_months: 1,
        user_address: user.to_string(),
    };
    let tx = make_tx(user, CEO_WALLET, 50, payload);
    let sub = manager
        .subscribe_user_with_transaction(&tx, CEO_WALLET, now)
        .expect("subscribe with tx");
    assert_eq!(sub.tier, SubscriptionTier::Basic);
    let state = chain_state.get_subscription(user).expect("state");
    assert_eq!(state.tier, SubscriptionTier::Basic.as_u8());
}

#[test]
fn subscription_expiration_and_auto_renewal() {
    reset_shutdown_for_tests();
    let chain_state = Arc::new(ChainState::new());
    let manager = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    );
    let user = "0x0000000000000000000000000000000000000ccc";
    manager
        .subscribe_user(user, SubscriptionTier::Basic, true, 0)
        .expect("subscribe");
    let renewed = manager.process_renewals(2_592_001);
    assert!(renewed >= 1);
    let sub = manager.get_subscription(user).expect("sub");
    assert_eq!(sub.status, model::SubscriptionStatus::Active);
}

#[test]
fn quota_enforcement_across_tier_boundaries() {
    reset_shutdown_for_tests();
    let chain_state = Arc::new(ChainState::new());
    let manager = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state,
    );
    let user = "0x0000000000000000000000000000000000000ddd";
    let now = now_timestamp();
    manager
        .subscribe_user(user, SubscriptionTier::Basic, false, now)
        .expect("sub");
    for _ in 0..100 {
        let d = manager
            .record_request(user, None, now)
            .expect("req basic");
        if !d.allowed {
            break;
        }
    }
    let after_basic = manager
        .record_request(user, None, now)
        .expect("post basic");
    assert!(!after_basic.allowed);
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now)
        .expect("upgrade");
    let d = manager
        .record_request(user, None, now)
        .expect("req pro");
    assert!(d.allowed);
}

#[test]
fn rate_limiting_free_tier_ip_and_wallet() {
    reset_shutdown_for_tests();
    let mut configs = default_tier_configs();
    if let Some(free) = configs.get_mut(&SubscriptionTier::Free) {
        free.request_limit = 2;
        free.window_seconds = 10;
    }
    let manager = TierManager::new(configs, Arc::new(model::DefaultPaymentProvider), None);
    let user = "0x0000000000000000000000000000000000000eee";
    let now = now_timestamp();
    manager
        .subscribe_user(user, SubscriptionTier::Free, false, now)
        .expect("free");
    let d1 = manager.record_request(user, Some("1.2.3.4"), now).expect("r1");
    assert!(d1.allowed);
    let _d2 = manager.record_request(user, Some("1.2.3.4"), now).expect("r2");
    let d3 = manager.record_request(user, Some("1.2.3.4"), now).expect("r3");
    assert!(!d3.allowed);
}

#[test]
fn tier_based_feature_gating_and_model_access() {
    reset_shutdown_for_tests();
    let registry = ModelRegistry::new();
    let open = ModelMetadata {
        model_id: "open".to_string(),
        name: "open".to_string(),
        version: "1".to_string(),
        total_size: 1,
        shard_count: 1,
        verification_hashes: vec![[0u8; 32]],
        is_core_model: false,
        minimum_tier: None,
        is_experimental: false,
        created_at: 1,
    };
    let pro_only = ModelMetadata {
        model_id: "pro-only".to_string(),
        name: "pro-only".to_string(),
        version: "1".to_string(),
        total_size: 1,
        shard_count: 1,
        verification_hashes: vec![[0u8; 32]],
        is_core_model: false,
        minimum_tier: Some(SubscriptionTier::Pro),
        is_experimental: false,
        created_at: 1,
    };
    registry.register_model(open).unwrap();
    registry.register_model(pro_only).unwrap();
    let manager = TierManager::new(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        None,
    );
    let user = "0x0000000000000000000000000000000000000fff";
    manager
        .subscribe_user(user, SubscriptionTier::Basic, false, now_timestamp())
        .unwrap();
    let allowed_open = manager
        .can_access_model(&registry, user, "open", now_timestamp())
        .unwrap();
    assert!(allowed_open);
    let allowed_pro = manager
        .can_access_model(&registry, user, "pro-only", now_timestamp())
        .unwrap();
    assert!(!allowed_pro);
}

#[tokio::test]
async fn concurrent_subscription_operations() {
    reset_shutdown_for_tests();
    let manager = Arc::new(TierManager::new(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        None,
    ));
    let user = "0x0000000000000000000000000000000000001000";
    let now = now_timestamp();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let m = manager.clone();
        tasks.push(tokio::spawn(async move {
            m.subscribe_user(user, SubscriptionTier::Basic, false, now)
                .unwrap();
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
    let sub = manager.get_subscription(user).expect("sub");
    assert_eq!(sub.tier, SubscriptionTier::Basic);
}

#[test]
fn subscription_state_persists_in_blockchain_state() {
    reset_shutdown_for_tests();
    let chain_state = Arc::new(ChainState::new());
    let manager = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    );
    let user = "0x0000000000000000000000000000000000002000";
    manager
        .subscribe_user(user, SubscriptionTier::Pro, false, now_timestamp())
        .unwrap();
    let snap = chain_state.get_subscription(user).expect("saved");
    assert_eq!(snap.tier, SubscriptionTier::Pro.as_u8());
    let manager2 = TierManager::with_chain_state(
        default_tier_configs(),
        Arc::new(model::DefaultPaymentProvider),
        chain_state.clone(),
    );
    let loaded = manager2.load_from_chain_state().unwrap();
    assert!(loaded >= 1);
    let sub = manager2.get_subscription(user).unwrap();
    assert_eq!(sub.tier, SubscriptionTier::Pro);
}
