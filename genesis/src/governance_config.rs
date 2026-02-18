// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_voting_period() -> u64 {
    168 // 7 days default
}

fn default_g8_min_stake_percentage() -> f64 {
    0.1 // 0.1% of supply
}

fn default_g8_term_years() -> u32 {
    2
}

fn default_g8_cooloff_years() -> u32 {
    1
}

fn default_g8_quorum() -> u8 {
    5
}

fn default_g8_compensation_base() -> u64 {
    10_000 // AIGEN tokens
}

fn default_g8_compensation_bonus() -> u64 {
    2_000 // AIGEN tokens
}

fn default_dao_proposal_min_stake_percentage() -> f64 {
    0.5 // 0.5% of total staked supply
}

fn default_dao_voting_period_hours() -> u64 {
    48 // 48 hours for DAO proposal voting
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceConfig {
    pub auto_approve_enabled: bool,
    pub min_approval_percentage: f64,
    pub min_participation_percentage: f64,
    #[serde(default = "default_voting_period")]
    pub voting_period_hours: u64,
    #[serde(default = "default_g8_enabled")]
    pub g8_enabled: bool,
    #[serde(default = "default_g8_min_stake_percentage")]
    pub g8_min_stake_percentage: f64,
    #[serde(default = "default_g8_term_years")]
    pub g8_term_years: u32,
    #[serde(default = "default_g8_cooloff_years")]
    pub g8_cooloff_years: u32,
    #[serde(default = "default_g8_quorum")]
    pub g8_quorum: u8,
    #[serde(default = "default_g8_compensation_base")]
    pub g8_compensation_base: u64,
    #[serde(default = "default_g8_compensation_bonus")]
    pub g8_compensation_bonus: u64,
    #[serde(default = "default_dao_proposal_min_stake_percentage")]
    pub dao_proposal_min_stake_percentage: f64,
    #[serde(default = "default_dao_voting_period_hours")]
    pub dao_voting_period_hours: u64,
}

fn default_g8_enabled() -> bool {
    true
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            auto_approve_enabled: true,
            min_approval_percentage: 80.0,
            min_participation_percentage: 50.0,
            voting_period_hours: 168, // 7 days
            g8_enabled: true,
            g8_min_stake_percentage: 0.1,
            g8_term_years: 2,
            g8_cooloff_years: 1,
            g8_quorum: 5,
            g8_compensation_base: 10_000,
            g8_compensation_bonus: 2_000,
            dao_proposal_min_stake_percentage: 0.5, // 0.5% of total staked supply
            dao_voting_period_hours: 48, // 48 hours for DAO proposal voting
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
