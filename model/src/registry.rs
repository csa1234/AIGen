use dashmap::DashMap;
use genesis::{check_shutdown, GenesisError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::tiers::SubscriptionTier;

const MAX_SHARD_SIZE: u64 = 4_294_967_296;
const TARGET_REDUNDANCY: usize = 3;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("model not found")]
    ModelNotFound,
    #[error("shard not found")]
    ShardNotFound,
    #[error("invalid model id")]
    InvalidModelId,
    #[error("invalid shard")]
    InvalidShard,
    #[error("registry locked")]
    RegistryLocked,
    #[error("shutdown active")]
    Shutdown,
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<GenesisError> for ModelError {
    fn from(err: GenesisError) -> Self {
        match err {
            GenesisError::ShutdownActive => ModelError::Shutdown,
            other => ModelError::Serialization(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub total_size: u64,
    pub shard_count: u32,
    pub verification_hashes: Vec<[u8; 32]>,
    pub is_core_model: bool,
    #[serde(default)]
    pub minimum_tier: Option<SubscriptionTier>,
    #[serde(default)]
    pub is_experimental: bool,
    pub created_at: i64,
}

impl ModelMetadata {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !is_valid_model_id(&self.model_id) {
            return Err(ModelError::InvalidModelId);
        }
        if self.shard_count == 0 {
            return Err(ModelError::InvalidShard);
        }
        if self.total_size == 0 {
            return Err(ModelError::InvalidShard);
        }
        if self.verification_hashes.len() != self.shard_count as usize {
            return Err(ModelError::InvalidShard);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelShard {
    pub model_id: String,
    pub shard_index: u32,
    pub total_shards: u32,
    pub hash: [u8; 32],
    pub size: u64,
    pub ipfs_cid: Option<String>,
    pub http_urls: Vec<String>,
    pub locations: Vec<ShardLocation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardLocation {
    pub node_id: String,
    pub backend_type: String,
    pub location_uri: String,
    pub last_verified: i64,
    pub is_healthy: bool,
}

impl ModelShard {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.model_id.trim().is_empty() {
            return Err(ModelError::InvalidModelId);
        }
        if self.total_shards == 0 || self.shard_index >= self.total_shards {
            return Err(ModelError::InvalidShard);
        }
        if self.size == 0 || self.size > MAX_SHARD_SIZE {
            return Err(ModelError::InvalidShard);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ModelRegistry {
    models: Arc<DashMap<String, ModelMetadata>>,
    shards: Arc<DashMap<(String, u32), ModelShard>>,
    core_model_id: Arc<RwLock<Option<String>>>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: Arc::new(DashMap::new()),
            shards: Arc::new(DashMap::new()),
            core_model_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn register_model(&self, mut metadata: ModelMetadata) -> Result<(), ModelError> {
        check_shutdown()?;
        metadata.validate()?;

        if self.models.contains_key(&metadata.model_id) {
            self.shards
                .retain(|key, _| key.0 != metadata.model_id.as_str());
        }

        let current_core = { self.core_model_id.read().clone() };
        if metadata.is_core_model {
            *self.core_model_id.write() = Some(metadata.model_id.clone());
            for mut entry in self.models.iter_mut() {
                entry.value_mut().is_core_model = false;
            }
        } else if let Some(core_id) = current_core {
            if core_id == metadata.model_id {
                metadata.is_core_model = true;
            }
        }

        self.models.insert(metadata.model_id.clone(), metadata);
        Ok(())
    }

    pub fn get_model(&self, model_id: &str) -> Result<ModelMetadata, ModelError> {
        check_shutdown()?;
        self.models
            .get(model_id)
            .map(|entry| entry.value().clone())
            .ok_or(ModelError::ModelNotFound)
    }

    pub fn list_models(&self) -> Result<Vec<ModelMetadata>, ModelError> {
        check_shutdown()?;
        let mut models: Vec<ModelMetadata> = self
            .models
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        models.sort_by_key(|model| model.created_at);
        Ok(models)
    }

    pub fn list_models_for_tier(
        &self,
        tier: SubscriptionTier,
    ) -> Result<Vec<ModelMetadata>, ModelError> {
        check_shutdown()?;
        let mut models: Vec<ModelMetadata> = self
            .models
            .iter()
            .map(|entry| entry.value().clone())
            .filter(|model| model_accessible_for_tier(model, tier))
            .collect();
        models.sort_by_key(|model| model.created_at);
        Ok(models)
    }

    pub fn model_accessible_for_tier(
        &self,
        model_id: &str,
        tier: SubscriptionTier,
    ) -> Result<bool, ModelError> {
        check_shutdown()?;
        let metadata = self.get_model(model_id)?;
        Ok(model_accessible_for_tier(&metadata, tier))
    }

    pub fn register_shard(&self, shard: ModelShard) -> Result<(), ModelError> {
        check_shutdown()?;
        shard.validate()?;

        let parent = self
            .models
            .get(&shard.model_id)
            .ok_or(ModelError::ModelNotFound)?;

        if shard.total_shards != parent.shard_count {
            return Err(ModelError::InvalidShard);
        }

        let expected = parent
            .verification_hashes
            .get(shard.shard_index as usize)
            .ok_or(ModelError::InvalidShard)?;
        if expected != &shard.hash {
            return Err(ModelError::InvalidShard);
        }

        self.shards
            .insert((shard.model_id.clone(), shard.shard_index), shard);
        Ok(())
    }

    pub fn get_shard(&self, model_id: &str, shard_index: u32) -> Result<ModelShard, ModelError> {
        check_shutdown()?;
        let key = (model_id.to_string(), shard_index);
        self.shards
            .get(&key)
            .map(|entry| entry.value().clone())
            .ok_or(ModelError::ShardNotFound)
    }

    pub fn list_shards(&self, model_id: &str) -> Result<Vec<ModelShard>, ModelError> {
        check_shutdown()?;
        let mut shards: Vec<ModelShard> = self
            .shards
            .iter()
            .filter(|entry| entry.key().0 == model_id)
            .map(|entry| entry.value().clone())
            .collect();
        shards.sort_by_key(|shard| shard.shard_index);
        Ok(shards)
    }

    pub fn get_core_model(&self) -> Result<Option<ModelMetadata>, ModelError> {
        check_shutdown()?;
        let core = { self.core_model_id.read().clone() };
        Ok(core.and_then(|model_id| {
            self.models
                .get(&model_id)
                .map(|entry| entry.value().clone())
        }))
    }

    pub fn set_core_model(&self, model_id: &str) -> Result<(), ModelError> {
        check_shutdown()?;
        if !self.models.contains_key(model_id) {
            return Err(ModelError::ModelNotFound);
        }
        *self.core_model_id.write() = Some(model_id.to_string());
        for mut entry in self.models.iter_mut() {
            entry.value_mut().is_core_model = entry.key() == model_id;
        }
        Ok(())
    }

    pub fn remove_model(&self, model_id: &str) -> Result<(), ModelError> {
        check_shutdown()?;
        let is_core = self
            .core_model_id
            .read()
            .as_ref()
            .map(|core_id| core_id == model_id)
            .unwrap_or(false);
        if is_core {
            return Err(ModelError::RegistryLocked);
        }

        let removed = self.models.remove(model_id);
        if removed.is_none() {
            return Err(ModelError::ModelNotFound);
        }

        self.shards.retain(|(id, _), _| id != model_id);
        Ok(())
    }

    pub fn model_exists(&self, model_id: &str) -> Result<bool, ModelError> {
        check_shutdown()?;
        Ok(self.models.contains_key(model_id))
    }

    pub fn get_shard_count(&self, model_id: &str) -> Result<u32, ModelError> {
        check_shutdown()?;
        if !self.models.contains_key(model_id) {
            return Ok(0);
        }
        Ok(self
            .shards
            .iter()
            .filter(|entry| entry.key().0 == model_id)
            .count() as u32)
    }

    pub fn register_shard_location(
        &self,
        model_id: &str,
        shard_index: u32,
        location: ShardLocation,
    ) -> Result<(), ModelError> {
        check_shutdown()?;
        let key = (model_id.to_string(), shard_index);
        let mut entry = self.shards.get_mut(&key).ok_or(ModelError::ShardNotFound)?;
        let locations = &mut entry.locations;
        let backend_type = location.backend_type.clone();
        if let Some(existing) = locations
            .iter_mut()
            .find(|item| item.location_uri == location.location_uri)
        {
            *existing = location;
        } else {
            locations.push(location);
        }

        let healthy = locations.iter().filter(|loc| loc.is_healthy).count();
        println!(
            "registered shard location: model={}, shard={}, backend={}, redundancy={}",
            model_id, shard_index, backend_type, healthy
        );
        if healthy < TARGET_REDUNDANCY {
            eprintln!(
                "insufficient redundancy: model={}, shard={}, current={}, target={}",
                model_id, shard_index, healthy, TARGET_REDUNDANCY
            );
        }
        Ok(())
    }

    pub fn get_shard_locations(
        &self,
        model_id: &str,
        shard_index: u32,
    ) -> Result<Vec<ShardLocation>, ModelError> {
        check_shutdown()?;
        let key = (model_id.to_string(), shard_index);
        let entry = self.shards.get(&key).ok_or(ModelError::ShardNotFound)?;
        Ok(entry
            .locations
            .iter()
            .filter(|loc| loc.is_healthy)
            .cloned()
            .collect())
    }

    pub fn update_shard_health(
        &self,
        model_id: &str,
        shard_index: u32,
        location_uri: &str,
        is_healthy: bool,
    ) -> Result<(), ModelError> {
        check_shutdown()?;
        let key = (model_id.to_string(), shard_index);
        let mut entry = self.shards.get_mut(&key).ok_or(ModelError::ShardNotFound)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ModelError::Serialization("invalid system time".to_string()))?
            .as_secs() as i64;
        let location = entry
            .locations
            .iter_mut()
            .find(|loc| loc.location_uri == location_uri)
            .ok_or(ModelError::InvalidShard)?;
        location.is_healthy = is_healthy;
        location.last_verified = timestamp;
        Ok(())
    }

    pub fn get_shards_needing_replication(
        &self,
        target_redundancy: u8,
    ) -> Result<Vec<(String, u32)>, ModelError> {
        check_shutdown()?;
        let mut results = Vec::new();
        for entry in self.shards.iter() {
            let shard = entry.value();
            let healthy = shard.locations.iter().filter(|loc| loc.is_healthy).count();
            if healthy < target_redundancy as usize {
                results.push((shard.model_id.clone(), shard.shard_index));
            }
        }
        Ok(results)
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn is_valid_model_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn model_accessible_for_tier(metadata: &ModelMetadata, tier: SubscriptionTier) -> bool {
    if metadata.is_core_model {
        return true;
    }
    if metadata.is_experimental && tier != SubscriptionTier::Unlimited {
        return false;
    }
    if let Some(required) = metadata.minimum_tier {
        tier >= required
    } else {
        true
    }
}
