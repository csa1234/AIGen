// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use genesis::{check_shutdown, CEO_WALLET};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::inference::InferenceOutput;
use crate::tiers::{
    now_timestamp, Subscription, SubscriptionTier, TierConfig, TierError, TierManager,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdConfig {
    pub enabled: bool,
    pub injection_rate: f64,
    pub max_ad_length: usize,
    pub max_total_output_length: Option<usize>,
    pub upgrade_prompt_threshold: u64,
    pub free_tier_only: bool,
    pub templates: Option<Vec<AdTemplate>>,
    pub template_file_path: Option<String>,
}

impl Default for AdConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            injection_rate: 0.5,
            max_ad_length: 500,
            max_total_output_length: None,
            upgrade_prompt_threshold: 8,
            free_tier_only: true,
            templates: None,
            template_file_path: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdTemplate {
    pub id: String,
    pub content: String,
    pub priority: u8,
    pub active: bool,
}

impl AdTemplate {
    pub fn render(
        &self,
        model_name: &str,
        tier: &SubscriptionTier,
        remaining_requests: u64,
        upgrade_url: &str,
        requests: u64,
        active_users: usize,
    ) -> String {
        let mut text = self.content.clone();
        text = text.replace("{model_name}", model_name);
        text = text.replace("{tier}", &format!("{tier:?}"));
        text = text.replace("{remaining_requests}", &remaining_requests.to_string());
        text = text.replace("{upgrade_url}", upgrade_url);
        text = text.replace("{requests}", &requests.to_string());
        text = text.replace("{active_users}", &active_users.to_string());
        text
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdImpression {
    pub timestamp: i64,
    pub template_id: String,
    pub model_id: String,
    pub tier: SubscriptionTier,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdImpressionLog {
    pub user_address: String,
    pub impressions: Vec<AdImpression>,
    pub total_impressions: u64,
    pub last_impression_time: i64,
}

#[derive(Debug, Error)]
pub enum AdError {
    #[error("template not found")]
    TemplateNotFound,
    #[error("invalid template")]
    InvalidTemplate,
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("tier check failed")]
    TierCheckFailed,
    #[error("shutdown active")]
    Shutdown,
}

impl From<TierError> for AdError {
    fn from(err: TierError) -> Self {
        match err {
            TierError::Shutdown => AdError::Shutdown,
            _ => AdError::TierCheckFailed,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpgradePromptType {
    None,
    Gentle,
    Urgent,
}

struct GeneratedAd {
    template_id: String,
    content: String,
    tier: SubscriptionTier,
}

#[derive(Clone)]
pub struct AdManager {
    tier_manager: Arc<TierManager>,
    ad_templates: Arc<RwLock<Vec<AdTemplate>>>,
    impression_tracker: Arc<DashMap<String, AdImpressionLog>>,
    config: AdConfig,
    current_template_index: Arc<AtomicUsize>,
}

impl AdManager {
    pub fn new(tier_manager: Arc<TierManager>, config: AdConfig) -> Self {
        let templates = Self::load_templates(&config);
        Self {
            tier_manager,
            ad_templates: Arc::new(RwLock::new(templates)),
            impression_tracker: Arc::new(DashMap::new()),
            config,
            current_template_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn load_templates(config: &AdConfig) -> Vec<AdTemplate> {
        // Try to load templates from config first
        if let Some(ref templates) = config.templates {
            return templates.clone();
        }
        
        // Try to load from template file if specified
        if let Some(ref file_path) = config.template_file_path {
            if let Ok(content) = fs::read_to_string(file_path) {
                if let Ok(templates) = serde_json::from_str::<Vec<AdTemplate>>(&content) {
                    return templates;
                }
                if let Ok(templates) = toml::from_str::<Vec<AdTemplate>>(&content) {
                    return templates;
                }
            }
        }
        
        // Fall back to default templates
        default_ad_templates()
    }

    pub fn with_default_config(tier_manager: Arc<TierManager>) -> Self {
        Self::new(tier_manager, AdConfig::default())
    }

    pub fn should_inject_ad(&self, user_address: &str) -> Result<bool, AdError> {
        check_shutdown().map_err(|_| AdError::Shutdown)?;
        if !self.config.enabled {
            return Ok(false);
        }
        if user_address == CEO_WALLET {
            return Ok(false);
        }
        let now = now_timestamp();
        let subscription = self
            .tier_manager
            .ensure_subscription_active(user_address, now)?;
        if self.config.free_tier_only && subscription.tier != SubscriptionTier::Free {
            return Ok(false);
        }
        let mut rng = rand::thread_rng();
        let roll: f64 = rng.gen();
        if roll > self.config.injection_rate {
            return Ok(false);
        }
        if let Some(log) = self.impression_tracker.get(user_address) {
            let min_interval_secs = 60;
            if now.saturating_sub(log.last_impression_time) < min_interval_secs {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn generate_ad_content(
        &self,
        user_address: &str,
        model_id: &str,
    ) -> Result<String, AdError> {
        let generated = self.generate_ad_payload(user_address, model_id)?;
        Ok(generated.content)
    }

    fn generate_ad_payload(
        &self,
        user_address: &str,
        model_id: &str,
    ) -> Result<GeneratedAd, AdError> {
        check_shutdown().map_err(|_| AdError::Shutdown)?;
        let now = now_timestamp();
        let subscription = self
            .tier_manager
            .ensure_subscription_active(user_address, now)?;
        let config = self.tier_manager.get_config(subscription.tier)?;
        let remaining = config
            .request_limit
            .saturating_sub(subscription.requests_used);
        let upgrade_url = format!("https://aigen.ai/upgrade?address={user_address}");
        let template = self.select_template()?;
        let active_users = self.impression_tracker.len();
        let mut content = template.render(
            model_id,
            &subscription.tier,
            remaining,
            &upgrade_url,
            config.request_limit,
            active_users,
        );
        let prompt_type = self.analyze_usage_pattern(&subscription, config)?;
        if prompt_type != UpgradePromptType::None {
            let prompt =
                self.generate_upgrade_prompt(prompt_type, &subscription, config, user_address);
            content = format!("{prompt}\n\n{content}");
        }
        content = truncate_text(&content, self.config.max_ad_length);
        Ok(GeneratedAd {
            template_id: template.id,
            content,
            tier: subscription.tier,
        })
    }

    pub fn inject_ad_into_outputs(
        &self,
        outputs: Vec<InferenceOutput>,
        user_address: &str,
        model_id: &str,
    ) -> Result<(Vec<InferenceOutput>, String, SubscriptionTier), AdError> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if outputs.is_empty() {
                return Ok((outputs, String::new(), SubscriptionTier::Free));
            }
            
            // Find a text-capable output to append ad to
            let text_output_index = self.find_text_capable_output(&outputs);
            
            let generated = self.generate_ad_payload(user_address, model_id)?;
            let mut outputs = outputs;
            
            if let Some(_index) = text_output_index {
                // Check total output length limit
                if let Some(max_total_length) = self.config.max_total_output_length {
                    let current_total_length: usize = outputs.iter()
                        .map(|output| output.data.len())
                        .sum();
                    let ad_length = generated.content.len();
                    
                    if current_total_length + ad_length > max_total_length {
                        // Skip ad injection if it would exceed total length limit
                        return Ok((outputs, String::new(), SubscriptionTier::Free));
                    }
                }
                
                // Append ad as separate output entry to preserve original output
                let ad_output = InferenceOutput {
                    name: "ad_text".to_string(),
                    shape: vec![generated.content.len() as i64],
                    data: string_to_data(&generated.content),
                };
                outputs.push(ad_output);
            }
            
            Ok((outputs, generated.template_id, generated.tier))
        }));
        match result {
            Ok(outcome) => outcome,
            Err(_) => Err(AdError::SerializationError("panic".to_string())),
        }
    }

    pub fn inject_ad_into_batch_result(
        &self,
        bytes: Vec<u8>,
        user_address: &str,
        model_id: &str,
    ) -> Result<(Vec<u8>, String, SubscriptionTier), AdError> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let outputs: Vec<InferenceOutput> = serde_json::from_slice(&bytes)
                .map_err(|err| AdError::SerializationError(err.to_string()))?;
            let (outputs, template_id, tier) =
                self.inject_ad_into_outputs(outputs, user_address, model_id)?;
            let bytes = serde_json::to_vec(&outputs)
                .map_err(|err| AdError::SerializationError(err.to_string()))?;
            Ok((bytes, template_id, tier))
        }));
        match result {
            Ok(outcome) => outcome,
            Err(_) => Err(AdError::SerializationError("panic".to_string())),
        }
    }

    fn find_text_capable_output(&self, outputs: &[InferenceOutput]) -> Option<usize> {
        // Look for outputs that are likely text-capable based on name and shape
        for (index, output) in outputs.iter().enumerate() {
            // Check if output name suggests text capability
            let name_lower = output.name.to_lowercase();
            if name_lower.contains("text") || name_lower.contains("output") || name_lower.contains("response") {
                // Check if shape is suitable for text (1D or small dimensions)
                if output.shape.len() <= 2 && output.shape.iter().all(|&dim| dim <= 10000) {
                    return Some(index);
                }
            }
        }
        None
    }

    pub fn record_impression(
        &self,
        user_address: &str,
        template_id: &str,
        model_id: &str,
        tier: SubscriptionTier,
    ) -> Result<(), AdError> {
        check_shutdown().map_err(|_| AdError::Shutdown)?;
        let now = now_timestamp();
        let mut log = self
            .impression_tracker
            .entry(user_address.to_string())
            .or_insert(AdImpressionLog {
                user_address: user_address.to_string(),
                impressions: Vec::new(),
                total_impressions: 0,
                last_impression_time: 0,
            });
        log.impressions.push(AdImpression {
            timestamp: now,
            template_id: template_id.to_string(),
            model_id: model_id.to_string(),
            tier,
        });
        log.total_impressions = log.total_impressions.saturating_add(1);
        log.last_impression_time = now;
        let cutoff = now.saturating_sub(90 * 24 * 60 * 60);
        log.impressions.retain(|entry| entry.timestamp >= cutoff);

        // Use println! for now instead of trace! to avoid callsite issues
        println!(
            "ad impression recorded: user={}, template={}, model={}",
            user_address, template_id, model_id
        );

        Ok(())
    }

    pub fn analyze_usage_pattern(
        &self,
        subscription: &Subscription,
        config: &TierConfig,
    ) -> Result<UpgradePromptType, AdError> {
        let total = self
            .impression_tracker
            .get(&subscription.user_address)
            .map(|log| log.total_impressions)
            .unwrap_or(0);
        let remaining = config
            .request_limit
            .saturating_sub(subscription.requests_used);
        let used = config.request_limit.saturating_sub(remaining);
        let threshold = self.config.upgrade_prompt_threshold;
        if used >= threshold {
            return Ok(UpgradePromptType::Urgent);
        }
        let now = now_timestamp();
        let thirty_days = 2_592_000;
        if total >= threshold || now.saturating_sub(subscription.start_timestamp) > thirty_days {
            return Ok(UpgradePromptType::Gentle);
        }
        Ok(UpgradePromptType::None)
    }

    pub fn generate_upgrade_prompt(
        &self,
        prompt_type: UpgradePromptType,
        subscription: &Subscription,
        config: &TierConfig,
        user_address: &str,
    ) -> String {
        let basic = self.tier_manager.get_config(SubscriptionTier::Basic).ok();
        let pro = self.tier_manager.get_config(SubscriptionTier::Pro).ok();
        let unlimited = self
            .tier_manager
            .get_config(SubscriptionTier::Unlimited)
            .ok();
        let basic_requests = basic.map(|c| c.request_limit).unwrap_or(100);
        let pro_requests = pro.map(|c| c.request_limit).unwrap_or(1000);
        let unlimited_requests = unlimited.map(|c| c.request_limit).unwrap_or(u64::MAX);
        let upgrade_url = format!("https://aigen.ai/upgrade?address={user_address}");
        let header = match prompt_type {
            UpgradePromptType::Gentle => {
                "Enjoying AIGEN? Upgrade to Basic for 100 requests/month!".to_string()
            }
            UpgradePromptType::Urgent => format!(
                "You've used {}/{} free requests. Upgrade now to avoid interruption!",
                subscription.requests_used, config.request_limit
            ),
            UpgradePromptType::None => String::new(),
        };
        let unlimited_label = if unlimited_requests == u64::MAX {
            "Unlimited".to_string()
        } else {
            unlimited_requests.to_string()
        };
        format!(
            "{header}\nTier comparison:\nBasic: {basic_requests} requests/month\nPro: {pro_requests} requests/month\nUnlimited: {unlimited_label} requests/month\nUpgrade: {upgrade_url}"
        )
    }

    pub fn get_user_impressions(&self, user_address: &str) -> u64 {
        self.impression_tracker
            .get(user_address)
            .map(|log| log.total_impressions)
            .unwrap_or(0)
    }

    pub fn get_total_impressions(&self) -> u64 {
        self.impression_tracker
            .iter()
            .map(|entry| entry.total_impressions)
            .sum()
    }

    pub fn get_impressions_by_tier(&self) -> HashMap<SubscriptionTier, u64> {
        let mut counts = HashMap::new();
        for entry in self.impression_tracker.iter() {
            for impression in entry.impressions.iter() {
                *counts.entry(impression.tier).or_insert(0) += 1;
            }
        }
        counts
    }

    pub fn get_template_performance(&self) -> HashMap<String, u64> {
        let mut counts = HashMap::new();
        for entry in self.impression_tracker.iter() {
            for impression in entry.impressions.iter() {
                *counts.entry(impression.template_id.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    pub fn get_recent_impressions(&self, limit: usize) -> Vec<AdImpression> {
        let mut all = Vec::new();
        for entry in self.impression_tracker.iter() {
            all.extend(entry.impressions.clone());
        }
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.into_iter().take(limit).collect()
    }

    pub fn cleanup_old_impressions(&self) -> Result<(), AdError> {
        check_shutdown().map_err(|_| AdError::Shutdown)?;
        let now = now_timestamp();
        let cutoff = now.saturating_sub(90 * 24 * 60 * 60);
        let mut to_remove = Vec::new();
        for mut entry in self.impression_tracker.iter_mut() {
            entry
                .impressions
                .retain(|impression| impression.timestamp >= cutoff);
            if entry.impressions.is_empty() {
                to_remove.push(entry.user_address.clone());
            }
        }
        for user in to_remove {
            self.impression_tracker.remove(&user);
        }
        Ok(())
    }

    pub fn add_template(&self, template: AdTemplate) {
        let mut templates = self.ad_templates.write();
        templates.push(template);
        println!("ad template added");
    }

    pub fn remove_template(&self, id: &str) -> Result<(), AdError> {
        let mut templates = self.ad_templates.write();
        let before = templates.len();
        templates.retain(|template| template.id != id);
        if templates.len() == before {
            return Err(AdError::TemplateNotFound);
        }
        println!("ad template removed: {id}");
        Ok(())
    }

    pub fn update_template(&self, updated: AdTemplate) -> Result<(), AdError> {
        let mut templates = self.ad_templates.write();
        if let Some(existing) = templates.iter_mut().find(|t| t.id == updated.id) {
            *existing = updated;
            println!("ad template updated");
            return Ok(());
        }
        Err(AdError::TemplateNotFound)
    }

    pub fn list_templates(&self) -> Vec<AdTemplate> {
        self.ad_templates.read().clone()
    }

    pub fn set_template_active(&self, id: &str, active: bool) -> Result<(), AdError> {
        let mut templates = self.ad_templates.write();
        if let Some(template) = templates.iter_mut().find(|t| t.id == id) {
            template.active = active;
            println!("ad template active state changed: {id} -> {active}");
            return Ok(());
        }
        Err(AdError::TemplateNotFound)
    }

    pub fn insert_impression_log(&self, user_address: &str, log: AdImpressionLog) {
        self.impression_tracker
            .insert(user_address.to_string(), log);
    }

    fn select_template(&self) -> Result<AdTemplate, AdError> {
        let templates = self.ad_templates.read();
        let active: Vec<AdTemplate> = templates.iter().filter(|t| t.active).cloned().collect();
        if active.is_empty() {
            return Err(AdError::TemplateNotFound);
        }
        let mut expanded = Vec::new();
        for template in active.iter() {
            let count = template.priority.max(1) as usize;
            for _ in 0..count {
                expanded.push(template.clone());
            }
        }
        let index = self.current_template_index.fetch_add(1, Ordering::Relaxed);
        let selected = expanded
            .get(index % expanded.len())
            .cloned()
            .or_else(|| active.first().cloned())
            .ok_or(AdError::TemplateNotFound)?;
        Ok(selected)
    }
}

pub fn default_ad_templates() -> Vec<AdTemplate> {
    vec![
        AdTemplate {
            id: "core-upgrade".to_string(),
            content:
                "Powered by AIGEN Blockchain. Upgrade to {tier} for {requests} requests/month!"
                    .to_string(),
            priority: 3,
            active: true,
        },
        AdTemplate {
            id: "remaining-requests".to_string(),
            content:
                "You have {remaining_requests} free requests remaining. Unlock more with AIGEN Pro!"
                    .to_string(),
            priority: 4,
            active: true,
        },
        AdTemplate {
            id: "support-decentralized".to_string(),
            content: "Support decentralized AI! Upgrade to remove ads and get priority processing."
                .to_string(),
            priority: 2,
            active: true,
        },
        AdTemplate {
            id: "model-speed".to_string(),
            content: "Running {model_name} on AIGEN. Want faster inference? Try our Batch tier!"
                .to_string(),
            priority: 2,
            active: true,
        },
        AdTemplate {
            id: "active-users".to_string(),
            content: "Join {active_users} users on AIGEN. Upgrade for advanced features!"
                .to_string(),
            priority: 1,
            active: true,
        },
    ]
}

fn string_to_data(text: &str) -> Vec<f32> {
    text.as_bytes().iter().map(|b| *b as f32).collect()
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    text.chars().take(max_len).collect()
}
