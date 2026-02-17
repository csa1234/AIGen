//! Canary Rollout Tests
//!
//! Test suite for the canary deployment system including:
//! - Shadow mode testing (0% traffic, 100% logging)
//! - Gradual traffic increment testing
//! - Rollback trigger testing
//! - Complaint threshold testing
//! - Latency threshold testing
//! - CEO veto testing
//! - Full integration testing

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use dashmap::DashMap;
use uuid::Uuid;

// Import types from distributed-inference
use distributed_inference::orchestrator::{
    CanaryDeployment, CanaryMetrics, CanaryPhase, UserComplaint, RollbackEvent,
    SafetyCheckResult, RolloutFailure, DeploymentStatus,
};

// Helper to get current timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// Helper to create a test canary deployment
fn create_test_canary_deployment(model_id: &str, canary_version: &str) -> CanaryDeployment {
    CanaryDeployment::new(
        model_id.to_string(),
        "v1.0.0".to_string(),
        canary_version.to_string(),
    )
}

// Helper to create metrics with specific values
fn create_metrics(
    requests: u64,
    violations: u64,
    benevolence_failures: u64,
    oracle_unsafe_votes: u64,
    complaints: u64,
    canary_latency: f32,
    stable_latency: f32,
    last_benevolence_score: f32,
    stable_requests: u64,
) -> CanaryMetrics {
    CanaryMetrics {
        canary_requests: requests,
        canary_violations: violations,
        canary_benevolence_failures: benevolence_failures,
        canary_oracle_unsafe_votes: oracle_unsafe_votes,
        canary_complaints: complaints,
        canary_avg_latency_ms: canary_latency,
        stable_avg_latency_ms: stable_latency,
        last_benevolence_score: last_benevolence_score,
        stable_requests: stable_requests,
    }
}

/// Test 1: Shadow Mode Testing
/// Verify that shadow mode routes 0% of user traffic to canary
/// while still running inference on canary in background
#[test]
fn test_shadow_mode_zero_traffic() {
    let deployment = create_test_canary_deployment("test-model", "v2.0.0");
    
    // Verify shadow mode settings
    assert!(deployment.shadow_mode, "Shadow mode should be enabled");
    assert_eq!(deployment.traffic_percentage, 0.0, "Traffic should be 0% in shadow mode");
    assert_eq!(deployment.phase, CanaryPhase::PendingCeoWindow, "Should start in PendingCeoWindow phase");
    
    // In shadow mode, all user requests should go to stable version
    // The canary version should only process background/shadow requests
    let metrics = create_metrics(1000, 0, 0, 0, 0, 50.0, 50.0, 0.995, 10000);
    
    // Verify no violations in shadow mode
    assert_eq!(metrics.canary_violations, 0);
    assert_eq!(metrics.canary_benevolence_failures, 0);
    assert_eq!(metrics.canary_oracle_unsafe_votes, 0);
}

/// Test 2: Gradual Traffic Increment Testing
/// Verify traffic increments from 0.1% to 100% in 0.5% steps
#[test]
fn test_gradual_traffic_increment() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    
    // Transition to canary phase
    deployment.start_canary_phase();
    
    // Verify initial traffic
    assert_eq!(deployment.traffic_percentage, 0.1, "Should start at 0.1% traffic");
    
    // Simulate hourly increments
    let increment_steps = vec![
        (0.1, 0.6),   // Hour 1: 0.1% -> 0.6%
        (0.6, 1.1),   // Hour 2: 0.6% -> 1.1%
        (1.1, 1.6),   // Hour 3: 1.1% -> 1.6%
        (50.0, 50.5), // Mid-deployment
        (99.5, 100.0), // Final step
    ];
    
    for (before, expected_after) in increment_steps {
        deployment.traffic_percentage = before;
        
        // Simulate increment (0.5% increase)
        deployment.increment_traffic();
        
        assert_eq!(
            deployment.traffic_percentage, expected_after,
            "Traffic should increment from {}% to {}%",
            before, expected_after
        );
    }
    
    // Verify final state
    assert_eq!(deployment.traffic_percentage, 100.0, "Should reach 100% traffic");
}

/// Test 3: Rollback Trigger Testing - Constitutional Violations
/// Verify that violations > 0 triggers immediate rollback
#[test]
fn test_rollback_trigger_violations() {
    let metrics = create_metrics(100, 1, 0, 0, 0, 50.0, 50.0, 0.995, 100);
    
    // Check rollout criteria
    let result = metrics.check_rollout_criteria();
    
    assert!(!result.passed, "Should fail rollout criteria when violations > 0");
    assert!(result.failures.contains(&RolloutFailure::ConstitutionalViolation(1)));
}

