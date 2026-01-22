//! Offline signing tool for SIP approvals and vetoes.
//!
//! WORKFLOW:
//! 1. Review SIP proposal content offline.
//! 2. Compute a 32-byte hash of the proposal content (hex encoded; 64 hex chars).
//! 3. Use this tool to sign an approval (`approve`) or veto (`veto`) message.
//! 4. Deliver the JSON output to operators via a secure channel.
//!
//! SECURITY NOTES:
//! - Run this tool on an air-gapped machine.
//! - Protect the encrypted key file and password.
//!
//! EXAMPLE:
//!   cargo run -p aigen-scripts --bin sign-sip-approval -- \
//!     --key-file ./ceo_key.encrypted.json \
//!     --proposal-id "SIP-001" \
//!     --proposal-hash "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" \
//!     --action approve \
//!     --output sip_001_approval.json
use anyhow::{bail, Context, Result};
use blockchain_core::crypto;
use clap::Parser;
use ed25519_dalek::SigningKey;
use genesis::CeoSignature;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(
    name = "sign-sip-approval",
    about = "Sign a SIP approval or veto payload (offline)",
    long_about = "Sign a SIP approval or veto message for the AIGEN Genesis control layer.\n\nINPUTS:\n- proposal-id: SIP identifier (e.g. SIP-001)\n- proposal-hash: 32-byte hex digest (64 hex chars) of the proposal content\n- action: approve|veto\n\nSECURITY NOTES:\n- Run on an air-gapped machine.\n- Protect the encrypted key file and password.\n\nOUTPUT:\n- Pretty-printed JSON containing proposal metadata and a CEO signature.\n\nEXAMPLE:\n  cargo run -p aigen-scripts --bin sign-sip-approval -- --key-file ./ceo_key.encrypted.json --proposal-id SIP-001 --proposal-hash <64-hex> --action approve --output sip_001_approval.json"
)]
struct Args {
    #[arg(long)]
    key_file: PathBuf,

    #[arg(long)]
    proposal_id: String,

    #[arg(long)]
    proposal_hash: String,

    #[arg(long)]
    action: String,

    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SipApprovalOutput {
    proposal_id: String,
    proposal_hash: String,
    action: String,
    signature: CeoSignature,
    timestamp: i64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let encrypted = aigen_scripts::load_encrypted_key(&args.key_file)?;
    let password = Zeroizing::new(prompt_password("Enter decryption password: ")?);
    let secret_bytes = Zeroizing::new(aigen_scripts::decrypt_private_key(
        &encrypted,
        password.as_str(),
    )?);
    let signing_key = SigningKey::from_bytes(&secret_bytes);

    let proposal_hash_bytes = parse_hash_32(&args.proposal_hash)?;

    let action = args.action.to_lowercase();
    if action != "approve" && action != "veto" {
        bail!("invalid action: {} (expected approve|veto)", args.action);
    }

    let msg = match action.as_str() {
        "approve" => format!("sip_approve:{}:{:x?}", args.proposal_id, proposal_hash_bytes),
        "veto" => format!("sip_veto:{}:{:x?}", args.proposal_id, proposal_hash_bytes),
        _ => unreachable!(),
    }
    .into_bytes();

    let sig = crypto::sign_message(&msg, &signing_key);

    let out = SipApprovalOutput {
        proposal_id: args.proposal_id,
        proposal_hash: args.proposal_hash,
        action,
        signature: CeoSignature(sig),
        timestamp: chrono::Utc::now().timestamp(),
    };

    let json = serde_json::to_vec_pretty(&out).context("failed to serialize sip approval")?;

    match args.output {
        Some(path) => {
            fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
        }
        None => {
            println!("{}", String::from_utf8_lossy(&json));
        }
    }

    Ok(())
}

fn parse_hash_32(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).context("invalid hex")?;
    if bytes.len() != 32 {
        bail!("proposal-hash must be 32 bytes (got {})", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
