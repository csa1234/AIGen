//! Offline CEO Ed25519 keypair generation.
//!
//! SECURITY NOTES:
//! - Run this on an air-gapped machine.
//! - Treat the encrypted private key file as highly sensitive.
//! - After generating a new keypair, update `genesis/src/config.rs` (`CEO_PUBLIC_KEY_HEX`) and
//!   recompile/redeploy nodes.
//!
//! EXAMPLE:
//!   cargo run -p aigen-scripts --bin generate-ceo-keypair -- \
//!     --output-dir . \
//!     --key-name ceo_key
use anyhow::{bail, Context, Result};
use blockchain_core::crypto;
use clap::Parser;
use rpassword::prompt_password;
use std::fs;
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(
    name = "generate-ceo-keypair",
    about = "Generate a new CEO Ed25519 keypair (offline) and encrypt the private key",
    long_about = "Generate a new CEO Ed25519 keypair intended for the AIGEN Genesis control layer.\n\nSECURITY NOTES:\n- Run this tool on an air-gapped machine.\n- Do not move plaintext private key material onto internet-connected systems.\n- Back up the encrypted key file to secure offline storage.\n\nOUTPUTS:\n- <key-name>.encrypted.json  (encrypted private key; portable JSON)\n- <key-name>.pub.hex         (public key hex)\n\nNEXT STEPS:\n1) Update genesis/src/config.rs (CEO_PUBLIC_KEY_HEX) with the emitted public key hex\n2) Recompile and redeploy all nodes\n\nEXAMPLE:\n  cargo run -p aigen-scripts --bin generate-ceo-keypair -- --output-dir . --key-name ceo_key"
)]
struct Args {
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,

    #[arg(long, default_value = "ceo_key")]
    key_name: String,

    #[arg(long)]
    no_encrypt: bool,
}

fn main() -> Result<()> {
    eprintln!("SECURITY WARNING: Run this on an air-gapped machine. You are responsible for CEO key custody.");

    let args = Args::parse();

    let (public, secret) = crypto::generate_keypair();
    let public_hex = hex::encode(public.to_bytes());

    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let output_dir = args.output_dir;
    let key_name = args.key_name;

    let pub_path = output_dir.join(format!("{key_name}.pub.hex"));
    fs::write(&pub_path, format!("{public_hex}\n"))
        .with_context(|| format!("failed to write {}", pub_path.display()))?;

    if args.no_encrypt {
        let priv_path = output_dir.join(format!("{key_name}.priv.hex"));
        let secret_hex = hex::encode(secret.to_bytes());
        fs::write(&priv_path, format!("{secret_hex}\n"))
            .with_context(|| format!("failed to write {}", priv_path.display()))?;

        println!("=== AIGEN CEO KEYPAIR GENERATED ===");
        println!("Public Key (hex): {public_hex}");
        println!("PLAINTEXT Private Key File: {}", priv_path.display());
        println!("Public Key File: {}", pub_path.display());
        println!();
        println!("NEXT STEPS:");
        println!("1. Delete the plaintext private key file after testing");
        println!("2. Update genesis/src/config.rs:");
        println!("   pub const CEO_PUBLIC_KEY_HEX: &str = \"{public_hex}\";");
        println!("3. Recompile all nodes with new genesis configuration");
        return Ok(());
    }

    let password = Zeroizing::new(prompt_password("Enter encryption password: ")?);
    let confirm = Zeroizing::new(prompt_password("Confirm encryption password: ")?);
    if password.as_str() != confirm.as_str() {
        bail!("passwords do not match");
    }

    let secret_bytes = secret.to_bytes();
    let encrypted = aigen_scripts::encrypt_private_key(&secret_bytes, password.as_str())?;

    let enc_path = output_dir.join(format!("{key_name}.encrypted.json"));
    aigen_scripts::save_encrypted_key(&enc_path, &encrypted)?;

    println!("=== AIGEN CEO KEYPAIR GENERATED ===");
    println!("Public Key (hex): {public_hex}");
    println!("Encrypted Private Key: {}", enc_path.display());
    println!("Public Key File: {}", pub_path.display());
    println!();
    println!("NEXT STEPS:");
    println!("1. Backup {key_name}.encrypted.json to secure offline storage");
    println!("2. Update genesis/src/config.rs:");
    println!("   pub const CEO_PUBLIC_KEY_HEX: &str = \"{public_hex}\";");
    println!("3. Recompile all nodes with new genesis configuration");
    println!("4. NEVER expose the encrypted key file to internet-connected systems");

    Ok(())
}
