use blockchain_core::types::*;

#[test]
fn block_height_validation() {
    assert!(BlockHeight::new_non_genesis(0).is_err());
    assert_eq!(BlockHeight::new_genesis().value(), 0);
    assert_eq!(BlockHeight::new_non_genesis(1).unwrap().value(), 1);
}

#[test]
fn block_version_support() {
    assert!(BlockVersion::new(BlockVersion::CURRENT).is_ok());
    assert!(BlockVersion::new(999).is_err());
    assert!(BlockVersion::current().is_supported());
}

#[test]
fn amount_arithmetic() {
    let a = Amount::new(10);
    let b = Amount::new(3);
    assert_eq!(a.safe_add(b).unwrap().value(), 13);
    assert_eq!(a.safe_sub(b).unwrap().value(), 7);
    assert!(b.safe_sub(a).is_err());
}

#[test]
fn nonce_increment() {
    let mut n = Nonce::ZERO;
    assert_eq!(n.value(), 0);
    n.increment();
    assert_eq!(n.value(), 1);
}

#[test]
fn timestamp_validation() {
    assert!(Timestamp::new(-1).is_err());
    assert!(Timestamp::new(1).is_ok());
}

#[test]
fn hash_hex_format_and_parse() {
    let bytes = [7u8; 32];
    let bh = BlockHash(bytes);
    let s = bh.to_string();
    assert!(s.starts_with("0x"));
    let parsed: BlockHash = s.parse().unwrap();
    assert_eq!(parsed.0, bytes);

    let th = TxHash(bytes);
    let s2 = th.to_string();
    let parsed2: TxHash = s2.parse().unwrap();
    assert_eq!(parsed2.0, bytes);
}

#[test]
fn fee_split_invariants() {
    let fee = Fee::new(Amount::new(100), Amount::new(50));
    assert_eq!(fee.total_fee().value(), 150);
    assert_eq!(fee.validator_share().value(), 75);
    assert_eq!(fee.burn_amount.value(), 60);
    assert_eq!(fee.dev_share().value(), 15);
}

#[test]
fn balance_safe_ops() {
    let b = Balance::zero().safe_add(Amount::new(10)).unwrap();
    assert_eq!(b.amount().value(), 10);
    assert!(b.safe_sub(Amount::new(11)).is_err());
}

#[test]
fn chain_id_deterministic() {
    let a = ChainId::from_str_id("aigen-mainnet-1");
    let b = ChainId::from_str_id("aigen-mainnet-1");
    let c = ChainId::from_str_id("aigen-testnet-1");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn address_validation() {
    assert!(validate_address("0x0000000000000000000000000000000000000001").is_ok());
    assert!(validate_address("nope").is_err());
}
