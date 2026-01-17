//! Genesis CEO control layer for the AIGEN blockchain.
//! 
//! This crate hardcodes the CEO wallet and public key, verifies CEO signatures,
//! exposes an irreversible global shutdown flag, and manages SIP approval/veto
//! as described in the CRITICAL PRIME DIRECTIVE of `spec.md`.

pub mod config;
pub mod types;
pub mod authority;
pub mod shutdown;
pub mod veto;

pub use crate::config::{GenesisConfig, CEO_PUBLIC_KEY_HEX, CEO_WALLET};
pub use crate::types::{CeoSignature, ShutdownCommand, SipProposal, WalletAddress, GenesisError};
pub use crate::authority::{CeoAuthority, CeoTransactable, verify_ceo_signature, is_ceo_wallet, verify_ceo_transaction};
pub use crate::shutdown::{emergency_shutdown, is_shutdown, check_shutdown, ShutdownRegistry, shutdown_registry};
pub use crate::veto::{SipRegistry, SipStatus, submit_sip, veto_sip, approve_sip, can_deploy_sip, get_sip_status};
