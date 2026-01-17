//! Offline script for signing shutdown commands.
//! In production this should load the CEO private key from secure storage and emit a signed ShutdownCommand JSON.

fn main() {
    eprintln!("This file is deprecated.");
    eprintln!("Use the canonical workspace binary instead:");
    eprintln!("  cargo run -p aigen-scripts --bin sign-shutdown-command -- --key-file ./ceo_key.encrypted.json --reason \"Emergency maintenance\" --nonce 1 --output shutdown_cmd.json");
    std::process::exit(1);
}