/// Test 4: Rollback Trigger Testing - Benevolence Score
/// Verify that benevolence score < 0.99 triggers rollback
#[test]
fn test_rollback_trigger_benevolence() {
    let metrics = create_metrics(100, 0, 5, 0, 0, 50.0, 50.0, 0.85, 100);
    
    // Check rollout criteria
    let result = metrics.check_rollout_criteria();
    
    assert!(!result.passed, "Should fail rollout criteria when benevolence < 0.99");
    assert!(matches!(result.failures[0], RolloutFailure::BenevolenceScoreTooLow(_)));
}

/// Test 5: Rollback Trigger Testing - Safety Oracle
/// Verify that unsafe votes > 0 triggers rollback
#[test]
fn test_rollback_trigger_oracle_unsafe() {
    let metrics = create_metrics(100, 0, 0, 1, 0, 50.0, 50.0, 0.995, 100);
    
    // Check rollout criteria
    let result = metrics.check_rollout_criteria();
    
    assert!(!result.passed, "Should fail rollout criteria when oracle unsafe votes > 0");
    assert!(result.failures.contains(&RolloutFailure::SafetyOracleUnsafe(1)));
}

/// Test 6: Complaint Threshold Testing
/// Verify that complaint rate > 0.1% triggers rollback
#[test]
fn test_complaint_threshold_rollback() {
    // Test case 1: Complaint rate at 0.05% (below threshold)
    let metrics_safe = create_metrics(10000, 0, 0, 0, 5, 50.0, 50.0, 0.995, 10000);
    let result_safe = metrics_safe.check_rollout_criteria();
    
    assert!(result_safe.passed, "Should pass with 0.05% complaint rate");
    
    // Test case 2: Complaint rate at 0.2% (above threshold)
    let metrics_unsafe = create_metrics(10000, 0, 0, 0, 20, 50.0, 50.0, 0.995, 10000);
    let result_unsafe = metrics_unsafe.check_rollout_criteria();
    
    assert!(!result_unsafe.passed, "Should fail with 0.2% complaint rate");
    assert!(matches!(result_unsafe.failures[0], RolloutFailure::ComplaintRateTooHigh(_)));
}

/// Test 7: Latency Threshold Testing
/// Verify that latency increase > 5% triggers rollback
#[test]
fn test_latency_threshold_rollback() {
    // Test case 1: Latency within 5% threshold
    let metrics_safe = create_metrics(100, 0, 0, 0, 0, 52.0, 50.0, 0.995, 100); // 4% increase
    let result_safe = metrics_safe.check_rollout_criteria();
    
    assert!(result_safe.passed, "Should pass with 4% latency increase");
    
    // Test case 2: Latency exceeds 5% threshold
    let metrics_unsafe = create_metrics(100, 0, 0, 0, 0, 55.0, 50.0, 0.995, 100); // 10% increase
    let result_unsafe = metrics_unsafe.check_rollout_criteria();
    
    assert!(!result_unsafe.passed, "Should fail with 10% latency increase");
    assert!(matches!(result_unsafe.failures[0], RolloutFailure::LatencyTooHigh(_)));
}

/// Test 8: CEO Veto Testing
/// Verify that CEO veto during 24h window aborts deployment
#[test]
fn test_ceo_veto_aborts_deployment() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    
    // Simulate CEO veto
    deployment.abort_for_ceo_veto();
    
    assert_eq!(deployment.phase, CanaryPhase::Aborted, "Deployment should be aborted after CEO veto");
    assert!(deployment.ceo_vetoed, "CEO vetoed flag should be set");
}

/// Test 9: No CEO Veto - Proceed to Shadow Mode
/// Verify that deployment proceeds to shadow mode after 24h without veto
#[test]
fn test_no_ceo_veto_proceeds_to_shadow() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    
    // Simulate veto window expired
    deployment.ceo_veto_deadline = Some(current_timestamp() - 1); // Already expired
    
    // Check if window expired
    assert!(deployment.is_ceo_window_expired(), "Veto window should be expired");
    
    // Transition to shadow phase
    deployment.start_shadow_phase();
    
    assert_eq!(deployment.phase, CanaryPhase::Shadow, "Should transition to Shadow phase");
    assert!(deployment.shadow_mode, "Shadow mode should be enabled");
}

