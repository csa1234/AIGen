//! Offline script for signing SIP approvals.
//! In production this should load the CEO private key, verify the SIP contents, and sign an approval payload.

fn main() {
    eprintln!("This file is deprecated.");
    eprintln!("Use the canonical workspace binary instead:");
    eprintln!("  cargo run -p aigen-scripts --bin sign-sip-approval -- --key-file ./ceo_key.encrypted.json --proposal-id SIP-001 --proposal-hash <64-hex> --action approve --output sip_001_approval.json");
    std::process::exit(1);
}
