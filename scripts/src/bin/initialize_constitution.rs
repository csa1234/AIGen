// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

//! Constitution initialization script
//!
//! This script serializes the constitution and outputs the hash
//! for on-chain registration via CEO signature.

use std::fs;
use std::path::PathBuf;

fn main() {
    println!("AIGEN Constitution Initialization");
    println!("=================================\n");

    // Compute constitution hash
    let hash = genesis::compute_constitution_hash();
    let hash_hex = hex::encode(hash);
    
    println!("Constitution Hash (SHA3-256): {}", hash_hex);
    println!();

    // Serialize constitution
    let serialized = genesis::serialize_constitution()
        .expect("Failed to serialize constitution");
    
    println!("Serialized constitution size: {} bytes", serialized.len());
    println!();

    // Print principle summary
    use genesis::{get_principles_by_category, PrincipleCategory};
    
    println!("Constitutional Principles Summary:");
    println!("- Love & Compassion: {} principles", get_principles_by_category(PrincipleCategory::LoveAndCompassion).len());
    println!("- Truth & Integrity: {} principles", get_principles_by_category(PrincipleCategory::TruthAndIntegrity).len());
    println!("- Peace & Non-Violence: {} principles", get_principles_by_category(PrincipleCategory::PeaceAndNonViolence).len());
    println!("- Humility & Service: {} principles", get_principles_by_category(PrincipleCategory::HumilityAndService).len());
    println!("- Purity & Chastity: {} principles", get_principles_by_category(PrincipleCategory::PurityAndChastity).len());
    println!("- Justice & Honesty: {} principles", get_principles_by_category(PrincipleCategory::JusticeAndHonesty).len());
    println!("- Family & Relationships: {} principles", get_principles_by_category(PrincipleCategory::FamilyAndRelationships).len());
    println!("- Science & Knowledge: {} principles", get_principles_by_category(PrincipleCategory::ScienceAndKnowledge).len());
    println!("- Technology & AI: {} principles", get_principles_by_category(PrincipleCategory::TechnologyAndAI).len());
    println!("- Governance & Transparency: {} principles", get_principles_by_category(PrincipleCategory::GovernanceAndTransparency).len());
    println!();

    // Create data directory
    let data_dir = PathBuf::from("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // Save constitution data
    let init_data = serde_json::json!({
        "constitution_hash": hash_hex,
        "total_principles": genesis::CONSTITUTION.len(),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        "message_to_sign": format!("constitution_init:{}", hash_hex),
    });

    let init_path = data_dir.join("constitution_init.json");
    fs::write(&init_path, serde_json::to_string_pretty(&init_data).unwrap())
        .expect("Failed to write constitution init file");

    println!("Constitution initialization data saved to: {}", init_path.display());
    println!();
    
    // Print next steps
    println!("Next Steps:");
    println!("-----------");
    println!("1. Upload constitution to IPFS (manual step):");
    println!("   - Use: curl -X POST 'http://localhost:5001/api/v0/add' -F file=@data/constitution.json");
    println!("   - Or upload via IPFS web UI");
    println!();
    println!("2. Get IPFS CID from response");
    println!();
    println!("3. Sign constitution update message with CEO key:");
    println!("   Message format: update_constitution:<network_magic>:<timestamp>:<ipfs_cid>");
    println!();
    println!("4. Call updateConstitution RPC with:");
    println!("   - ipfs_cid: The CID from step 2");
    println!("   - signature: CEO signature from step 3");
    println!("   - timestamp: Same timestamp used in signature");
    println!();
}
