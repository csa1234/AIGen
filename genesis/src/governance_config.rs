// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_voting_period() -> u64 {
    168 // 7 days default
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceConfig {
    pub auto_approve_enabled: bool,
    pub min_approval_percentage: f64,
    pub min_participation_percentage: f64,
    #[serde(default = "default_voting_period")]
    pub voting_period_hours: u64,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            auto_approve_enabled: true,
            min_approval_percentage: 80.0,
            min_participation_percentage: 50.0,
            voting_period_hours: 168, // 7 days
        }
    }
}

impl GovernanceConfig {
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }
    
    fn config_path() -> PathBuf {
        std::env::var("AIGEN_GOVERNANCE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/governance_config.json"))
    }
}