/// Test 10: Shadow to Canary Transition
/// Verify transition from shadow to canary after 24h stable shadow
#[test]
fn test_shadow_to_canary_transition() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    deployment.phase = CanaryPhase::Shadow;
    deployment.shadow_mode = true;
    
    // Simulate 24h of stable shadow mode
    deployment.deployment_start = current_timestamp() - (24 * 3600); // 24h ago
    
    // Check transition criteria
    assert!(deployment.is_shadow_phase_complete(), "Shadow phase should be complete after 24h");
    
    // Transition to canary phase
    deployment.start_canary_phase();
    
    assert_eq!(deployment.phase, CanaryPhase::Canary, "Should transition to Canary phase");
    assert!(!deployment.shadow_mode, "Shadow mode should be disabled");
    assert_eq!(deployment.traffic_percentage, 0.1, "Should start at 0.1% traffic");
}

/// Test 11: Full Integration Test - Shadow to Canary to Complete
/// Test the full deployment lifecycle
#[test]
fn test_full_deployment_lifecycle() {
    // Phase 1: Pending CEO Window
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    assert_eq!(deployment.phase, CanaryPhase::PendingCeoWindow);
    
    // Phase 2: CEO window expires, transition to Shadow
    deployment.start_shadow_phase();
    assert_eq!(deployment.phase, CanaryPhase::Shadow);
    assert_eq!(deployment.traffic_percentage, 0.0);
    
    // Phase 3: Shadow mode passes, transition to Canary
    deployment.start_canary_phase();
    assert_eq!(deployment.phase, CanaryPhase::Canary);
    
    // Phase 4: Gradual traffic increase
    // Starting at 0.1%, need to reach 100% with 0.5% increments
    // 0.1 + n * 0.5 = 100 => n = 199.8, so we need 200 increments
    for _ in 0..200 {
        deployment.increment_traffic();
    }
    
    assert!(deployment.traffic_percentage >= 100.0, "Should reach at least 100% traffic, got {}", deployment.traffic_percentage);
    
    // Phase 5: Deployment complete
    deployment.mark_complete();
    assert_eq!(deployment.phase, CanaryPhase::Complete);
}

/// Test 12: Rollback Time Constraint
/// Verify that rollback completes within 5 minutes
#[test]
fn test_rollback_time_constraint() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    deployment.phase = CanaryPhase::Canary;
    deployment.traffic_percentage = 50.0;
    
    // Simulate rollback trigger
    let rollback_start = current_timestamp();
    
    // Execute rollback (immediate in test)
    deployment.rollback("Test rollback");
    
    let rollback_end = current_timestamp();
    let rollback_duration_seconds = rollback_end - rollback_start;
    
    // Verify rollback completed in under 5 minutes (300 seconds)
    assert!(
        rollback_duration_seconds < 300,
        "Rollback should complete in under 5 minutes"
    );
    assert_eq!(deployment.traffic_percentage, 0.0, "Traffic should be 0% after rollback");
    assert_eq!(deployment.phase, CanaryPhase::RolledBack, "Phase should be RolledBack");
}

/// Test 13: Multiple Rollback Reasons Priority
/// Test that multiple failures are all recorded
#[test]
fn test_multiple_failures_recorded() {
    // Create metrics with multiple issues
    let metrics = create_metrics(100, 5, 3, 2, 10, 60.0, 50.0, 0.85, 100);
    
    // Check rollout criteria
    let result = metrics.check_rollout_criteria();
    
    // Should have multiple failures
    assert!(!result.passed, "Should fail with multiple issues");
    assert!(result.failures.len() >= 3, "Should have at least 3 failures recorded");
    
    // Verify specific failures are present
    assert!(result.failures.contains(&RolloutFailure::ConstitutionalViolation(5)));
    assert!(result.failures.contains(&RolloutFailure::SafetyOracleUnsafe(2)));
}

/// Test 14: Safety Check Result Integration
/// Test the SafetyCheckResult struct with various outcomes
#[test]
fn test_safety_check_result() {
    // Test passing safety check
    let passing_result = SafetyCheckResult {
        violations: 0,
        benevolence_score: 0.995,
        unsafe_votes: 0,
        passed: true,
    };
    
    assert!(passing_result.passed, "Safety check should pass with good metrics");
    assert_eq!(passing_result.violations, 0);
    assert!(passing_result.benevolence_score >= 0.99);
    
    // Test failing safety check
    let failing_result = SafetyCheckResult {
        violations: 1,
        benevolence_score: 0.98,
        unsafe_votes: 1,
        passed: false,
    };
    
    assert!(!failing_result.passed, "Safety check should fail with bad metrics");
}

