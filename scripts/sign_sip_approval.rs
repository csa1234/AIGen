// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Offline script for signing SIP approvals.
//! In production this should load the CEO private key, verify the SIP contents, and sign an approval payload.

fn main() {
    eprintln!("This file is deprecated.");
    eprintln!("Use the canonical workspace binary instead:");
    eprintln!("  cargo run -p aigen-scripts --bin sign-sip-approval -- --key-file ./ceo_key.encrypted.json --proposal-id SIP-001 --proposal-hash <64-hex> --action approve --output sip_001_approval.json");
    std::process::exit(1);
}
