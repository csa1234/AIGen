use crate::authority::verify_ceo_signature;
use crate::config::GenesisConfig;
use crate::types::{GenesisError, ShutdownCommand};
#[cfg(not(any(test, feature = "test-utils")))]
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(any(test, feature = "test-utils"))]
use std::cell::{Cell, RefCell};
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::atomic::AtomicBool;
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::atomic::Ordering;
#[cfg(not(any(test, feature = "test-utils")))]
use std::sync::Mutex;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static SHUTDOWN_FLAG_TL: Cell<bool> = Cell::new(false);
    static SHUTDOWN_REGISTRY_TL: RefCell<ShutdownRegistry> = RefCell::new(ShutdownRegistry::default());
}

#[cfg(not(any(test, feature = "test-utils")))]
static SHUTDOWN_FLAG: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShutdownRegistry {
    pub commands: Vec<ShutdownCommand>,
    pub nonces: HashSet<u64>,
}

#[cfg(not(any(test, feature = "test-utils")))]
static SHUTDOWN_REGISTRY: Lazy<Mutex<ShutdownRegistry>> =
    Lazy::new(|| Mutex::new(ShutdownRegistry::default()));

fn shutdown_flag_load() -> bool {
    #[cfg(any(test, feature = "test-utils"))]
    {
        return SHUTDOWN_FLAG_TL.with(|f| f.get());
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        return SHUTDOWN_FLAG.load(Ordering::SeqCst);
    }
}

fn shutdown_flag_store(value: bool) {
    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_FLAG_TL.with(|f| f.set(value));
        return;
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
        return SHUTDOWN_REGISTRY_TL.with(|r| {
            let mut registry = r.borrow_mut();
            if registry.nonces.contains(&command.nonce) {
                return Err(GenesisError::ReplayAttack);
            }
            registry.nonces.insert(command.nonce);
            registry.commands.push(command);
            shutdown_flag_store(true);
            Ok(())
        });
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
        return Ok(());
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
        return SHUTDOWN_REGISTRY_TL.with(|r| r.borrow().clone());
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        return SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned")
            .clone();
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn reset_shutdown_for_tests() {
    shutdown_flag_store(false);

    #[cfg(any(test, feature = "test-utils"))]
    {
        SHUTDOWN_REGISTRY_TL.with(|r| *r.borrow_mut() = ShutdownRegistry::default());
        return;
    }

    #[cfg(not(any(test, feature = "test-utils")))]
    {
        let mut registry = SHUTDOWN_REGISTRY
            .lock()
            .expect("shutdown registry mutex poisoned");
        *registry = ShutdownRegistry::default();
    }
}