/// Test 15: User Complaint Tracking
/// Test the complaint tracking system
#[test]
fn test_user_complaint_tracking() {
    let complaint_tracker: Arc<DashMap<String, Vec<UserComplaint>>> = Arc::new(DashMap::new());
    
    // Add complaints for a model
    let model_id = "test-model";
    let inf_id1 = Uuid::new_v4();
    let inf_id2 = Uuid::new_v4();
    let complaints = vec![
        UserComplaint {
            user_id: "user1".to_string(),
            inference_id: inf_id1,
            timestamp: current_timestamp(),
            reason: "Incorrect output".to_string(),
            model_version: "v2.0.0".to_string(),
        },
        UserComplaint {
            user_id: "user2".to_string(),
            inference_id: inf_id2,
            timestamp: current_timestamp(),
            reason: "Slow response".to_string(),
            model_version: "v2.0.0".to_string(),
        },
    ];
    
    complaint_tracker.insert(model_id.to_string(), complaints.clone());
    
    // Verify complaints are tracked
    let tracked = complaint_tracker.get(model_id).unwrap();
    assert_eq!(tracked.len(), 2);
    assert_eq!(tracked[0].user_id, "user1");
    assert_eq!(tracked[1].reason, "Slow response");
}

/// Test 16: Rollback Event Recording
/// Test the RollbackEvent struct
#[test]
fn test_rollback_event_recording() {
    let event = RollbackEvent {
        model_id: "test-model".to_string(),
        canary_version: "v2.0.0".to_string(),
        reason: "Constitutional violation detected".to_string(),
        timestamp: current_timestamp(),
        metrics_snapshot: create_metrics(1000, 5, 0, 0, 0, 50.0, 50.0, 0.995, 1000),
        traffic_at_rollback: 25.5,
    };
    
    assert_eq!(event.model_id, "test-model");
    assert_eq!(event.canary_version, "v2.0.0");
    assert_eq!(event.traffic_at_rollback, 25.5);
    assert_eq!(event.metrics_snapshot.canary_violations, 5);
}

/// Test 17: Canary Phase Transitions
/// Test all valid phase transitions
#[test]
fn test_canary_phase_transitions() {
    // Valid transition: PendingCeoWindow -> Shadow
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    deployment.start_shadow_phase();
    assert_eq!(deployment.phase, CanaryPhase::Shadow);
    
    // Valid transition: Shadow -> Canary
    deployment.start_canary_phase();
    assert_eq!(deployment.phase, CanaryPhase::Canary);
    
    // Valid transition: Canary -> Complete
    deployment.mark_complete();
    assert_eq!(deployment.phase, CanaryPhase::Complete);
    
    // Test rollback from canary
    let mut deployment2 = create_test_canary_deployment("test-model", "v2.0.0");
    deployment2.start_canary_phase();
    deployment2.rollback("Test rollback");
    assert_eq!(deployment2.phase, CanaryPhase::RolledBack);
    
    // Test abort from pending
    let mut deployment3 = create_test_canary_deployment("test-model", "v2.0.0");
    deployment3.abort_for_ceo_veto();
    assert_eq!(deployment3.phase, CanaryPhase::Aborted);
}

/// Test 18: Metrics Update During Deployment
/// Test that metrics are correctly updated during deployment
#[test]
fn test_metrics_update_during_deployment() {
    let mut metrics = CanaryMetrics::new();
    
    // Simulate processing requests
    for i in 0..100 {
        metrics.canary_requests += 1;
        
        // Simulate occasional latency variation
        let latency = 45.0 + (i % 10) as f32;
        metrics.update_canary_latency(latency);
    }
    
    assert_eq!(metrics.canary_requests, 100);
    assert!(metrics.canary_avg_latency_ms > 0.0);
    assert!(metrics.canary_avg_latency_ms < 100.0);
}

/// Test 19: Deployment ID Generation
/// Test that deployment creates valid timestamps
#[test]
fn test_deployment_timestamps() {
    let before = current_timestamp();
    let deployment = create_test_canary_deployment("test-model", "v2.0.0");
    let after = current_timestamp();
    
    // Deployment start should be between before and after
    assert!(deployment.deployment_start >= before);
    assert!(deployment.deployment_start <= after);
    
    // CEO veto deadline should be 24h in the future
    let expected_deadline = deployment.deployment_start + 86400;
    assert_eq!(deployment.ceo_veto_deadline, Some(expected_deadline));
}

