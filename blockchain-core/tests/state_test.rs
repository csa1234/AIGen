// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use blockchain_core::types::{Amount, Balance};
use blockchain_core::{ChainId, ChainState, SubscriptionState, Transaction};
use genesis::CEO_WALLET;

fn mk_addr(n: u8) -> String {
    format!("0x{:0>40}", hex::encode([n; 20]))
}

#[test]
fn state_fee_distribution_when_validator_is_ceo_wallet() {
    let st = ChainState::new();
    let sender = mk_addr(9);
    let receiver = mk_addr(10);

    // Seed sender balance enough for amount + fee
    st.set_balance(
        sender.clone(),
        Balance::zero().safe_add(Amount::new(10_000)).unwrap(),
    );
    // Default validator reward address in ChainState::new is CEO_WALLET.

    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(
        sender.clone(),
        receiver.clone(),
        100,
        1,
        0,
        false,
        chain_id,
        None,
    )
    .unwrap();
    st.apply_transaction(&tx).unwrap();

    let ceo_balance = st.get_balance(CEO_WALLET).amount().value();
    // CEO should receive both validator_share and dev_share when validator == CEO.
    assert_eq!(
        ceo_balance,
        tx.fee.validator_share().value() + tx.fee.dev_share().value()
    );
}

#[test]
fn state_get_balance_default_zero() {
    let st = ChainState::new();
    assert_eq!(st.get_balance(&mk_addr(9)).amount().value(), 0);
}

#[test]
fn state_transfer_success() {
    let st = ChainState::new();
    let a = mk_addr(1);
    let b = mk_addr(2);
    st.set_balance(
        a.clone(),
        Balance::zero().safe_add(Amount::new(10)).unwrap(),
    );
    st.transfer(&a, &b, Amount::new(3)).unwrap();
    assert_eq!(st.get_balance(&a).amount().value(), 7);
    assert_eq!(st.get_balance(&b).amount().value(), 3);
}

#[test]
fn state_transfer_insufficient() {
    let st = ChainState::new();
    let a = mk_addr(1);
    let b = mk_addr(2);
    st.set_balance(a.clone(), Balance::zero());
    assert!(st.transfer(&a, &b, Amount::new(1)).is_err());
}

#[test]
fn state_apply_transaction_fee_distribution() {
    let st = ChainState::new();
    let sender = mk_addr(1);
    let receiver = mk_addr(2);

    // Seed balance enough for amount + fee
    st.set_balance(
        sender.clone(),
        Balance::zero().safe_add(Amount::new(10_000)).unwrap(),
    );
    st.set_validator_reward_address(mk_addr(7));

    let chain_id = ChainId::from_str_id("aigen-test");
    let tx = Transaction::new(
        sender.clone(),
        receiver.clone(),
        100,
        1,
        0,
        false,
        chain_id,
        None,
    )
    .unwrap();
    let fee_total = tx.fee.total_fee().value();

    st.apply_transaction(&tx).unwrap();

    assert_eq!(st.get_balance(&receiver).amount().value(), 100);

    let validator = st.get_balance(&mk_addr(7)).amount().value();
    assert_eq!(validator, tx.fee.validator_share().value());

    let dev = st.get_balance(CEO_WALLET).amount().value();
    assert_eq!(dev, tx.fee.dev_share().value());

    // burn is not tracked as an account; validate that distributed parts do not exceed total
    assert_eq!(
        tx.fee.validator_share().value() + tx.fee.dev_share().value() + tx.fee.burn_amount.value(),
        fee_total
    );
}

#[test]
fn snapshot_and_restore() {
    let st = ChainState::new();
    let a = mk_addr(1);
    st.set_balance(a.clone(), Balance::zero().safe_add(Amount::new(5)).unwrap());
    let snap = st.snapshot();
    st.mint_tokens(&a, Amount::new(5)).unwrap();
    assert_eq!(st.get_balance(&a).amount().value(), 10);
    st.restore(snap);
    assert_eq!(st.get_balance(&a).amount().value(), 5);
}

#[test]
fn snapshot_and_restore_subscription() {
    let st = ChainState::new();
    let address = mk_addr(3);
    let subscription = SubscriptionState {
        user_address: address.clone(),
        tier: 1,
        start_timestamp: 10,
        expiry_timestamp: 100,
        requests_used: 5,
        last_reset_timestamp: 10,
        auto_renew: true,
    };
    st.set_subscription(address.clone(), subscription.clone());
    let snap = st.snapshot();
    st.remove_subscription(&address).unwrap();
    assert!(st.get_subscription(&address).is_none());
    st.restore(snap);
    assert_eq!(st.get_subscription(&address), Some(subscription));
}

#[test]
fn state_root_changes_when_state_changes() {
    let st = ChainState::new();
    let r1 = st.calculate_state_root();
    st.mint_tokens(&mk_addr(1), Amount::new(1)).unwrap();
    let r2 = st.calculate_state_root();
    assert_ne!(r1, r2);
}

#[test]
fn state_root_changes_when_subscription_changes() {
    let st = ChainState::new();
    let r1 = st.calculate_state_root();
    let address = mk_addr(4);
    st.set_subscription(
        address.clone(),
        SubscriptionState {
            user_address: address,
            tier: 2,
            start_timestamp: 1,
            expiry_timestamp: 100,
            requests_used: 0,
            last_reset_timestamp: 1,
            auto_renew: false,
        },
    );
    let r2 = st.calculate_state_root();
    assert_ne!(r1, r2);
}
