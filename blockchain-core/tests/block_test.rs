// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use blockchain_core::{Block, ChainId, GenesisConfig, Transaction};

fn mk_addr(n: u8) -> String {
    format!("0x{:0>40}", hex::encode([n; 20]))
}

#[test]
fn block_new_sets_merkle_and_hash() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 1, 1, 0, false, chain_id, None).unwrap();
    let b = Block::new([0u8; 32], vec![tx], 1, 1, None);
    assert_ne!(b.block_hash.0, [0u8; 32]);
    assert_ne!(b.header.merkle_root.0, [0u8; 32]);
}

#[test]
fn genesis_block_has_height_zero() {
    let cfg = GenesisConfig::default();
    let g = Block::genesis_block(&cfg);
    assert!(g.is_genesis());
    assert_eq!(g.header.previous_hash.0, [0u8; 32]);
}

#[test]
fn block_serialization_roundtrip() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 1, 1, 0, false, chain_id, None).unwrap();
    let b = Block::new([0u8; 32], vec![tx], 1, 1, None);
    let bytes = b.to_bytes().unwrap();
    let back = Block::from_bytes(&bytes).unwrap();
    assert_eq!(back.block_hash.0, b.block_hash.0);
}

#[test]
fn block_verify_rejects_bad_merkle() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 1, 1, 0, false, chain_id, None).unwrap();
    let mut b = Block::new([0u8; 32], vec![tx], 1, 1, None);
    b.header.merkle_root.0 = [9u8; 32];
    assert!(b.verify().is_err());
}