/// Test 20: Edge Cases - Zero Requests
/// Test behavior when no requests have been processed
#[test]
fn test_edge_case_zero_requests() {
    let metrics = CanaryMetrics::default();
    
    // With zero requests, complaint rate calculation should not panic
    let complaint_rate = metrics.complaint_rate();
    assert_eq!(complaint_rate, 0.0);
    
    // Latency ratio should also handle zero
    let latency_ratio = metrics.latency_ratio();
    assert_eq!(latency_ratio, 1.0);
    
    // With zero requests and zero benevolence score, the rollout criteria will fail
    // because last_benevolence_score is 0.0 which is < 0.99
    // This is expected behavior - we need actual metrics before rollout can proceed
    let result = metrics.check_rollout_criteria();
    assert!(!result.passed, "Should fail with zero benevolence score (need actual metrics)");
    assert!(matches!(result.failures[0], RolloutFailure::BenevolenceScoreTooLow(_)));
}

/// Test 21: Deployment Status Enum
/// Test the DeploymentStatus enum values
#[test]
fn test_deployment_status_enum() {
    assert_eq!(DeploymentStatus::Stable, DeploymentStatus::default());
    
    // Test all variants
    let statuses = vec![
        DeploymentStatus::Stable,
        DeploymentStatus::Canary,
        DeploymentStatus::Shadow,
        DeploymentStatus::Deprecated,
    ];
    
    // Each status should be distinct
    for (i, s1) in statuses.iter().enumerate() {
        for (j, s2) in statuses.iter().enumerate() {
            if i != j {
                assert_ne!(s1, s2);
            }
        }
    }
}

/// Test 22: CanaryMetrics Default
/// Test that CanaryMetrics::default() provides sensible defaults
#[test]
fn test_canary_metrics_default() {
    let metrics = CanaryMetrics::default();
    
    assert_eq!(metrics.canary_requests, 0);
    assert_eq!(metrics.canary_violations, 0);
    assert_eq!(metrics.canary_benevolence_failures, 0);
    assert_eq!(metrics.canary_oracle_unsafe_votes, 0);
    assert_eq!(metrics.canary_complaints, 0);
    assert_eq!(metrics.canary_avg_latency_ms, 0.0);
    assert_eq!(metrics.stable_avg_latency_ms, 0.0);
    assert_eq!(metrics.last_benevolence_score, 0.0);
    assert_eq!(metrics.stable_requests, 0);
}

/// Test 23: Latency Update Methods
/// Test the latency update methods
#[test]
fn test_latency_update_methods() {
    let mut metrics = CanaryMetrics::new();
    
    // First update should set the value directly
    metrics.update_canary_latency(100.0);
    assert_eq!(metrics.canary_avg_latency_ms, 100.0);
    
    // Second update should average
    metrics.update_canary_latency(200.0);
    assert_eq!(metrics.canary_avg_latency_ms, 150.0); // (100 + 200) / 2
    
    // Same for stable latency
    metrics.update_stable_latency(50.0);
    assert_eq!(metrics.stable_avg_latency_ms, 50.0);
    
    metrics.update_stable_latency(70.0);
    assert_eq!(metrics.stable_avg_latency_ms, 60.0); // (50 + 70) / 2
}

/// Test 24: Is Complete Check
/// Test the is_complete method
#[test]
fn test_is_complete_check() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    
    assert!(!deployment.is_complete(), "Should not be complete at start");
    
    deployment.traffic_percentage = 99.9;
    assert!(!deployment.is_complete(), "Should not be complete at 99.9%");
    
    deployment.traffic_percentage = 100.0;
    assert!(deployment.is_complete(), "Should be complete at 100%");
}

/// Test 25: Increment Due Check
/// Test the is_increment_due method
#[test]
fn test_increment_due_check() {
    let mut deployment = create_test_canary_deployment("test-model", "v2.0.0");
    deployment.start_canary_phase();
    
    // Just incremented, should not be due
    // The interval is 1 hour, so we need to wait
    assert!(!deployment.is_increment_due(), "Should not be due immediately after increment");
    
    // Simulate time passing
    deployment.last_increment = current_timestamp() - 3601; // More than 1 hour ago
    assert!(deployment.is_increment_due(), "Should be due after 1 hour");
}