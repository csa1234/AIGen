// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! G8 Safety Committee Election Scheduler
//!
//! This module handles automatic election triggers and finalization
//! based on term expiration and voting periods.

use crate::state::ChainState;
use crate::types::BlockchainError;

/// Check if any G8 member terms expire within 30 days and trigger new election
pub fn check_and_trigger_election(current_time: i64, state: &ChainState) -> Result<(), BlockchainError> {
    let config = genesis::GovernanceConfig::load();
    if !config.g8_enabled {
        return Ok(());
    }

    // First check if there's already an open election - if so, don't create another
    let snapshot = state.snapshot();
    let has_open_election = snapshot.g8_elections
        .values()
        .any(|e| e.status == 0); // status 0 = open
    
    if has_open_election {
        // Early return to avoid creating multiple concurrent elections
        return Ok(());
    }

    // Check if there's already an active election by using list_active_g8_members
    let active_members = state.list_active_g8_members();
    
    // Check for members with terms expiring within 30 days
    let mut needs_election = false;

    for member in &active_members {
        if member.term_end > current_time {
            let days_until_expiry = (member.term_end - current_time) / (24 * 60 * 60);
            if days_until_expiry <= 30 {
                needs_election = true;
                break;
            }
        }
    }

    // Also check if we have fewer than 8 active members
    if active_members.len() < 8 {
        needs_election = true;
    }

    if needs_election {
        let election_id = format!("election-{}", current_time);
        state.create_g8_election(election_id, current_time)?;
        eprintln!("G8 Election triggered at timestamp {}", current_time);
    }

    Ok(())
}

/// Auto-finalize elections that have passed their end timestamp
pub fn auto_finalize_elections(current_time: i64, state: &ChainState) -> Result<(), BlockchainError> {
    let config = genesis::GovernanceConfig::load();
    if !config.g8_enabled {
        return Ok(());
    }

    // We need to get election IDs that are open and expired
    // Since g8_elections is private, we'll use the snapshot to get election IDs
    let snapshot = state.snapshot();
    let election_ids: Vec<String> = snapshot.g8_elections
        .values()
        .filter(|e| e.status == 0 && e.end_timestamp < current_time)
        .map(|e| e.election_id.clone())
        .collect();

    for election_id in election_ids {
        match state.finalize_g8_election(&election_id) {
            Ok(elected) => {
                eprintln!("G8 Election {} finalized. Elected members: {:?}", election_id, elected);
            }
            Err(e) => {
                eprintln!("Failed to finalize G8 election {}: {:?}", election_id, e);
            }
        }
    }

    Ok(())
}

/// Run G8 scheduler tasks - should be called every block
pub fn run_g8_scheduler(current_time: i64, state: &ChainState) -> Result<(), BlockchainError> {
    check_and_trigger_election(current_time, state)?;
    auto_finalize_elections(current_time, state)?;
    Ok(())
}
