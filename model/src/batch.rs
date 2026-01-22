use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::Arc;

use dashmap::DashMap;
use genesis::{check_shutdown, CEO_WALLET};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{watch, Mutex, RwLock};
use uuid::Uuid;

use blockchain_core::transaction::Transaction;
use blockchain_core::{hash_data, ChainState, TxHash};

use crate::tiers::{FeatureFlag, PaymentValidationError};
use crate::{
    now_timestamp, InferenceEngine, InferenceError, InferenceOutput, InferenceTensor, ModelError,
    SubscriptionTier, TierError, TierManager,
};

const STANDARD_PRICE: u64 = 10;
const BATCH_PRICE: u64 = 5;
const ECONOMY_PRICE: u64 = 3;
const STANDARD_DELAY_SECONDS: i64 = 0;
const BATCH_DELAY_SECONDS: i64 = 86_400;
const ECONOMY_DELAY_SECONDS: i64 = 259_200;
const DEFAULT_PROCESSING_INTERVAL_MS: u64 = 1_000;
const DEFAULT_AVG_PROCESSING_TIME_MS: u64 = 1_000;
const RETRY_LIMIT: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 250;
const CANCELLATION_FEE_BPS: u64 = 500;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BatchPriority {
    Standard,
    Batch,
    Economy,
}

impl BatchPriority {
    pub fn delay_seconds(self) -> i64 {
        match self {
            BatchPriority::Standard => STANDARD_DELAY_SECONDS,
            BatchPriority::Batch => BATCH_DELAY_SECONDS,
            BatchPriority::Economy => ECONOMY_DELAY_SECONDS,
        }
    }

    pub fn base_price(self) -> u64 {
        match self {
            BatchPriority::Standard => STANDARD_PRICE,
            BatchPriority::Batch => BATCH_PRICE,
            BatchPriority::Economy => ECONOMY_PRICE,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            BatchPriority::Standard => 0,
            BatchPriority::Batch => 1,
            BatchPriority::Economy => 2,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(BatchPriority::Standard),
            1 => Some(BatchPriority::Batch),
            2 => Some(BatchPriority::Economy),
            _ => None,
        }
    }

    fn weight(self) -> u8 {
        match self {
            BatchPriority::Standard => 3,
            BatchPriority::Batch => 2,
            BatchPriority::Economy => 1,
        }
    }
}

impl std::fmt::Display for BatchPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BatchPriority::Standard => write!(f, "Standard"),
            BatchPriority::Batch => write!(f, "Batch"),
            BatchPriority::Economy => write!(f, "Economy"),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BatchJobStatus {
    Pending,
    Scheduled,
    Processing,
    Completed,
    Failed,
}

impl BatchJobStatus {
    pub fn as_u8(self) -> u8 {
        match self {
            BatchJobStatus::Pending => 0,
            BatchJobStatus::Scheduled => 1,
            BatchJobStatus::Processing => 2,
            BatchJobStatus::Completed => 3,
            BatchJobStatus::Failed => 4,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(BatchJobStatus::Pending),
            1 => Some(BatchJobStatus::Scheduled),
            2 => Some(BatchJobStatus::Processing),
            3 => Some(BatchJobStatus::Completed),
            4 => Some(BatchJobStatus::Failed),
            _ => None,
        }
    }
}

