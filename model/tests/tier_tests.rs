use std::collections::HashMap;
use std::sync::Arc;

use genesis::shutdown::reset_shutdown_for_tests;
use genesis::CEO_WALLET;

use model::{
    tiers::FeatureFlag,
    ModelMetadata,
    ModelRegistry,
    PaymentOutcome,
    PaymentProvider,
    PaymentStatus,
    SubscriptionTier,
    TierConfig,
    TierError,
    TierManager,
};

#[derive(Clone)]
struct TestPaymentProvider {
    outcome: PaymentOutcome,
}

impl PaymentProvider for TestPaymentProvider {
    fn charge(&self, _user_address: &str, _amount: u64) -> Result<PaymentOutcome, TierError> {
        Ok(self.outcome.clone())
    }
}

fn test_configs() -> HashMap<SubscriptionTier, TierConfig> {
    let mut configs = HashMap::new();
    configs.insert(
        SubscriptionTier::Free,
        TierConfig {
            tier: SubscriptionTier::Free,
            monthly_cost: 0,
            request_limit: 2,
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
        SubscriptionTier::Basic,
        TierConfig {
            tier: SubscriptionTier::Basic,
            monthly_cost: 10,
            request_limit: 5,
            window_seconds: 10,
            billing_cycle_seconds: 10,
            grace_period_seconds: 5,
            max_context_size: 2048,
            features: vec![FeatureFlag::BasicInference, FeatureFlag::ApiAccess],
            allowed_models: vec![],
            minimum_tier_for_access: Some(SubscriptionTier::Basic),
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
            features: vec![FeatureFlag::BasicInference, FeatureFlag::AdvancedInference, FeatureFlag::ApiAccess, FeatureFlag::BatchProcessing],
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
            features: vec![FeatureFlag::BasicInference, FeatureFlag::AdvancedInference, FeatureFlag::ModelTraining, FeatureFlag::BatchProcessing, FeatureFlag::PriorityQueue, FeatureFlag::ApiAccess, FeatureFlag::CustomModels],
            allowed_models: vec![],
            minimum_tier_for_access: None,
        },
    );
    configs
}

fn manager_with_outcome(outcome: PaymentOutcome) -> TierManager {
    TierManager::new(test_configs(), Arc::new(TestPaymentProvider { outcome }), None)
}

fn sample_metadata(model_id: &str, minimum_tier: Option<SubscriptionTier>, is_experimental: bool) -> ModelMetadata {
    ModelMetadata {
        model_id: model_id.to_string(),
        name: format!("Model {model_id}"),
        version: "1".to_string(),
        total_size: 1024,
        shard_count: 1,
        verification_hashes: vec![[0u8; 32]],
        is_core_model: false,
        minimum_tier,
        is_experimental,
        created_at: 1,
    }
}

#[test]
fn tier_rate_limit_enforced() {
    reset_shutdown_for_tests();
    let manager = manager_with_outcome(PaymentOutcome {
        status: PaymentStatus::Confirmed,
        reference: None,
    });
    let now = 100;
    manager
        .subscribe_user("0x00000000000000000000000000000000000000aa", SubscriptionTier::Free, false, now)
        .expect("subscribe");

    let first = manager
        .record_request("0x00000000000000000000000000000000000000aa", None, now)
        .expect("first request");
    assert!(first.allowed);
    assert_eq!(first.remaining, 1);

    let second = manager
        .record_request("0x00000000000000000000000000000000000000aa", None, now)
        .expect("second request");
    assert!(second.allowed);
    assert_eq!(second.remaining, 0);

    let third = manager
        .record_request("0x00000000000000000000000000000000000000aa", None, now)
        .expect("third request");
    assert!(!third.allowed);
    assert_eq!(third.remaining, 0);
}

#[test]
fn auto_renew_success_updates_subscription() {
    reset_shutdown_for_tests();
    let manager = manager_with_outcome(PaymentOutcome {
        status: PaymentStatus::Confirmed,
        reference: Some("ok".to_string()),
    });
    let start = 0;
    manager
        .subscribe_user("0x00000000000000000000000000000000000000bb", SubscriptionTier::Basic, true, start)
        .expect("subscribe");

    let renewed = manager.process_renewals(11);
    assert_eq!(renewed, 1);

    let updated = manager
        .get_subscription("0x00000000000000000000000000000000000000bb")
        .expect("subscription");
    assert_eq!(updated.status, model::SubscriptionStatus::Active);
    assert_eq!(updated.start_timestamp, 11);
    assert_eq!(updated.expiry_timestamp, 21);
}

#[test]
fn auto_renew_failure_marks_past_due() {
    reset_shutdown_for_tests();
    let manager = manager_with_outcome(PaymentOutcome {
        status: PaymentStatus::Failed,
        reference: Some("fail".to_string()),
    });
    manager
        .subscribe_user("0x00000000000000000000000000000000000000cc", SubscriptionTier::Basic, true, 0)
        .expect("subscribe");

    let renewed = manager.process_renewals(11);
    assert_eq!(renewed, 0);

    let updated = manager
        .get_subscription("0x00000000000000000000000000000000000000cc")
        .expect("subscription");
    assert_eq!(updated.status, model::SubscriptionStatus::PastDue);
    assert_eq!(updated.expiry_timestamp, 16);
}

#[test]
fn model_access_respects_tiers() {
    reset_shutdown_for_tests();
    let manager = manager_with_outcome(PaymentOutcome {
        status: PaymentStatus::Confirmed,
        reference: None,
    });
    let registry = ModelRegistry::new();

    registry
        .register_model(sample_metadata("open-model", None, false))
        .expect("register");
    registry
        .register_model(sample_metadata("pro-model", Some(SubscriptionTier::Basic), false))
        .expect("register");
    registry
        .register_model(sample_metadata("lab-model", Some(SubscriptionTier::Pro), true))
        .expect("register");

    manager
        .subscribe_user("0x00000000000000000000000000000000000000dd", SubscriptionTier::Basic, false, 0)
        .expect("subscribe");

    let now = 5;
    let open_access = manager
        .can_access_model(&registry, "0x00000000000000000000000000000000000000dd", "open-model", now)
        .expect("open access");
    assert!(open_access);

    let pro_access = manager
        .can_access_model(&registry, "0x00000000000000000000000000000000000000dd", "pro-model", now)
        .expect("pro access");
    assert!(pro_access);

    let lab_access = manager
        .can_access_model(&registry, "0x00000000000000000000000000000000000000dd", "lab-model", now)
        .expect("lab access");
    assert!(!lab_access);
}

#[test]
fn ceo_is_always_unlimited() {
    reset_shutdown_for_tests();
    let manager = manager_with_outcome(PaymentOutcome {
        status: PaymentStatus::Confirmed,
        reference: None,
    });
    let subscription = manager
        .ensure_subscription_active(CEO_WALLET, 42)
        .expect("ceo subscription");
    assert_eq!(subscription.tier, SubscriptionTier::Unlimited);
}
