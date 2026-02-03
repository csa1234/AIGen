// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use blockchain_core::{generate_keypair, ChainId, Transaction, TransactionPool};
use genesis::CEO_WALLET;

fn mk_addr(n: u8) -> String {
    format!("0x{:0>40}", hex::encode([n; 20]))
}

#[test]
fn transaction_new_has_hash_and_fee() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 10, 1, 0, false, chain_id, None).unwrap();
    assert_ne!(tx.tx_hash.0, [0u8; 32]);
    assert!(tx.fee.total_fee().value() > 0);
}

#[test]
fn transaction_sign_and_verify() {
    let (pk, sk) = generate_keypair();
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 10, 1, 0, false, chain_id, None)
        .unwrap()
        .sign(&sk);
    assert!(tx.verify(&pk));
}

#[test]
fn transaction_validate_rejects_same_sender_receiver() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(1), 10, 1, 0, false, chain_id, None).unwrap();
    assert!(tx.validate().is_err());
}

#[test]
fn transaction_total_cost_includes_fees() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 10, 1, 0, false, chain_id, None).unwrap();
    let cost = tx.total_cost().unwrap().value();
    assert!(cost >= 10);
}

#[test]
fn ceo_transactions_fee_free_and_prioritized() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx1 = Transaction::new(
        CEO_WALLET.to_string(),
        mk_addr(2),
        1,
        1,
        0,
        true,
        chain_id,
        None,
    )
    .unwrap();
    assert_eq!(tx1.fee.total_fee().value(), 0);

    let tx2 = Transaction::new(mk_addr(3), mk_addr(4), 1, 1, 0, true, chain_id, None).unwrap();

    let mut pool = TransactionPool::new();
    pool.push(tx2);
    pool.push(tx1.clone());

    let first = pool.pop_next().unwrap();
    assert_eq!(first.sender, CEO_WALLET);
}

#[test]
fn pool_remove_by_hash() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(mk_addr(1), mk_addr(2), 10, 1, 0, false, chain_id, None).unwrap();
    let h = tx.tx_hash;
    let mut pool = TransactionPool::new();
    pool.push(tx);
    assert!(pool.contains(&h));
    let removed = pool.remove_by_hash(&h);
    assert!(removed.is_some());
    assert!(!pool.contains(&h));
}

#[test]
fn pool_validate_duplicate_nonce_same_sender() {
    let mut pool = TransactionPool::new();
    let a = mk_addr(1);
    let chain_id = ChainId::from_str_id("aigen-test");
    pool.push(Transaction::new(a.clone(), mk_addr(2), 1, 1, 0, false, chain_id, None).unwrap());
    pool.push(Transaction::new(a.clone(), mk_addr(3), 1, 1, 0, false, chain_id, None).unwrap());
    assert!(pool.validate_pool().is_err());
}