impl std::fmt::Display for BatchJobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BatchJobStatus::Pending => write!(f, "Pending"),
            BatchJobStatus::Scheduled => write!(f, "Scheduled"),
            BatchJobStatus::Processing => write!(f, "Processing"),
            BatchJobStatus::Completed => write!(f, "Completed"),
            BatchJobStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchRequest {
    pub request_id: String,
    pub user_address: String,
    pub priority: BatchPriority,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub model_id: String,
    pub input_data: Vec<u8>,
    pub payment_tx_hash: TxHash,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchJob {
    pub request_id: String,
    pub user_address: String,
    pub priority: BatchPriority,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub model_id: String,
    pub input_data: Vec<u8>,
    pub payment_tx_hash: TxHash,
    pub status: BatchJobStatus,
    pub result: Option<Vec<u8>>,
    pub error_message: Option<String>,
    pub processing_start_time: Option<i64>,
    pub completion_time: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchJobMetrics {
    pub total_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub average_processing_time_ms: u64,
    pub queue_depth_by_priority: HashMap<BatchPriority, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchPaymentPayload {
    pub request_id: String,
    pub user_address: String,
    pub priority: BatchPriority,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub model_id: String,
    pub input_data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchJobInfo {
    pub request_id: String,
    pub user_address: String,
    pub priority: BatchPriority,
    pub status: BatchJobStatus,
    pub submission_time: i64,
    pub scheduled_time: i64,
    pub processing_start_time: Option<i64>,
    pub completion_time: Option<i64>,
    pub model_id: String,
    pub result: Option<Vec<u8>>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionEstimate {
    pub scheduled_time: i64,
    pub estimated_completion_time: i64,
    pub estimated_wait_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueStats {
    pub jobs_by_priority: HashMap<BatchPriority, u64>,
    pub average_wait_time_ms: u64,
    pub estimates: HashMap<BatchPriority, CompletionEstimate>,
}

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("payment validation failed: {0}")]
    PaymentValidationFailed(#[from] PaymentValidationError),
    #[error("insufficient tier")]
    InsufficientTier,
    #[error("model not found")]
    ModelNotFound,
    #[error("inference failed: {0}")]
    InferenceFailed(String),
    #[error("job not found")]
    JobNotFound,
    #[error("queue full")]
    QueueFull,
    #[error("shutdown active")]
    Shutdown,
    #[error("invalid priority")]
    InvalidPriority,
}

impl From<TierError> for BatchError {
    fn from(err: TierError) -> Self {
        match err {
            TierError::SubscriptionInactive | TierError::SubscriptionNotFound => BatchError::InsufficientTier,
            TierError::Shutdown => BatchError::Shutdown,
            other => BatchError::InferenceFailed(other.to_string()),
        }
    }
}

impl From<ModelError> for BatchError {
    fn from(err: ModelError) -> Self {
        match err {
            ModelError::ModelNotFound => BatchError::ModelNotFound,
            ModelError::Shutdown => BatchError::Shutdown,
            other => BatchError::InferenceFailed(other.to_string()),
        }
    }
}

impl From<InferenceError> for BatchError {
    fn from(err: InferenceError) -> Self {
        match err {
            InferenceError::ModelNotFound => BatchError::ModelNotFound,
            InferenceError::Shutdown => BatchError::Shutdown,
            other => BatchError::InferenceFailed(other.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
struct ScheduledJob {
    job_id: String,
    scheduled_time: i64,
    priority: BatchPriority,
}

impl Ord for ScheduledJob {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.scheduled_time.cmp(&other.scheduled_time) {
            Ordering::Less => Ordering::Greater,
            Ordering::Greater => Ordering::Less,
            Ordering::Equal => {
                let p = self.priority.weight().cmp(&other.priority.weight());
                if p == Ordering::Equal {
                    other.job_id.cmp(&self.job_id)
                } else {
                    p
                }
            }
        }
    }
}

impl PartialOrd for ScheduledJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.job_id == other.job_id
            && self.scheduled_time == other.scheduled_time
            && self.priority == other.priority
    }
}

impl Eq for ScheduledJob {}

#[derive(Clone, Debug)]
pub struct VolumeDiscountTracker {
    user_jobs: DashMap<String, VecDeque<i64>>,
    discount_tiers: Vec<(u64, f64)>,
}

impl VolumeDiscountTracker {
    pub fn new(discount_tiers: Vec<(u64, f64)>) -> Self {
        Self {
            user_jobs: DashMap::new(),
            discount_tiers,
        }
    }

    pub fn default_tiers() -> Vec<(u64, f64)> {
        vec![(0, 0.0), (100, 0.10), (500, 0.20), (1000, 0.30)]
    }

    pub fn record_job(&self, user_address: &str, timestamp: i64) {
        let mut entry = self
            .user_jobs
            .entry(user_address.to_string())
            .or_default();
        entry.push_back(timestamp);
        Self::trim_old_entries(&mut entry, timestamp);
    }

    pub fn calculate_discount(&self, user_address: &str, now: i64) -> f64 {
        let count = self.jobs_in_window(user_address, now);
        let mut discount = 0.0;
        for (threshold, pct) in &self.discount_tiers {
            if count >= *threshold {
                discount = *pct;
            }
        }
        discount
    }

    pub fn apply_volume_discount(
        &self,
        tier_manager: &TierManager,
        user_address: &str,
        base_price: u64,
        now: i64,
    ) -> Result<u64, TierError> {
        let subscription = tier_manager.ensure_subscription_active(user_address, now)?;
        if subscription.tier != SubscriptionTier::Unlimited {
            return Ok(base_price);
        }
        let discount = self.calculate_discount(user_address, now);
        let discounted = (base_price as f64 * (1.0 - discount)).round() as u64;
        Ok(discounted)
    }

    fn jobs_in_window(&self, user_address: &str, now: i64) -> u64 {
        let mut count = 0;
        if let Some(entry) = self.user_jobs.get(user_address) {
            let cutoff = now.saturating_sub(30 * 24 * 60 * 60);
            count = entry.iter().filter(|ts| **ts >= cutoff).count() as u64;
        }
        count
    }

    fn trim_old_entries(entries: &mut VecDeque<i64>, now: i64) {
        let cutoff = now.saturating_sub(30 * 24 * 60 * 60);
        while let Some(front) = entries.front() {
            if *front >= cutoff {
                break;
            }
            entries.pop_front();
        }
    }
}

pub fn calculate_volume_discount(
    tracker: &VolumeDiscountTracker,
    user_address: &str,
    now: i64,
) -> f64 {
    tracker.calculate_discount(user_address, now)
}

pub struct BatchQueue {
    jobs: DashMap<String, BatchJob>,
    priority_queue: Arc<Mutex<BinaryHeap<ScheduledJob>>>,
    metrics: Arc<RwLock<BatchJobMetrics>>,
    tier_manager: Arc<TierManager>,
    inference_engine: Arc<InferenceEngine>,
    chain_state: Arc<ChainState>,
    discount_tracker: Arc<VolumeDiscountTracker>,
    receiver_address: String,
    cancellation_fee_bps: u64,
}

impl BatchQueue {
    pub fn new(
        tier_manager: Arc<TierManager>,
        inference_engine: Arc<InferenceEngine>,
        chain_state: Arc<ChainState>,
        discount_tracker: VolumeDiscountTracker,
    ) -> Self {
        let mut queue_depth_by_priority = HashMap::new();
        queue_depth_by_priority.insert(BatchPriority::Standard, 0);
        queue_depth_by_priority.insert(BatchPriority::Batch, 0);
        queue_depth_by_priority.insert(BatchPriority::Economy, 0);
        Self {
            jobs: DashMap::new(),
            priority_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            metrics: Arc::new(RwLock::new(BatchJobMetrics {
                total_jobs: 0,
                completed_jobs: 0,
                failed_jobs: 0,
                average_processing_time_ms: 0,
                queue_depth_by_priority,
            })),
            tier_manager,
            inference_engine,
            chain_state,
            discount_tracker: Arc::new(discount_tracker),
            receiver_address: CEO_WALLET.to_string(),
            cancellation_fee_bps: CANCELLATION_FEE_BPS,
        }
    }

    pub fn discount_tracker(&self) -> Arc<VolumeDiscountTracker> {
        self.discount_tracker.clone()
    }

    async fn submit_job(&self, mut request: BatchRequest) -> Result<String, BatchError> {
        check_shutdown().map_err(|_| BatchError::Shutdown)?;
        let now = now_timestamp();
        request.submission_time = now;
        request.scheduled_time = now.saturating_add(request.priority.delay_seconds());
        let job_id = if request.request_id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            request.request_id.clone()
        };
        let job = BatchJob {
            request_id: job_id.clone(),
            user_address: request.user_address.clone(),
            priority: request.priority,
            submission_time: request.submission_time,
            scheduled_time: request.scheduled_time,
            model_id: request.model_id.clone(),
            input_data: request.input_data.clone(),
            payment_tx_hash: request.payment_tx_hash,
            status: BatchJobStatus::Pending,
            result: None,
            error_message: None,
            processing_start_time: None,
            completion_time: None,
        };

        self.jobs.insert(job_id.clone(), job.clone());
        self.sync_job_to_chain(&job);
        self.discount_tracker.record_job(&job.user_address, now);

        {
            let mut queue = self.priority_queue.lock().await;
            queue.push(ScheduledJob {
                job_id: job_id.clone(),
                scheduled_time: job.scheduled_time,
                priority: job.priority,
            });
        }

        {
            let mut metrics = self.metrics.write().await;
            metrics.total_jobs = metrics.total_jobs.saturating_add(1);
            if let Some(entry) = metrics.queue_depth_by_priority.get_mut(&job.priority) {
                *entry = entry.saturating_add(1);
            }
        }

        eprintln!("batch job submitted: job_id={}, user={}, priority={}", job_id, request.user_address, request.priority);

        Ok(job_id)
    }

    pub async fn validate_and_submit(
        &self,
        transaction: &Transaction,
        model_id: String,
        input_data: Vec<u8>,
    ) -> Result<String, BatchError> {
        check_shutdown().map_err(|_| BatchError::Shutdown)?;
        let now = now_timestamp();
        let payload = validate_batch_payment(
            transaction,
            &self.receiver_address,
            None,
            model_id.clone(),
            input_data.clone(),
        )?;

        let subscription = self.tier_manager.ensure_subscription_active(&payload.user_address, now)?;
        let config = self.tier_manager.get_config(subscription.tier)?;
        if !config.validate_feature_access(&FeatureFlag::BatchProcessing) {
            return Err(BatchError::InsufficientTier);
        }

        if !self.inference_engine.model_exists(&payload.model_id)? {
            return Err(BatchError::ModelNotFound);
        }

        let registry = self.inference_engine.registry();
        let allowed = self
            .tier_manager
            .can_access_model(&registry, &payload.user_address, &payload.model_id, now)?;
        if !allowed {
            return Err(BatchError::InsufficientTier);
        }

        let expected_amount = self
            .discount_tracker
            .apply_volume_discount(
                &self.tier_manager,
                &payload.user_address,
                payload.priority.base_price(),
                now,
            )?;
        let actual_amount = transaction.amount.value();
        if actual_amount != expected_amount {
            return Err(PaymentValidationError::AmountMismatch {
                expected: expected_amount,
                actual: actual_amount,
            }
            .into());
        }

        let request = BatchRequest {
            request_id: payload.request_id.clone(),
            user_address: payload.user_address.clone(),
            priority: payload.priority,
            submission_time: now,
            scheduled_time: now.saturating_add(payload.priority.delay_seconds()),
            model_id: payload.model_id.clone(),
            input_data: payload.input_data.clone(),
            payment_tx_hash: transaction.tx_hash,
        };

        self.submit_job(request).await
    }

    pub async fn get_next_ready_job(&self) -> Result<Option<BatchJob>, BatchError> {
        check_shutdown().map_err(|_| BatchError::Shutdown)?;
        let now = now_timestamp();
        let mut queue = self.priority_queue.lock().await;
        while let Some(entry) = queue.peek() {
            if entry.scheduled_time > now {
                return Ok(None);
            }
            let entry = queue.pop().expect("queue pop");
            if let Some(mut job_entry) = self.jobs.get_mut(&entry.job_id) {
                if job_entry.scheduled_time != entry.scheduled_time {
                    continue;
                }
                if job_entry.status != BatchJobStatus::Pending {
                    continue;
                }
                job_entry.status = BatchJobStatus::Scheduled;
                self.sync_job_to_chain(&job_entry);
                {
                    let mut metrics = self.metrics.write().await;
                    if let Some(count) = metrics.queue_depth_by_priority.get_mut(&job_entry.priority) {
                        *count = count.saturating_sub(1);
                    }
                }
                return Ok(Some(job_entry.clone()));
            }
        }
        Ok(None)
    }

    pub async fn get_job_status(&self, job_id: &str) -> Option<BatchJobInfo> {
        self.jobs.get(job_id).map(|entry| job_to_info(entry.value()))
    }

    pub async fn get_job(&self, job_id: &str) -> Option<BatchJob> {
        self.jobs.get(job_id).map(|entry| entry.value().clone())
    }

    pub async fn list_user_jobs(
        &self,
        user_address: &str,
        status: Option<BatchJobStatus>,
    ) -> Vec<BatchJobInfo> {
        self.jobs
            .iter()
            .filter(|entry| entry.user_address == user_address)
            .filter(|entry| status.map(|s| entry.status == s).unwrap_or(true))
            .map(|entry| job_to_info(entry.value()))
            .collect()
    }

    pub async fn get_queue_stats(&self) -> QueueStats {
        let now = now_timestamp();
        let metrics = self.metrics.read().await;
        let average_wait_time_ms = self.average_wait_time_ms(now);
        let mut estimates = HashMap::new();
        for priority in [BatchPriority::Standard, BatchPriority::Batch, BatchPriority::Economy] {
            estimates.insert(priority, self.estimate_completion_time(priority, now).await);
        }
        QueueStats {
            jobs_by_priority: metrics.queue_depth_by_priority.clone(),
            average_wait_time_ms,
            estimates,
        }
    }

    pub async fn estimate_completion_time(
        &self,
        priority: BatchPriority,
        now: i64,
    ) -> CompletionEstimate {
        let metrics = self.metrics.read().await;
        let avg_ms = if metrics.average_processing_time_ms == 0 {
            DEFAULT_AVG_PROCESSING_TIME_MS
        } else {
            metrics.average_processing_time_ms
        };
        let standard = *metrics.queue_depth_by_priority.get(&BatchPriority::Standard).unwrap_or(&0);
        let batch = *metrics.queue_depth_by_priority.get(&BatchPriority::Batch).unwrap_or(&0);
        let economy = *metrics.queue_depth_by_priority.get(&BatchPriority::Economy).unwrap_or(&0);
        let depth = match priority {
            BatchPriority::Standard => standard,
            BatchPriority::Batch => standard.saturating_add(batch),
            BatchPriority::Economy => standard.saturating_add(batch).saturating_add(economy),
        };
        let scheduled_time = now.saturating_add(priority.delay_seconds());
        let estimated_wait_ms = depth.saturating_add(1).saturating_mul(avg_ms);
        let estimated_completion_time =
            scheduled_time.saturating_add((estimated_wait_ms / 1000) as i64);
        CompletionEstimate {
            scheduled_time,
            estimated_completion_time,
            estimated_wait_ms,
        }
    }

    pub async fn update_job_status(
        &self,
        job_id: &str,
        status: BatchJobStatus,
        result: Option<Vec<u8>>,
        error: Option<String>,
    ) -> Result<(), BatchError> {
        let now = now_timestamp();
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or(BatchError::JobNotFound)?;
        job.status = status;
        if status == BatchJobStatus::Processing {
            job.processing_start_time = Some(now);
        }
        if status == BatchJobStatus::Completed {
            job.completion_time = Some(now);
            job.result = result;
            job.error_message = None;
            let _ = self.tier_manager.record_request(&job.user_address, None, now);
            self.update_metrics_for_completion(&job).await;
        }
        if status == BatchJobStatus::Failed {
            job.completion_time = Some(now);
            job.result = None;
            job.error_message = error;
            self.update_metrics_for_failure().await;
        }
        self.sync_job_to_chain(&job);
        eprintln!("batch job status updated: job_id={}, status={}", job_id, status);
        Ok(())
    }

    pub async fn cancel_job(&self, job_id: &str, user_address: &str) -> Result<u64, BatchError> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or(BatchError::JobNotFound)?;
        if job.user_address != user_address {
            return Err(BatchError::JobNotFound);
        }
        if !matches!(job.status, BatchJobStatus::Pending | BatchJobStatus::Scheduled) {
            return Err(BatchError::JobNotFound);
        }
        let fee = job
            .priority
            .base_price()
            .saturating_mul(self.cancellation_fee_bps)
            / 10_000;
        let refund = job.priority.base_price().saturating_sub(fee);
        job.status = BatchJobStatus::Failed;
        job.error_message = Some("canceled".to_string());
        job.completion_time = Some(now_timestamp());
        self.sync_job_to_chain(&job);
        self.update_metrics_for_failure().await;
        {
            let mut metrics = self.metrics.write().await;
            if let Some(count) = metrics.queue_depth_by_priority.get_mut(&job.priority) {
                *count = count.saturating_sub(1);
            }
        }
        Ok(refund)
    }

    pub async fn reschedule_job(&self, job_id: &str, scheduled_time: i64) -> Result<(), BatchError> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or(BatchError::JobNotFound)?;
        job.scheduled_time = scheduled_time;
        job.status = BatchJobStatus::Pending;
        {
            let mut queue = self.priority_queue.lock().await;
            queue.push(ScheduledJob {
                job_id: job_id.to_string(),
                scheduled_time,
                priority: job.priority,
            });
        }
        {
            let mut metrics = self.metrics.write().await;
            if let Some(count) = metrics.queue_depth_by_priority.get_mut(&job.priority) {
                *count = count.saturating_add(1);
            }
        }
        self.sync_job_to_chain(&job);
        Ok(())
    }

    fn sync_job_to_chain(&self, job: &BatchJob) {
        let result_hash = job
            .result
            .as_ref()
            .map(|data| hash_data(data.as_slice()));
        let state = blockchain_core::state::BatchJobState {
            job_id: job.request_id.clone(),
            user_address: job.user_address.clone(),
            priority: job.priority.as_u8(),
            status: job.status.as_u8(),
            submission_time: job.submission_time,
            scheduled_time: job.scheduled_time,
            completion_time: job.completion_time,
            model_id: job.model_id.clone(),
            result_hash,
        };
        self.chain_state.set_batch_job(job.request_id.clone(), state);
    }

    fn average_wait_time_ms(&self, now: i64) -> u64 {
        let mut total = 0u64;
        let mut count = 0u64;
        for entry in self.jobs.iter() {
            if matches!(entry.status, BatchJobStatus::Pending | BatchJobStatus::Scheduled) {
                let wait = entry
                    .scheduled_time
                    .saturating_sub(now)
                    .max(0) as u64;
                total = total.saturating_add(wait.saturating_mul(1000));
                count = count.saturating_add(1);
            }
        }
        if count == 0 {
            0
        } else {
            total / count
        }
    }

    async fn update_metrics_for_completion(&self, job: &BatchJob) {
        let mut metrics = self.metrics.write().await;
        metrics.completed_jobs = metrics.completed_jobs.saturating_add(1);
        if let (Some(start), Some(end)) = (job.processing_start_time, job.completion_time) {
            let elapsed_ms = end.saturating_sub(start).max(0) as u64 * 1000;
            let completed = metrics.completed_jobs;
            let prev_avg = metrics.average_processing_time_ms;
            metrics.average_processing_time_ms =
                (prev_avg.saturating_mul(completed.saturating_sub(1)).saturating_add(elapsed_ms))
                    / completed.max(1);
        }
    }

    async fn update_metrics_for_failure(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.failed_jobs = metrics.failed_jobs.saturating_add(1);
    }
}

pub struct BatchWorker {
    queue: Arc<BatchQueue>,
    inference_engine: Arc<InferenceEngine>,
    shutdown_signal: watch::Receiver<bool>,
    processing_interval_ms: u64,
}

impl BatchWorker {
    pub fn new(
        queue: Arc<BatchQueue>,
        inference_engine: Arc<InferenceEngine>,
        shutdown_signal: watch::Receiver<bool>,
        processing_interval_ms: Option<u64>,
    ) -> Self {
        Self {
            queue,
            inference_engine,
            shutdown_signal,
            processing_interval_ms: processing_interval_ms.unwrap_or(DEFAULT_PROCESSING_INTERVAL_MS),
        }
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    pub async fn run(mut self) {
        let mut join_set = tokio::task::JoinSet::new();
        loop {
            if *self.shutdown_signal.borrow() {
                break;
            }
            if check_shutdown().is_err() {
                break;
            }

            while let Ok(Some(job)) = self.queue.get_next_ready_job().await {
                let queue = self.queue.clone();
                let engine = self.inference_engine.clone();
                join_set.spawn(async move {
                    let job_id = job.request_id.clone();
                    if let Err(err) = queue
                        .update_job_status(&job_id, BatchJobStatus::Processing, None, None)
                        .await
                    {
                        println!("job_id = {}, error = {}, failed to mark job processing", job_id, err);
                        return;
                    }
                    let handle = tokio::runtime::Handle::current();
                    let res = tokio::task::spawn_blocking(move || handle.block_on(process_job(engine, job))).await;
                    match res {
                        Ok(Ok(result)) => {
                            let _ = queue
                                .update_job_status(&job_id, BatchJobStatus::Completed, Some(result), None)
                                .await;
                        }
                        Ok(Err(err)) => {
                            let _ = queue
                                .update_job_status(
                                    &job_id,
                                    BatchJobStatus::Failed,
                                    None,
                                    Some(err.to_string()),
                                )
                                .await;
                        }
                        Err(_) => {
                            let _ = queue
                                .update_job_status(
                                    &job_id,
                                    BatchJobStatus::Failed,
                                    None,
                                    Some("panic".to_string()),
                                )
                                .await;
                        }
                    }
                });
            }

            tokio::select! {
                _ = self.shutdown_signal.changed() => {},
                Some(res) = join_set.join_next() => {
                    if res.is_err() {
                        eprintln!("batch worker task failed");
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(self.processing_interval_ms)) => {}
            }
        }

        while let Some(res) = join_set.join_next().await {
            if res.is_err() {
                eprintln!("batch worker task failed on shutdown");
            }
        }
    }
}

fn job_to_info(job: &BatchJob) -> BatchJobInfo {
    BatchJobInfo {
        request_id: job.request_id.clone(),
        user_address: job.user_address.clone(),
        priority: job.priority,
        status: job.status,
        submission_time: job.submission_time,
        scheduled_time: job.scheduled_time,
        processing_start_time: job.processing_start_time,
        completion_time: job.completion_time,
        model_id: job.model_id.clone(),
        result: job.result.clone(),
        error_message: job.error_message.clone(),
    }
}

pub fn validate_batch_payment(
    transaction: &Transaction,
    expected_receiver: &str,
    expected_amount: Option<u64>,
    model_id: String,
    input_data: Vec<u8>,
) -> Result<BatchPaymentPayload, PaymentValidationError> {
    let payload = transaction
        .payload
        .as_ref()
        .ok_or(PaymentValidationError::InvalidPayload)?;
    let mut batch_payload: BatchPaymentPayload =
        serde_json::from_slice(payload).map_err(|_| PaymentValidationError::InvalidPayload)?;

    if batch_payload.request_id.is_empty() {
        batch_payload.request_id = Uuid::new_v4().to_string();
    }
    batch_payload.model_id = model_id;
    batch_payload.input_data = input_data;

    if transaction.receiver != expected_receiver {
        return Err(PaymentValidationError::ReceiverMismatch {
            expected: expected_receiver.to_string(),
            actual: transaction.receiver.clone(),
        });
    }

    if transaction.sender != batch_payload.user_address {
        return Err(PaymentValidationError::SenderMismatch {
            expected: batch_payload.user_address.clone(),
            actual: transaction.sender.clone(),
        });
    }

    if let Some(expected_amount) = expected_amount {
        let actual_amount = transaction.amount.value();
        if actual_amount != expected_amount {
            return Err(PaymentValidationError::AmountMismatch {
                expected: expected_amount,
                actual: actual_amount,
            });
        }
    }

    Ok(batch_payload)
}

async fn process_job(
    engine: Arc<InferenceEngine>,
    job: BatchJob,
) -> Result<Vec<u8>, BatchError> {
    let inputs: Vec<InferenceTensor> =
        serde_json::from_slice(&job.input_data).map_err(|err| BatchError::InferenceFailed(err.to_string()))?;

    let mut attempt = 0;
    loop {
        attempt += 1;
        let res = engine.run_inference(&job.model_id, inputs.clone()).await;
        match res {
            Ok(outputs) => {
                let bytes = serialize_outputs(outputs)?;
                return Ok(bytes);
            }
            Err(err) => {
                if attempt >= RETRY_LIMIT || !is_transient_error(&err) {
                    return Err(BatchError::from(err));
                }
                tokio::time::sleep(std::time::Duration::from_millis(RETRY_BACKOFF_MS)).await;
            }
        }
    }
}

fn serialize_outputs(outputs: Vec<InferenceOutput>) -> Result<Vec<u8>, BatchError> {
    serde_json::to_vec(&outputs).map_err(|err| BatchError::InferenceFailed(err.to_string()))
}

#[allow(clippy::match_like_matches_macro)]
fn is_transient_error(err: &InferenceError) -> bool {
    match err {
        InferenceError::InferenceFailed(_)
        | InferenceError::ModelLoadFailed(_)
        | InferenceError::OutOfMemory
        | InferenceError::StorageError(_)
        | InferenceError::ShardError(_) => true,
        _ => false,
    }
}
