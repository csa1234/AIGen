use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use genesis::{check_shutdown, CEO_WALLET};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ModelError, ModelRegistry};
use blockchain_core::transaction::Transaction;
use blockchain_core::state::ChainState;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubscriptionTier {
    Free,
    Basic,
    Pro,
    Unlimited,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeatureFlag {
    BasicInference,
    AdvancedInference,
    ModelTraining,
    BatchProcessing,
    PriorityQueue,
    ApiAccess,
    CustomModels,
}

impl SubscriptionTier {
    pub fn as_u8(self) -> u8 {
        match self {
            SubscriptionTier::Free => 0,
            SubscriptionTier::Basic => 1,
            SubscriptionTier::Pro => 2,
            SubscriptionTier::Unlimited => 3,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SubscriptionTier::Free),
            1 => Some(SubscriptionTier::Basic),
            2 => Some(SubscriptionTier::Pro),
            3 => Some(SubscriptionTier::Unlimited),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierConfig {
    pub tier: SubscriptionTier,
    pub monthly_cost: u64,
    pub request_limit: u64,
    pub window_seconds: i64,
    pub billing_cycle_seconds: i64,
    pub grace_period_seconds: i64,
    pub max_context_size: u32,
    pub features: Vec<FeatureFlag>,
    pub allowed_models: Vec<String>,
    pub minimum_tier_for_access: Option<SubscriptionTier>,
}

impl TierConfig {
    pub fn is_unlimited(&self) -> bool {
        self.request_limit == u64::MAX
    }

    pub fn validate_context_size(&self, requested_size: u32) -> bool {
        requested_size <= self.max_context_size
    }

    pub fn validate_feature_access(&self, feature: &FeatureFlag) -> bool {
        self.features.contains(feature)
    }

    pub fn can_access_model(&self, model_id: &str) -> bool {
        self.allowed_models.is_empty() || self.allowed_models.contains(&model_id.to_string())
    }

    pub fn meets_minimum_tier(&self, _required_tier: SubscriptionTier) -> bool {
        if let Some(min_tier) = self.minimum_tier_for_access {
            self.tier >= min_tier
        } else {
            true
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Expired,
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscriptionPayload {
    pub tier: SubscriptionTier,
    pub duration_months: u32,
    pub user_address: String,
}

#[derive(Debug, Error)]
pub enum PaymentValidationError {
    #[error("invalid payload format")]
    InvalidPayload,
    #[error("tier mismatch: expected {expected:?}, got {actual:?}")]
    TierMismatch { expected: SubscriptionTier, actual: SubscriptionTier },
    #[error("amount mismatch: expected {expected}, got {actual}")]
    AmountMismatch { expected: u64, actual: u64 },
    #[error("receiver mismatch: expected {expected}, got {actual}")]
    ReceiverMismatch { expected: String, actual: String },
    #[error("invalid duration: {duration}")]
    InvalidDuration { duration: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaEntry {
    pub timestamp: i64,
    pub user_address: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct QuotaTracker {
    entries: Vec<QuotaEntry>,
    max_entries: usize,
}

impl QuotaTracker {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn record_request(&mut self, user_address: Option<&str>, ip_address: Option<&str>, now: i64) {
        let entry = QuotaEntry {
            timestamp: now,
            user_address: user_address.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
        };
        self.entries.push(entry);
        self.trim_old_entries(now);
    }

    pub fn count_requests_in_window(&self, user_address: Option<&str>, ip_address: Option<&str>, now: i64, window_seconds: i64) -> usize {
        let cutoff = now.saturating_sub(window_seconds);
        self.entries.iter()
            .filter(|entry| {
                entry.timestamp >= cutoff && (
                    user_address.map(|addr| entry.user_address.as_ref().map_or(false, |e| e == addr)).unwrap_or(false) ||
                    ip_address.map(|ip| entry.ip_address.as_ref().map_or(false, |e| e == ip)).unwrap_or(false)
                )
            })
            .count()
    }

    pub fn is_within_limit(&self, user_address: Option<&str>, ip_address: Option<&str>, now: i64, window_seconds: i64, limit: u64) -> bool {
        let count = self.count_requests_in_window(user_address, ip_address, now, window_seconds);
        count < limit as usize
    }

    fn trim_old_entries(&mut self, now: i64) {
        let cutoff = now.saturating_sub(30 * 24 * 60 * 60); // Keep 30 days of data
        self.entries.retain(|entry| entry.timestamp >= cutoff);
        
        // Also limit by max_entries to bound memory
        if self.entries.len() > self.max_entries {
            let excess = self.entries.len() - self.max_entries;
            self.entries.drain(0..excess);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentRecord {
    pub amount: u64,
    pub timestamp: i64,
    pub status: PaymentStatus,
    pub reference: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PaymentStatus {
    Pending,
    Confirmed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscription {
    pub user_address: String,
    pub tier: SubscriptionTier,
    pub start_timestamp: i64,
    pub expiry_timestamp: i64,
    pub requests_used: u64,
    pub last_reset_timestamp: i64,
    pub auto_renew: bool,
    pub status: SubscriptionStatus,
    pub payments: Vec<PaymentRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_at: i64,
    pub retry_after_secs: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessLogEntry {
    pub user_address: String,
    pub model_id: String,
    pub tier: SubscriptionTier,
    pub timestamp: i64,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum TierError {
    #[error("shutdown active")]
    Shutdown,
    #[error("invalid tier")]
    InvalidTier,
    #[error("subscription not found")]
    SubscriptionNotFound,
    #[error("subscription inactive")]
    SubscriptionInactive,
    #[error("payment failed")]
    PaymentFailed,
    #[error("rate limited")]
    RateLimited,
    #[error("model registry error: {0}")]
    Registry(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentOutcome {
    pub status: PaymentStatus,
    pub reference: Option<String>,
}

pub trait PaymentProvider: Send + Sync {
    fn charge(&self, user_address: &str, amount: u64) -> Result<PaymentOutcome, TierError>;
}

#[derive(Clone, Default)]
pub struct DefaultPaymentProvider;

impl PaymentProvider for DefaultPaymentProvider {
    fn charge(&self, _user_address: &str, _amount: u64) -> Result<PaymentOutcome, TierError> {
        Ok(PaymentOutcome {
            status: PaymentStatus::Confirmed,
            reference: None,
        })
    }
}

pub struct TierManager {
    configs: HashMap<SubscriptionTier, TierConfig>,
    subscriptions: DashMap<String, Subscription>,
    payment_provider: Arc<dyn PaymentProvider>,
    access_logs: Mutex<VecDeque<AccessLogEntry>>,
    max_log_entries: usize,
    quota_tracker: Mutex<QuotaTracker>,
    chain_state: Option<Arc<ChainState>>,
}

pub fn validate_subscription_payment(
    transaction: &Transaction,
    expected_tier: SubscriptionTier,
    expected_receiver: &str,
    tier_configs: &HashMap<SubscriptionTier, TierConfig>,
) -> Result<SubscriptionPayload, PaymentValidationError> {
    // Parse payload
    let payload = transaction.payload.as_ref().ok_or(PaymentValidationError::InvalidPayload)?;
    let subscription_payload: SubscriptionPayload = serde_json::from_slice(payload)
        .map_err(|_| PaymentValidationError::InvalidPayload)?;
    
    // Validate tier
    if subscription_payload.tier != expected_tier {
        return Err(PaymentValidationError::TierMismatch {
            expected: expected_tier,
            actual: subscription_payload.tier,
        });
    }
    
    // Validate receiver address
    if transaction.receiver != expected_receiver {
        return Err(PaymentValidationError::ReceiverMismatch {
            expected: expected_receiver.to_string(),
            actual: transaction.receiver.clone(),
        });
    }
    
    // Validate duration
    if subscription_payload.duration_months == 0 || subscription_payload.duration_months > 12 {
        return Err(PaymentValidationError::InvalidDuration {
            duration: subscription_payload.duration_months,
        });
    }
    
    // Validate amount
    let tier_config = tier_configs.get(&expected_tier)
        .ok_or(PaymentValidationError::TierMismatch {
            expected: expected_tier,
            actual: expected_tier, // This shouldn't happen but we need a valid tier
        })?;
    
    let expected_amount = tier_config.monthly_cost.saturating_mul(subscription_payload.duration_months as u64);
    let actual_amount = transaction.amount.value();
    
    if actual_amount != expected_amount {
        return Err(PaymentValidationError::AmountMismatch {
            expected: expected_amount,
            actual: actual_amount,
        });
    }
    
    Ok(subscription_payload)
}

impl TierManager {
    pub fn with_default_configs(payment_provider: Arc<dyn PaymentProvider>) -> Self {
        Self::new(default_tier_configs(), payment_provider, None)
    }

    pub fn with_chain_state(
        configs: HashMap<SubscriptionTier, TierConfig>,
        payment_provider: Arc<dyn PaymentProvider>,
        chain_state: Arc<ChainState>,
    ) -> Self {
        Self::new(configs, payment_provider, Some(chain_state))
    }

    pub fn new(
        configs: HashMap<SubscriptionTier, TierConfig>,
        payment_provider: Arc<dyn PaymentProvider>,
        chain_state: Option<Arc<ChainState>>,
    ) -> Self {
        Self {
            configs,
            subscriptions: DashMap::new(),
            payment_provider,
            access_logs: Mutex::new(VecDeque::new()),
            max_log_entries: 2000,
            quota_tracker: Mutex::new(QuotaTracker::new(10000)),
            chain_state,
        }
    }

    pub fn load_from_chain_state(&self) -> Result<usize, TierError> {
        if let Some(chain_state) = &self.chain_state {
            let chain_subscriptions: Vec<(String, blockchain_core::state::SubscriptionState)> = chain_state.list_subscriptions();
            let mut loaded = 0;
            
            for (address, sub_state) in chain_subscriptions {
                let tier = SubscriptionTier::from_u8(sub_state.tier)
                    .ok_or(TierError::InvalidTier)?;
                
                let subscription = Subscription {
                    user_address: address.clone(),
                    tier,
                    start_timestamp: sub_state.start_timestamp,
                    expiry_timestamp: sub_state.expiry_timestamp,
                    requests_used: sub_state.requests_used,
                    last_reset_timestamp: sub_state.last_reset_timestamp,
                    auto_renew: sub_state.auto_renew,
                    status: SubscriptionStatus::Active, // Assume active if in chain state
                    payments: Vec::new(),
                };
                
                self.subscriptions.insert(address, subscription);
                loaded += 1;
            }
            
            println!("subscriptions loaded from chain state");
            Ok(loaded)
        } else {
            Ok(0)
        }
    }

    fn sync_subscription_to_chain(&self, user_address: &str, subscription: &Subscription) {
        if let Some(chain_state) = &self.chain_state {
            let sub_state = blockchain_core::state::SubscriptionState {
                user_address: user_address.to_string(),
                tier: subscription.tier.as_u8(),
                start_timestamp: subscription.start_timestamp,
                expiry_timestamp: subscription.expiry_timestamp,
                requests_used: subscription.requests_used,
                last_reset_timestamp: subscription.last_reset_timestamp,
                auto_renew: subscription.auto_renew,
            };
            
            chain_state.set_subscription(user_address.to_string(), sub_state);
        }
    }

    pub fn get_config(&self, tier: SubscriptionTier) -> Result<&TierConfig, TierError> {
        self.configs.get(&tier).ok_or(TierError::InvalidTier)
    }

    pub fn subscribe_user_with_transaction(
        &self,
        transaction: &Transaction,
        expected_receiver: &str,
        now: i64,
    ) -> Result<Subscription, TierError> {
        self.ensure_running()?;
        
        // Validate payment transaction
        let payload = validate_subscription_payment(
            transaction,
            transaction.payload.as_ref()
                .and_then(|p| serde_json::from_slice::<SubscriptionPayload>(p).ok())
                .map(|p| p.tier)
                .unwrap_or(SubscriptionTier::Free),
            expected_receiver,
            &self.configs,
        ).map_err(|_e| TierError::PaymentFailed)?;
        
        let tier = self.normalize_tier(&payload.user_address, payload.tier);
        let config = self.get_config(tier)?;
        let start = now;
        let expiry = now.saturating_add(config.billing_cycle_seconds.saturating_mul(payload.duration_months as i64));
        let subscription = Subscription {
            user_address: payload.user_address.clone(),
            tier,
            start_timestamp: start,
            expiry_timestamp: expiry,
            requests_used: 0,
            last_reset_timestamp: now,
            auto_renew: false, // Transaction-based subscriptions don't auto-renew by default
            status: SubscriptionStatus::Active,
            payments: vec![PaymentRecord {
                amount: transaction.amount.value(),
                timestamp: now,
                status: PaymentStatus::Confirmed,
                reference: Some(format!("tx:{}", hex::encode(&transaction.tx_hash.0))),
            }],
        };
        self.subscriptions
            .insert(payload.user_address.clone(), subscription.clone());
        
        // Sync to chain state
        self.sync_subscription_to_chain(&payload.user_address, &subscription);
        
        println!(
            "user_address = {}, tier = {:?}, subscription created via transaction",
            payload.user_address,
            subscription.tier
        );
        Ok(subscription)
    }

    pub fn subscribe_user(
        &self,
        user_address: &str,
        tier: SubscriptionTier,
        auto_renew: bool,
        now: i64,
    ) -> Result<Subscription, TierError> {
        self.ensure_running()?;
        let tier = self.normalize_tier(user_address, tier);
        let config = self.get_config(tier)?;
        let start = now;
        let expiry = now.saturating_add(config.billing_cycle_seconds);
        let subscription = Subscription {
            user_address: user_address.to_string(),
            tier,
            start_timestamp: start,
            expiry_timestamp: expiry,
            requests_used: 0,
            last_reset_timestamp: now,
            auto_renew,
            status: SubscriptionStatus::Active,
            payments: Vec::new(),
        };
        self.subscriptions
            .insert(user_address.to_string(), subscription.clone());
        
        // Sync to chain state
        self.sync_subscription_to_chain(user_address, &subscription);
        
        println!(
            "user_address = {}, tier = {:?}, subscription created",
            user_address,
            subscription.tier
        );
        Ok(subscription)
    }

    pub fn get_subscription(&self, user_address: &str) -> Option<Subscription> {
        self.subscriptions
            .get(user_address)
            .map(|entry| entry.value().clone())
    }

    pub fn cancel_subscription(&self, user_address: &str, _now: i64) -> Result<(), TierError> {
        self.ensure_running()?;
        let mut entry = self
            .subscriptions
            .get_mut(user_address)
            .ok_or(TierError::SubscriptionNotFound)?;
        entry.status = SubscriptionStatus::Canceled;
        entry.auto_renew = false;
        
        // Sync to chain state
        self.sync_subscription_to_chain(user_address, &entry.clone());
        
        println!("user_address = {}, subscription canceled", user_address);
        Ok(())
    }

    pub fn ensure_subscription_active(
        &self,
        user_address: &str,
        now: i64,
    ) -> Result<Subscription, TierError> {
        self.ensure_running()?;
        let user_address = user_address.to_string();
        if user_address == CEO_WALLET {
            return Ok(self.ensure_unlimited_subscription(&user_address, now));
        }
        let mut entry = self
            .subscriptions
            .get_mut(&user_address)
            .ok_or(TierError::SubscriptionNotFound)?;
        let config = self.get_config(entry.tier)?;
        self.refresh_subscription(&mut entry, config, now)?;
        if entry.status != SubscriptionStatus::Active {
            return Err(TierError::SubscriptionInactive);
        }
        if now.saturating_sub(entry.last_reset_timestamp) >= config.window_seconds {
            entry.requests_used = 0;
            entry.last_reset_timestamp = now;
        }
        let remaining = config
            .request_limit
            .saturating_sub(entry.requests_used);
        if remaining == 0 {
            return Err(TierError::SubscriptionInactive);
        }
        Ok(entry.clone())
    }

    pub fn record_request(
        &self,
        user_address: &str,
        ip_address: Option<&str>,
        now: i64,
    ) -> Result<RateLimitDecision, TierError> {
        self.ensure_running()?;
        let user_address = user_address.to_string();
        if user_address == CEO_WALLET {
            return Ok(RateLimitDecision {
                allowed: true,
                remaining: u64::MAX,
                reset_at: now,
                retry_after_secs: None,
            });
        }
        
        // Record in quota tracker
        {
            let mut tracker = self.quota_tracker.lock();
            tracker.record_request(Some(user_address.as_str()), ip_address, now);
        }
        
        let mut entry = self
            .subscriptions
            .get_mut(&user_address)
            .ok_or(TierError::SubscriptionNotFound)?;
        let config = self.get_config(entry.tier)?;
        self.refresh_subscription(&mut entry, config, now)?;
        if entry.status != SubscriptionStatus::Active {
            return Err(TierError::SubscriptionInactive);
        }
        if now.saturating_sub(entry.last_reset_timestamp) >= config.window_seconds {
            entry.requests_used = 0;
            entry.last_reset_timestamp = now;
        }
        let remaining = config
            .request_limit
            .saturating_sub(entry.requests_used);
        if remaining == 0 {
            let reset_at = entry.last_reset_timestamp.saturating_add(config.window_seconds);
            let retry = reset_at.saturating_sub(now);
            return Ok(RateLimitDecision {
                allowed: false,
                remaining: 0,
                reset_at,
                retry_after_secs: Some(retry),
            });
        }
        entry.requests_used = entry.requests_used.saturating_add(1);
        
        // Sync usage to chain state
        self.sync_subscription_to_chain(&user_address, &entry.clone());
        
        Ok(RateLimitDecision {
            allowed: true,
            remaining: remaining.saturating_sub(1),
            reset_at: entry.last_reset_timestamp.saturating_add(config.window_seconds),
            retry_after_secs: None,
        })
    }

    pub fn process_renewals(&self, now: i64) -> usize {
        let mut renewed = 0;
        for mut entry in self.subscriptions.iter_mut() {
            let user_address = entry.user_address.clone();
            let tier = entry.tier;
            let config = match self.get_config(tier) {
                Ok(cfg) => cfg.clone(),
                Err(_) => continue,
            };
            if entry.status == SubscriptionStatus::Canceled {
                continue;
            }
            if now < entry.expiry_timestamp {
                continue;
            }
            if entry.auto_renew {
                if self
                    .attempt_renewal(&mut entry, &config, now)
                    .is_ok()
                {
                    // Sync renewed subscription to chain state
                    self.sync_subscription_to_chain(&user_address, &entry.clone());
                    renewed += 1;
                    println!("user_address = {}, tier = {:?}, subscription renewed", user_address, tier);
                } else {
                    // Sync failed renewal to chain state
                    self.sync_subscription_to_chain(&user_address, &entry.clone());
                    eprintln!("user_address = {}, tier = {:?}, subscription renewal failed", user_address, tier);
                }
            } else {
                entry.status = SubscriptionStatus::Expired;
                // Sync expired subscription to chain state
                self.sync_subscription_to_chain(&user_address, &entry.clone());
                println!("user_address = {}, tier = {:?}, subscription expired", user_address, tier);
            }
        }
        renewed
    }

    pub fn can_access_model(
        &self,
        registry: &ModelRegistry,
        user_address: &str,
        model_id: &str,
        now: i64,
    ) -> Result<bool, TierError> {
        self.ensure_running()?;
        let subscription = self.ensure_subscription_active(user_address, now)?;
        let allowed = registry
            .model_accessible_for_tier(model_id, subscription.tier)
            .map_err(|err| TierError::Registry(err.to_string()))?;
        let reason = if allowed {
            "model allowed"
        } else {
            "tier too low for model"
        };
        self.record_access_log(user_address, model_id, subscription.tier, allowed, reason);
        Ok(allowed)
    }

    pub fn record_access_log(
        &self,
        user_address: &str,
        model_id: &str,
        tier: SubscriptionTier,
        allowed: bool,
        reason: &str,
    ) {
        let entry = AccessLogEntry {
            user_address: user_address.to_string(),
            model_id: model_id.to_string(),
            tier,
            timestamp: now_timestamp(),
            allowed,
            reason: reason.to_string(),
        };
        let mut logs = self.access_logs.lock();
        if logs.len() >= self.max_log_entries {
            logs.pop_front();
        }
        logs.push_back(entry);
    }

    pub fn recent_access_logs(&self, limit: usize) -> Vec<AccessLogEntry> {
        let logs = self.access_logs.lock();
        logs.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn ensure_running(&self) -> Result<(), TierError> {
        check_shutdown().map_err(|_| TierError::Shutdown)
    }

    fn normalize_tier(&self, user_address: &str, tier: SubscriptionTier) -> SubscriptionTier {
        if user_address == CEO_WALLET {
            SubscriptionTier::Unlimited
        } else {
            tier
        }
    }

    fn ensure_unlimited_subscription(&self, user_address: &str, now: i64) -> Subscription {
        let config = self
            .get_config(SubscriptionTier::Unlimited)
            .cloned()
            .unwrap_or(TierConfig {
                tier: SubscriptionTier::Unlimited,
                monthly_cost: 0,
                request_limit: u64::MAX,
                window_seconds: 1,
                billing_cycle_seconds: 2_592_000,
                grace_period_seconds: 86_400,
                max_context_size: 16384,
                features: vec![FeatureFlag::BasicInference, FeatureFlag::AdvancedInference, FeatureFlag::ModelTraining, FeatureFlag::BatchProcessing, FeatureFlag::PriorityQueue, FeatureFlag::ApiAccess, FeatureFlag::CustomModels],
                allowed_models: vec![],
                minimum_tier_for_access: None,
            });
        let subscription = Subscription {
            user_address: user_address.to_string(),
            tier: SubscriptionTier::Unlimited,
            start_timestamp: now,
            expiry_timestamp: now.saturating_add(config.billing_cycle_seconds),
            requests_used: 0,
            last_reset_timestamp: now,
            auto_renew: true,
            status: SubscriptionStatus::Active,
            payments: Vec::new(),
        };
        self.subscriptions
            .insert(user_address.to_string(), subscription.clone());
        subscription
    }

    fn refresh_subscription(
        &self,
        entry: &mut Subscription,
        config: &TierConfig,
        now: i64,
    ) -> Result<(), TierError> {
        if entry.status == SubscriptionStatus::Canceled {
            return Ok(());
        }
        if now < entry.expiry_timestamp {
            return Ok(());
        }
        if entry.auto_renew {
            self.attempt_renewal(entry, config, now)?;
        } else {
            entry.status = SubscriptionStatus::Expired;
        }
        Ok(())
    }

    fn attempt_renewal(
        &self,
        entry: &mut Subscription,
        config: &TierConfig,
        now: i64,
    ) -> Result<(), TierError> {
        let outcome = self
            .payment_provider
            .charge(&entry.user_address, config.monthly_cost)?;
        let record = PaymentRecord {
            amount: config.monthly_cost,
            timestamp: now,
            status: outcome.status.clone(),
            reference: outcome.reference,
        };
        entry.payments.push(record.clone());
        match outcome.status {
            PaymentStatus::Confirmed => {
                entry.status = SubscriptionStatus::Active;
                entry.start_timestamp = now;
                entry.expiry_timestamp = now.saturating_add(config.billing_cycle_seconds);
                entry.requests_used = 0;
                entry.last_reset_timestamp = now;
                Ok(())
            }
            PaymentStatus::Pending => {
                entry.status = SubscriptionStatus::PastDue;
                entry.expiry_timestamp = now.saturating_add(config.grace_period_seconds);
                Err(TierError::PaymentFailed)
            }
            PaymentStatus::Failed => {
                entry.status = SubscriptionStatus::PastDue;
                entry.expiry_timestamp = now.saturating_add(config.grace_period_seconds);
                Err(TierError::PaymentFailed)
            }
        }
    }
}

pub fn default_tier_configs() -> HashMap<SubscriptionTier, TierConfig> {
    let mut configs = HashMap::new();
    configs.insert(
        SubscriptionTier::Free,
        TierConfig {
            tier: SubscriptionTier::Free,
            monthly_cost: 0,
            request_limit: 10,
            window_seconds: 2_592_000,
            billing_cycle_seconds: 2_592_000,
            grace_period_seconds: 86_400,
            max_context_size: 2048,
            features: vec![FeatureFlag::BasicInference],
            allowed_models: vec![],
            minimum_tier_for_access: None,
        },
    );
    configs.insert(
        SubscriptionTier::Basic,
        TierConfig {
            tier: SubscriptionTier::Basic,
            monthly_cost: 50,
            request_limit: 100,
            window_seconds: 2_592_000,
            billing_cycle_seconds: 2_592_000,
            grace_period_seconds: 172_800,
            max_context_size: 4096,
            features: vec![FeatureFlag::BasicInference, FeatureFlag::ApiAccess],
            allowed_models: vec![],
            minimum_tier_for_access: Some(SubscriptionTier::Basic),
        },
    );
    configs.insert(
        SubscriptionTier::Pro,
        TierConfig {
            tier: SubscriptionTier::Pro,
            monthly_cost: 400,
            request_limit: 1000,
            window_seconds: 2_592_000,
            billing_cycle_seconds: 2_592_000,
            grace_period_seconds: 259_200,
            max_context_size: 8192,
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
            billing_cycle_seconds: 2_592_000,
            grace_period_seconds: 604_800,
            max_context_size: 16384,
            features: vec![FeatureFlag::BasicInference, FeatureFlag::AdvancedInference, FeatureFlag::ModelTraining, FeatureFlag::BatchProcessing, FeatureFlag::PriorityQueue, FeatureFlag::ApiAccess, FeatureFlag::CustomModels],
            allowed_models: vec![],
            minimum_tier_for_access: None,
        },
    );
    configs
}

pub fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn tier_from_address(manager: &TierManager, user_address: &str, now: i64) -> SubscriptionTier {
    if user_address == CEO_WALLET {
        return SubscriptionTier::Unlimited;
    }
    manager
        .ensure_subscription_active(user_address, now)
        .map(|s| s.tier)
        .unwrap_or(SubscriptionTier::Free)
}

pub fn resolve_model_access(
    manager: &TierManager,
    registry: &ModelRegistry,
    user_address: &str,
    model_id: &str,
    now: i64,
) -> Result<bool, TierError> {
    manager.can_access_model(registry, user_address, model_id, now)
}

impl From<ModelError> for TierError {
    fn from(err: ModelError) -> Self {
        TierError::Registry(err.to_string())
    }
}
