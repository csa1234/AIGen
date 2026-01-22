use aigen_scripts::{
    decrypt_private_key, encrypt_private_key, load_encrypted_key, save_encrypted_key,
};
use blockchain_core::crypto;
use ed25519_dalek::SigningKey;
use genesis::{verify_ceo_signature, CeoSignature, ShutdownCommand};
use tempfile::tempdir;

#[test]
fn encryption_roundtrip_works() {
    let secret = [42u8; 32];
    let enc = encrypt_private_key(&secret, "correct horse battery staple").unwrap();
    let dec = decrypt_private_key(&enc, "correct horse battery staple").unwrap();
    assert_eq!(dec, secret);
}

#[test]
fn invalid_password_fails_decryption() {
    let secret = [7u8; 32];
    let enc = encrypt_private_key(&secret, "pw1").unwrap();
    let err = decrypt_private_key(&enc, "pw2").unwrap_err();
    assert!(err.to_string().contains("decryption failed"));
}

#[test]
fn save_load_roundtrip_works() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ceo.encrypted.json");

    let secret = [1u8; 32];
    let enc = encrypt_private_key(&secret, "pw").unwrap();
    save_encrypted_key(&path, &enc).unwrap();

    let loaded = load_encrypted_key(&path).unwrap();
    let dec = decrypt_private_key(&loaded, "pw").unwrap();
    assert_eq!(dec, secret);
}

#[test]
fn keypair_generation_produces_valid_ed25519() {
    let (public, secret) = crypto::generate_keypair();
    let msg = b"hello";
    let sig = crypto::sign_message(msg, &secret);
    assert!(crypto::verify_signature(msg, &sig, &public));
}

#[test]
fn shutdown_command_signature_verifies_with_genesis_test_vector_key() {
    // This uses the RFC8032 Ed25519 test vector #1 secret key:
    // secret scalar seed:
    // 9d61b19deffd5a60ba844af492ec2cc4
    // 4449c5697b326919703bac031cae7f60
    // which corresponds to the public key hardcoded in genesis/src/config.rs by default.
    let secret_hex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    let secret_bytes_vec = hex::decode(secret_hex).unwrap();
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&secret_bytes_vec);

    let signing_key = SigningKey::from_bytes(&secret_bytes);

    let timestamp = 1734220800i64;
    let reason = "Critical security vulnerability".to_string();
    let nonce = 1u64;
    let network_magic = genesis::config::DEFAULT_NETWORK_MAGIC;

    let message = format!(
        "shutdown:{}:{}:{}:{}",
        network_magic, timestamp, nonce, reason
    )
    .into_bytes();

    let sig = crypto::sign_message(&message, &signing_key);

    let cmd = ShutdownCommand {
        timestamp,
        reason,
        ceo_signature: CeoSignature(sig),
        nonce,
        network_magic,
    };

    verify_ceo_signature(&cmd.message_to_sign(), &cmd.ceo_signature).unwrap();
}

#[test]
fn sip_approval_and_veto_signatures_verify_with_genesis_test_vector_key() {
    let secret_hex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    let secret_bytes_vec = hex::decode(secret_hex).unwrap();
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&secret_bytes_vec);

    let signing_key = SigningKey::from_bytes(&secret_bytes);

    let proposal_id = "SIP-001".to_string();
    let mut proposal_hash = [0u8; 32];
    proposal_hash[0] = 0xa3;

    let approve_msg = format!("sip_approve:{}:{:x?}", proposal_id, proposal_hash).into_bytes();
    let approve_sig = crypto::sign_message(&approve_msg, &signing_key);
    verify_ceo_signature(&approve_msg, &CeoSignature(approve_sig)).unwrap();

    let veto_msg = format!("sip_veto:{}:{:x?}", proposal_id, proposal_hash).into_bytes();
    let veto_sig = crypto::sign_message(&veto_msg, &signing_key);
    verify_ceo_signature(&veto_msg, &CeoSignature(veto_sig)).unwrap();
}
