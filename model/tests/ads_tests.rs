use std::sync::Arc;

use genesis::shutdown::reset_shutdown_for_tests;

use model::{
    now_timestamp, AdConfig, AdImpression, AdImpressionLog, AdManager, AdTemplate,
    DefaultPaymentProvider, InferenceOutput, SubscriptionTier, TierManager,
};

fn manager_with_free_user(user_address: &str) -> (Arc<TierManager>, AdManager) {
    let tier_manager = Arc::new(TierManager::with_default_configs(Arc::new(
        DefaultPaymentProvider,
    )));
    let now = now_timestamp();
    tier_manager
        .subscribe_user(user_address, SubscriptionTier::Free, false, now)
        .expect("subscribe");
    let config = AdConfig {
        injection_rate: 1.0,
        ..AdConfig::default()
    };
    let ad_manager = AdManager::new(tier_manager.clone(), config);
    (tier_manager, ad_manager)
}

fn data_from_text(text: &str) -> Vec<f32> {
    text.as_bytes().iter().map(|b| *b as f32).collect()
}

fn text_from_data(data: &[f32]) -> String {
    let bytes: Vec<u8> = data
        .iter()
        .map(|value| value.round().clamp(0.0, 255.0) as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

#[test]
fn should_inject_ads_for_free_tier() {
    reset_shutdown_for_tests();
    let (_tier_manager, ad_manager) =
        manager_with_free_user("0x0000000000000000000000000000000000000a01");
    let inject = ad_manager
        .should_inject_ad("0x0000000000000000000000000000000000000a01")
        .expect("inject decision");
    assert!(inject);
}

#[test]
fn should_skip_ads_for_paid_tier() {
    reset_shutdown_for_tests();
    let tier_manager = Arc::new(TierManager::with_default_configs(Arc::new(
        DefaultPaymentProvider,
    )));
    let now = now_timestamp();
    tier_manager
        .subscribe_user(
            "0x0000000000000000000000000000000000000a02",
            SubscriptionTier::Basic,
            false,
            now,
        )
        .expect("subscribe");
    let config = AdConfig {
        injection_rate: 1.0,
        ..AdConfig::default()
    };
    let ad_manager = AdManager::new(tier_manager, config);
    let inject = ad_manager
        .should_inject_ad("0x0000000000000000000000000000000000000a02")
        .expect("inject decision");
    assert!(!inject);
}

#[test]
fn generates_upgrade_prompt_when_threshold_reached() {
    reset_shutdown_for_tests();
    let (tier_manager, ad_manager) =
        manager_with_free_user("0x0000000000000000000000000000000000000a03");
    let now = now_timestamp();
    for _ in 0..9 {
        tier_manager
            .record_request("0x0000000000000000000000000000000000000a03", None, now)
            .expect("record");
    }
    let content = ad_manager
        .generate_ad_content("0x0000000000000000000000000000000000000a03", "model-x")
        .expect("content");
    assert!(content.contains("Upgrade:"));
    assert!(content.contains("Tier comparison:"));
}

#[test]
fn injects_ads_into_outputs() {
    reset_shutdown_for_tests();
    let (_tier_manager, ad_manager) =
        manager_with_free_user("0x0000000000000000000000000000000000000a04");
    let outputs = vec![InferenceOutput {
        name: "output".to_string(),
        shape: vec![5],
        data: data_from_text("Hello"),
    }];
    let (modified, template_id, tier) = ad_manager
        .inject_ad_into_outputs(
            outputs,
            "0x0000000000000000000000000000000000000a04",
            "model-x",
        )
        .expect("inject");
    assert!(!template_id.is_empty());
    assert_eq!(tier, SubscriptionTier::Free);
    let text = text_from_data(&modified[0].data);
    assert!(text.contains("Hello"));
    assert!(text.contains("AIGEN"));
}

#[test]
fn tracks_and_cleans_impressions() {
    reset_shutdown_for_tests();
    let (_tier_manager, ad_manager) =
        manager_with_free_user("0x0000000000000000000000000000000000000a05");
    ad_manager
        .record_impression(
            "0x0000000000000000000000000000000000000a05",
            "t1",
            "model-a",
            SubscriptionTier::Free,
        )
        .expect("record");
    ad_manager
        .record_impression(
            "0x0000000000000000000000000000000000000a05",
            "t2",
            "model-a",
            SubscriptionTier::Free,
        )
        .expect("record");
    assert_eq!(
        ad_manager.get_user_impressions("0x0000000000000000000000000000000000000a05"),
        2
    );
    let performance = ad_manager.get_template_performance();
    assert_eq!(*performance.get("t1").unwrap_or(&0), 1);

    let old_ts = now_timestamp().saturating_sub(91 * 24 * 60 * 60);
    ad_manager.insert_impression_log(
        "0x0000000000000000000000000000000000000a06",
        AdImpressionLog {
            user_address: "0x0000000000000000000000000000000000000a06".to_string(),
            impressions: vec![AdImpression {
                timestamp: old_ts,
                template_id: "old".to_string(),
                model_id: "model-old".to_string(),
                tier: SubscriptionTier::Free,
            }],
            total_impressions: 1,
            last_impression_time: old_ts,
        },
    );
    ad_manager.cleanup_old_impressions().expect("cleanup");
    assert_eq!(
        ad_manager.get_user_impressions("0x0000000000000000000000000000000000000a06"),
        0
    );
}

#[test]
fn rotates_templates_round_robin() {
    reset_shutdown_for_tests();
    let (_tier_manager, ad_manager) =
        manager_with_free_user("0x0000000000000000000000000000000000000a07");
    for template in ad_manager.list_templates() {
        let _ = ad_manager.remove_template(&template.id);
    }
    ad_manager.add_template(AdTemplate {
        id: "t1".to_string(),
        content: "Template one {model_name}".to_string(),
        priority: 1,
        active: true,
    });
    ad_manager.add_template(AdTemplate {
        id: "t2".to_string(),
        content: "Template two {model_name}".to_string(),
        priority: 1,
        active: true,
    });
    let outputs = vec![InferenceOutput {
        name: "output".to_string(),
        shape: vec![3],
        data: data_from_text("Hey"),
    }];
    let (_, first_id, _) = ad_manager
        .inject_ad_into_outputs(
            outputs.clone(),
            "0x0000000000000000000000000000000000000a07",
            "model-x",
        )
        .expect("inject");
    let (_, second_id, _) = ad_manager
        .inject_ad_into_outputs(
            outputs,
            "0x0000000000000000000000000000000000000a07",
            "model-x",
        )
        .expect("inject");
    assert_ne!(first_id, second_id);
}
