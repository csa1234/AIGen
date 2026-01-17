//! Offline CEO keypair generation script.
//! Run this on an air-gapped machine.

fn main() {
    eprintln!("This file is deprecated.");
    eprintln!("Use the canonical workspace binary instead:");
    eprintln!("  cargo run -p aigen-scripts --bin generate-ceo-keypair -- --output-dir . --key-name ceo_key");
    std::process::exit(1);
}
