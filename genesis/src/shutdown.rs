// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

#![cfg_attr(
    any(test, feature = "test-utils"),
    allow(clippy::missing_const_for_thread_local)
)]

use crate::authority::verify_ceo_signature;
use crate::config::GenesisConfig;
use crate::types::{AutoShutdownRecord, GenesisError, ShutdownCommand, ShutdownTrigger};
#[cfg(not(any(test, feature = "test-utils")))]
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "test-utils"))]
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::atomic::AtomicBool;
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::atomic::Ordering;
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::Mutex;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static SHUTDOWN_FLAG_TL: Cell<bool> = const { Cell::new(false) };
    static SHUTDOWN_REGISTRY_TL: RefCell<ShutdownRegistry> = RefCell::new(ShutdownRegistry::default());
}

#[cfg(not(any(test, feature = "test-utils")))]
static SHUTDOWN_FLAG: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShutdownRegistry {
    pub commands: Vec<ShutdownCommand>,
    pub nonces: HashSet<u64>,
    /// Records of automated safety shutdowns (no CEO signature required)
    pub auto_triggers: Vec<AutoShutdownRecord>,
}

#[cfg(not(any(test, feature = "test-utils")))]
static SHUTDOWN_REGISTRY: Lazy<Mutex<ShutdownRegistry>> =
    Lazy::new(|| Mutex::new(ShutdownRegistry::default()));

fn shutdown_flag_load() -> bool {
    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_FLAG_TL.with(|f| f.get())
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        SHUTDOWN_FLAG.load(Ordering::SeqCst)
    }
}

fn shutdown_flag_store(value: bool) {
    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_FLAG_TL.with(|f| f.set(value));
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        SHUTDOWN_FLAG.store(value, Ordering::SeqCst);
    }
}

pub fn emergency_shutdown(command: ShutdownCommand) -> Result<(), GenesisError> {
    if shutdown_flag_load() {
        return Err(GenesisError::ShutdownActive);
    }

    let expected_magic = GenesisConfig::default().network_magic;
    if command.network_magic != expected_magic {
        return Err(GenesisError::InvalidNetworkMagic);
    }

    // Verify CEO signature over the canonical shutdown message
    verify_ceo_signature(&command.message_to_sign(), &command.ceo_signature)?;

    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| {
            let mut registry = r.borrow_mut();
            if registry.nonces.contains(&command.nonce) {
                return Err(GenesisError::ReplayAttack);
            }
            registry.nonces.insert(command.nonce);
            registry.commands.push(command);
            shutdown_flag_store(true);
            Ok(())
        })
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        let mut registry = SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned");

        if registry.nonces.contains(&command.nonce) {
            return Err(GenesisError::ReplayAttack);
        }

        registry.nonces.insert(command.nonce);
        registry.commands.push(command);
        shutdown_flag_store(true);
        Ok(())
    }
}

pub fn is_shutdown() -> bool {
    shutdown_flag_load()
}

pub fn check_shutdown() -> Result<(), GenesisError> {
    if is_shutdown() {
        Err(GenesisError::ShutdownActive)
    } else {
        Ok(())
    }
}

pub fn shutdown_registry() -> ShutdownRegistry {
    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| r.borrow().clone())
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned")
            .clone()
    }
}

/// Triggers an automated safety shutdown without requiring a CEO signature.
/// 
/// This function is called by the safety pipeline (veto.rs, orchestrator.rs) when
/// critical safety conditions are violated. It does not require CEO authorization
/// and is designed for immediate response to safety threats.
///
/// # Arguments
/// * `trigger` - The type of safety condition that triggered the shutdown
/// * `details` - Human-readable description of the shutdown cause
///
/// # Returns
/// * `Ok(())` if the shutdown was successfully triggered
/// * `Err(GenesisError::ShutdownActive)` if a shutdown is already active
pub fn auto_safety_shutdown(trigger: ShutdownTrigger, details: String) -> Result<(), GenesisError> {
    if shutdown_flag_load() {
        return Err(GenesisError::ShutdownActive);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let record = AutoShutdownRecord {
        trigger: trigger.clone(),
        details,
        timestamp,
    };

    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| {
            let mut registry = r.borrow_mut();
            registry.auto_triggers.push(record);
            shutdown_flag_store(true);
            Ok(())
        })
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        let mut registry = SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned");
        registry.auto_triggers.push(record);
        shutdown_flag_store(true);
        Ok(())
    }
}

/// Returns the trigger from the most recent automated safety shutdown.
///
/// # Returns
/// * `Some(ShutdownTrigger)` if the shutdown was triggered by an automated safety condition
/// * `None` if the shutdown was CEO-manual (i.e., `auto_triggers` is empty but `commands` is not)
pub fn get_shutdown_trigger() -> Option<ShutdownTrigger> {
    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| {
            let registry = r.borrow();
            registry.auto_triggers.last().map(|r| r.trigger.clone())
        })
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        let registry = SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned");
        registry.auto_triggers.last().map(|r| r.trigger.clone())
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn reset_shutdown_for_tests() {
    shutdown_flag_store(false);

    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| *r.borrow_mut() = ShutdownRegistry::default());
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        let mut registry = SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned");
        *registry = ShutdownRegistry::default();
    }
}
