use blockchain_core::{
    blake3_hash, calculate_merkle_root, derive_address_from_pubkey, generate_keypair, generate_merkle_proof,
    hash_data, keccak256, verify_merkle_proof, ChainId, Transaction,
};

fn mk_addr(n: u8) -> String {
    format!("0x{:0>40}", hex::encode([n; 20]))
}

#[test]
fn hash_data_consistency() {
    let a = hash_data(b"abc");
    let b = hash_data(b"abc");
    let c = hash_data(b"abcd");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn keccak_and_blake3_different() {
    let k = keccak256(b"abc");
    let b = blake3_hash(b"abc");
    assert_ne!(k, b);
}

#[test]
fn merkle_root_and_proof_roundtrip() {
    let chain_id = ChainId::from_str_id("aigen-test");
    let txs: Vec<Transaction> = (0..5)
        .map(|i| {
            Transaction::new(
                mk_addr(1),
                mk_addr(2 + i as u8),
                1,
                1,
                i as u64,
                false,
                chain_id,
                None,
            )
            .unwrap()
        })
        .collect();

    let root = calculate_merkle_root(&txs);
    for idx in 0..txs.len() {
        let proof = generate_merkle_proof(&txs, idx).unwrap();
        let leaf = txs[idx].tx_hash.0;
        assert!(verify_merkle_proof(leaf, &proof, root));
    }
}

#[test]
fn derive_address_format() {
    let (pk, _) = generate_keypair();
    let addr = derive_address_from_pubkey(&pk);
    assert!(addr.starts_with("0x"));
    assert_eq!(addr.len(), 42);
}
