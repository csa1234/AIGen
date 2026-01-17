//! Offline signing tool for AIGEN emergency shutdown commands.
//!
//! SECURITY NOTES:
//! - Run this tool on an air-gapped machine.
//! - The `--nonce` must be monotonically increasing (unique per network) to avoid replay.
//! - The `--network-magic` must match the target network.
//! - Deliver the signed JSON to operators via a secure out-of-band channel.
//!
//! EXAMPLE:
//!   cargo run -p aigen-scripts --bin sign-shutdown-command -- \
//!     --key-file ./ceo_key.encrypted.json \
//!     --reason "Emergency maintenance" \
//!     --nonce 1 \
//!     --network-magic 2711576721 \
//!     --output shutdown_cmd.json
use anyhow::{Context, Result};
use blockchain_core::crypto;
use clap::Parser;
use ed25519_dalek::SigningKey;
use genesis::config::DEFAULT_NETWORK_MAGIC;
use genesis::{CeoSignature, ShutdownCommand};
use rpassword::prompt_password;
use std::fs;
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(
    name = "sign-shutdown-command",
    about = "Sign an emergency ShutdownCommand JSON payload (offline)",
    long_about = "Sign an emergency shutdown command for the AIGEN network using the CEO private key.\n\nSECURITY NOTES:\n- Run on an air-gapped machine.\n- Protect the encrypted key file and password.\n- Nonce must be unique/monotonic per network to prevent replay.\n- Network magic must match the target network instance.\n\nOUTPUT:\n- Pretty-printed JSON compatible with genesis::ShutdownCommand.\n\nEXAMPLE:\n  cargo run -p aigen-scripts --bin sign-shutdown-command -- --key-file ./ceo_key.encrypted.json --reason \"Emergency maintenance\" --nonce 1 --network-magic 2711576721 --output shutdown_cmd.json"
)]
struct Args {
    #[arg(long)]
    key_file: PathBuf,

    #[arg(long)]
    reason: String,

    #[arg(long)]
    nonce: u64,

    #[arg(long, default_value_t = DEFAULT_NETWORK_MAGIC)]
    network_magic: u64,

    #[arg(long)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let encrypted = aigen_scripts::load_encrypted_key(&args.key_file)?;
    let password = Zeroizing::new(prompt_password("Enter decryption password: ")?);
    let secret_bytes = Zeroizing::new(aigen_scripts::decrypt_private_key(
        &encrypted,
        password.as_str(),
    )?);

    let signing_key = SigningKey::from_bytes(&*secret_bytes);

    let timestamp = chrono::Utc::now().timestamp();
    let message = format!(
        "shutdown:{}:{}:{}:{}",
        args.network_magic, timestamp, args.nonce, args.reason
    )
    .into_bytes();

    let signature = crypto::sign_message(&message, &signing_key);

    let cmd = ShutdownCommand {
        timestamp,
        reason: args.reason,
        ceo_signature: CeoSignature(signature),
        nonce: args.nonce,
        network_magic: args.network_magic,
    };

    let json = serde_json::to_vec_pretty(&cmd).context("failed to serialize ShutdownCommand")?;

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
