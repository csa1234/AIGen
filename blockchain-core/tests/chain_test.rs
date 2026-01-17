use blockchain_core::{Block, Blockchain, Transaction};
use ed25519_dalek::SigningKey;
use ed25519_dalek::Signer;
use genesis::CEO_WALLET;
use genesis::ShutdownCommand;

fn reset_shutdown() {
    genesis::shutdown::reset_shutdown_for_tests();
}

fn mk_addr(n: u8) -> String {
    format!("0x{:0>40}", hex::encode([n; 20]))
}

fn dummy_poi_proof_bytes(miner_address: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "miner_address": miner_address,
        "difficulty": 1000u64,
        "work_type": "MatrixMultiplication",
        "verification_data": {"committee": []},
        "work_hash": vec![0u8; 32],
        "timestamp": 1i64,
        "nonce": 0u64,
        "input_hash": vec![0u8; 32],
        "output_data": [],
        "computation_metadata": {},
    }))
    .unwrap()
}

#[test]
fn mempool_rejects_insufficient_balance() {
    reset_shutdown();
    let mut c = Blockchain::new();
    let sender = mk_addr(1);
    let receiver = mk_addr(2);

    // sender has 0 by default
    let tx = Transaction::new(sender.clone(), receiver, 100, 1, 0, false, c.chain_id, None).unwrap();
    assert!(c.add_transaction_to_pool(tx).is_err());
}

#[test]
fn mempool_rejects_bad_nonce() {
    reset_shutdown();
    let mut c = Blockchain::new();
    let sender = mk_addr(1);
    let receiver = mk_addr(2);
    c.state.set_balance(sender.clone(), blockchain_core::types::Balance::zero().safe_add(blockchain_core::types::Amount::new(10_000)).unwrap());

    // expected nonce is 0, provide 1
    let tx = Transaction::new(sender, receiver, 1, 1, 1, false, c.chain_id, None).unwrap();
    assert!(c.add_transaction_to_pool(tx).is_err());
}

#[test]
fn ceo_shutdown_transaction_uses_payload_not_hash() {
    reset_shutdown();
    let mut c = Blockchain::new();

    // RFC8032 Ed25519 test vector 1 seed (produces public key d75a...511a).
    let seed: [u8; 32] = hex::decode(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
    )
    .unwrap()
    .try_into()
    .unwrap();
    let sk = SigningKey::from_bytes(&seed);

    let network_magic = c.genesis_config.network_magic;
    let mut cmd = ShutdownCommand {
        timestamp: 1,
        reason: "test".to_string(),
        ceo_signature: genesis::CeoSignature(ed25519_dalek::Signature::from_bytes(&[0u8; 64])),
        nonce: 42,
        network_magic,
    };

    let sig_cmd = sk.sign(&cmd.message_to_sign());
    cmd.ceo_signature = genesis::CeoSignature(sig_cmd);

    let payload = bincode::serialize(&cmd).unwrap();
    let mut tx = Transaction::new(
        CEO_WALLET.to_string(),
        "SHUTDOWN".to_string(),
        0,
        1,
        0,
        true,
        c.chain_id,
        Some(payload),
    )
    .unwrap();

    // CEO tx must also include ceo_signature over tx.message_to_sign (tx_hash)
    let sig_tx = sk.sign(&tx.tx_hash.0);
    tx.ceo_signature = Some(genesis::CeoSignature(sig_tx));

    assert!(c.process_ceo_command(&tx).is_ok());
    assert!(genesis::is_shutdown());
}

#[test]
fn blockchain_new_has_genesis_block() {
    reset_shutdown();
    let c = Blockchain::new();
    assert_eq!(c.blocks.len(), 1);
    assert!(c.blocks[0].is_genesis());
    assert_eq!(c.get_balance(CEO_WALLET), 0);
}

#[test]
fn mempool_orders_ceo_first() {
    reset_shutdown();
    let mut c = Blockchain::new();

    let chain_id = c.chain_id;
    // fund normal sender so it can be admitted to the pool under state validation
    let normal_sender = mk_addr(3);
    c.state.set_balance(
        normal_sender.clone(),
        blockchain_core::types::Balance::zero()
            .safe_add(blockchain_core::types::Amount::new(10_000))
            .unwrap(),
    );
    let tx_normal = Transaction::new(normal_sender, mk_addr(4), 1, 1, 0, true, chain_id, None).unwrap();
    let tx_ceo = Transaction::new(CEO_WALLET.to_string(), mk_addr(2), 1, 1, 0, true, chain_id, None).unwrap();

    c.add_transaction_to_pool(tx_normal).unwrap();
    c.add_transaction_to_pool(tx_ceo.clone()).unwrap();

    let pending = c.get_pending_transactions(10);
    assert_eq!(pending[0].sender, CEO_WALLET);
}

#[test]
fn add_block_applies_state() {
    reset_shutdown();
    let mut c = Blockchain::new();

    // seed sender funds
    let sender = mk_addr(1);
    let receiver = mk_addr(2);
    c.state
        .set_balance(sender.clone(), blockchain_core::types::Balance::zero().safe_add(blockchain_core::types::Amount::new(10_000)).unwrap());

    let tx = Transaction::new(sender.clone(), receiver.clone(), 100, 1, 0, false, c.chain_id, None).unwrap();
    let prev = c.blocks.last().unwrap().block_hash.0;
    let b = Block::new(prev, vec![tx], 1, 1, Some(dummy_poi_proof_bytes(&sender)));

    c.add_block(b).unwrap();

    assert_eq!(c.get_balance(&receiver), 100);
}

#[test]
fn add_block_rolls_back_on_failure() {
    reset_shutdown();
    let mut c = Blockchain::new();

    let sender = mk_addr(1);
    let receiver = mk_addr(2);
    c.state
        .set_balance(sender.clone(), blockchain_core::types::Balance::zero());

    let tx = Transaction::new(sender.clone(), receiver.clone(), 100, 1, 0, false, c.chain_id, None).unwrap();
    let prev = c.blocks.last().unwrap().block_hash.0;
    let b = Block::new(prev, vec![tx], 1, 1, Some(dummy_poi_proof_bytes(&sender)));

    assert!(c.add_block(b).is_err());
    assert_eq!(c.get_balance(&receiver), 0);
}
