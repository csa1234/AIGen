// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use crate::block::BlockHeader;
use crate::transaction::Transaction;
use crate::types::{BlockHash, TxHash};
use base64::engine::general_purpose;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde_json;
use sha2::{Digest, Sha256};

pub type PublicKey = VerifyingKey;
pub type SecretKey = SigningKey;

pub fn hash_data(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut h = sha3::Keccak256::new();
    h.update(data);
    h.finalize().into()
}

pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn hash_block_header(header: &BlockHeader) -> BlockHash {
    // bincode for consistent structural encoding
    let bytes = bincode::serialize(header).unwrap_or_default();
    BlockHash(hash_data(&bytes))
}

pub fn hash_transaction(tx: &Transaction) -> TxHash {
    // Ensure we commit to chain_id and fee fields.
    let payload = serde_json::json!({
        "sender": tx.sender,
        "receiver": tx.receiver,
        "amount": tx.amount.value(),
        "timestamp": tx.timestamp.value(),
        "nonce": tx.nonce.value(),
        "priority": tx.priority,
        "chain_id": tx.chain_id.value(),
        "fee": {
            "base_fee": tx.fee.base_fee.value(),
            "priority_fee": tx.fee.priority_fee.value(),
            "burn_amount": tx.fee.burn_amount.value(),
        },
        "payload": tx.payload.as_ref().map(|p| general_purpose::STANDARD.encode(p)),
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    TxHash(hash_data(&bytes))
}

pub fn calculate_merkle_root(transactions: &[Transaction]) -> [u8; 32] {
    if transactions.is_empty() {
        return hash_data(&[][..]);
    }

    let mut hashes: Vec<[u8; 32]> = transactions.iter().map(|tx| tx.tx_hash.0).collect();

    while hashes.len() > 1 {
        let mut next = Vec::with_capacity(hashes.len().div_ceil(2));
        for pair in hashes.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            let mut buf = Vec::with_capacity(64);
            buf.extend_from_slice(&left);
            buf.extend_from_slice(&right);
            next.push(hash_data(&buf));
        }
        hashes = next;
    }

    hashes[0]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleSibling {
    pub hash: [u8; 32],
    pub is_left: bool,
}

pub fn generate_merkle_proof(
    transactions: &[Transaction],
    index: usize,
) -> Option<Vec<MerkleSibling>> {
    if transactions.is_empty() || index >= transactions.len() {
        return None;
    }

    let mut layer: Vec<[u8; 32]> = transactions.iter().map(|t| t.tx_hash.0).collect();
    let mut idx = index;
    let mut proof = Vec::new();

    while layer.len() > 1 {
        let is_right = idx % 2 == 1;
        let pair_idx = if is_right { idx - 1 } else { idx + 1 };
        let sibling_hash = if pair_idx < layer.len() {
            layer[pair_idx]
        } else {
            layer[idx]
        };
        proof.push(MerkleSibling {
            hash: sibling_hash,
            is_left: is_right, // if current is right child, sibling is left
        });

        let mut next = Vec::with_capacity(layer.len().div_ceil(2));
        for pair in layer.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            let mut buf = Vec::with_capacity(64);
            buf.extend_from_slice(&left);
            buf.extend_from_slice(&right);
            next.push(hash_data(&buf));
        }

        layer = next;
        idx /= 2;
    }

    Some(proof)
}

pub fn verify_merkle_proof(leaf_hash: [u8; 32], proof: &[MerkleSibling], root: [u8; 32]) -> bool {
    if proof.is_empty() {
        return leaf_hash == root;
    }

    let mut current = leaf_hash;
    for sib in proof.iter() {
        let mut buf = Vec::with_capacity(64);
        if sib.is_left {
            buf.extend_from_slice(&sib.hash);
            buf.extend_from_slice(&current);
        } else {
            buf.extend_from_slice(&current);
            buf.extend_from_slice(&sib.hash);
        }
        current = hash_data(&buf);
    }
    current == root
}

pub fn generate_keypair() -> (PublicKey, SecretKey) {
    let mut rng = OsRng;
    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(secret_bytes.as_mut());
    let secret = SigningKey::from_bytes(&secret_bytes);
    let public = secret.verifying_key();
    (public, secret)
}

pub fn sign_message(message: &[u8], secret_key: &SecretKey) -> Signature {
    secret_key.sign(message)
}

pub fn verify_signature(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    public_key.verify(message, signature).is_ok()
}

pub fn wallet_address_from_pubkey(pubkey: &PublicKey) -> String {
    let pk_bytes = pubkey.to_bytes();
    let hash = hash_data(pk_bytes.as_ref());
    // Ethereum-style last 20 bytes of hash
    let addr_bytes = &hash[12..];
    format!("0x{}", hex::encode(addr_bytes))
}

pub fn derive_address_from_pubkey(pubkey: &PublicKey) -> String {
    let pk_bytes = pubkey.to_bytes();
    let hash = keccak256(pk_bytes.as_ref());
    let addr_bytes = &hash[12..];
    let lower = hex::encode(addr_bytes);
    let addr_hash = keccak256(lower.as_bytes());

    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in lower.chars().enumerate() {
        let byte = addr_hash[i / 2];
        let nibble = if i % 2 == 0 {
            (byte >> 4) & 0x0f
        } else {
            byte & 0x0f
        };
        if ch.is_ascii_hexdigit() && ch.is_ascii_alphabetic() && nibble >= 8 {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn validate_address_format(address: &str) -> bool {
    if !address.starts_with("0x") {
        return false;
    }
    let hex_part = &address[2..];
    if hex_part.len() != 40 {
        return false;
    }
    hex_part.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut res = 0u8;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        res |= x ^ y;
    }
    res == 0
}

/// AES-GCM encryption for federated learning share bundles
/// 
/// Encrypts data using AES-256-GCM with an ephemeral key.
/// Returns (encrypted_data, nonce, ephemeral_public_key) for ECDH key exchange.
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

/// Encrypt data using AES-256-GCM with ephemeral key
/// 
/// # Arguments
/// * `data` - Data to encrypt
/// * `recipient_pubkey` - Recipient's X25519 public key (32 bytes) for ECDH
/// 
/// # Returns
/// (encrypted_data, nonce, ephemeral_pubkey) - Send all three to recipient
pub fn encrypt_with_aes_gcm(data: &[u8], recipient_x25519_pubkey: &[u8; 32]) -> Result<(Vec<u8>, Vec<u8>, [u8; 32]), String> {
    // Generate ephemeral X25519 keypair for ECDH
    let ephemeral_secret = x25519_dalek::EphemeralSecret::random_from_rng(&mut rand::rngs::OsRng);
    let ephemeral_public = x25519_dalek::PublicKey::from(&ephemeral_secret);
    let recipient_public = x25519_dalek::PublicKey::from(*recipient_x25519_pubkey);
    
    // Perform ECDH to get shared secret
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_public);
    
    // Derive AES key from shared secret using SHA256
    let aes_key = sha2::Sha256::digest(shared_secret.as_bytes());
    
    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt data
    let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| format!("Key init failed: {:?}", e))?;
    let encrypted = cipher.encrypt(nonce, data).map_err(|e| format!("Encryption failed: {:?}", e))?;
    
    Ok((encrypted, nonce_bytes.to_vec(), ephemeral_public.to_bytes()))
}

/// Decrypt data using AES-256-GCM with ephemeral key
/// 
/// # Arguments
/// * `encrypted_data` - Encrypted data
/// * `nonce` - Nonce used for encryption
/// * `ephemeral_pubkey` - Ephemeral public key from sender (X25519)
/// * `recipient_x25519_secret` - Recipient's X25519 secret key (32 bytes, clamped)
/// 
/// # Returns
/// Decrypted data
pub fn decrypt_with_aes_gcm(
    encrypted_data: &[u8],
    nonce: &[u8],
    ephemeral_pubkey: &[u8; 32],
    recipient_x25519_secret: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let ephemeral_public = x25519_dalek::PublicKey::from(*ephemeral_pubkey);
    
    // Convert bytes to StaticSecret for the recipient's persistent key
    let recipient_secret = x25519_dalek::StaticSecret::from(*recipient_x25519_secret);
    
    // Perform ECDH to get shared secret
    let shared_secret = recipient_secret.diffie_hellman(&ephemeral_public);
    
    // Derive AES key from shared secret
    let aes_key = sha2::Sha256::digest(shared_secret.as_bytes());
    
    // Decrypt data
    let nonce = Nonce::from_slice(nonce);
    let cipher = Aes256Gcm::new_from_slice(&aes_key).map_err(|e| format!("Key init failed: {:?}", e))?;
    let decrypted = cipher.decrypt(nonce, encrypted_data).map_err(|e| format!("Decryption failed: {:?}", e))?;
    
    Ok(decrypted)
}

/// Convert Ed25519 public key bytes to X25519 public key bytes
/// 
/// This uses the standard Montgomery u-coordinate conversion from Ed25519 to X25519.
/// Returns 32 bytes that can be used with x25519_dalek::PublicKey::from()
pub fn ed25519_pubkey_to_x25519(ed25519_pubkey: &[u8; 32]) -> Result<[u8; 32], String> {
    // Decompress Edwards point
    let edwards_point = curve25519_dalek::edwards::CompressedEdwardsY(*ed25519_pubkey)
        .decompress()
        .ok_or("Invalid Ed25519 public key: cannot decompress Edwards point")?;
    
    // Convert to Montgomery form (u-coordinate)
    let montgomery_point = edwards_point.to_montgomery();
    Ok(montgomery_point.to_bytes())
}

/// Derive X25519 secret key from Ed25519 secret key
/// 
/// Ed25519 secret keys are 64 bytes (seed + pubkey), but we only need the 32-byte seed
/// to derive the X25519 secret. The scalar is hashed from the seed.
pub fn ed25519_secret_to_x25519(ed25519_secret: &[u8; 64]) -> [u8; 32] {
    // Ed25519 secret is 64 bytes: first 32 are the seed, last 32 are the pubkey
    // For X25519, we need to hash the seed to get the scalar, then clamp it
    let mut hasher = sha2::Sha512::new();
    hasher.update(&ed25519_secret[0..32]);
    let hash = hasher.finalize();
    
    let mut scalar_bytes = [0u8; 32];
    scalar_bytes.copy_from_slice(&hash[0..32]);
    
    // Clamp the scalar for X25519
    scalar_bytes[0] &= 248;
    scalar_bytes[31] &= 127;
    scalar_bytes[31] |= 64;
    
    scalar_bytes
}

/// Sign encrypted bundle with sender's secret key
/// 
/// # Arguments
/// * `encrypted_bundle` - Encrypted data
/// * `nonce` - Nonce used for encryption
/// * `round_id` - Round ID as bytes
/// * `sender_secret` - Sender's Ed25519 secret key (64 bytes: seed || pubkey)
/// 
/// # Returns
/// 64-byte Ed25519 signature
pub fn sign_encrypted_bundle(
    encrypted_bundle: &[u8],
    nonce: &[u8],
    round_id: &[u8],
    sender_secret: &[u8; 64],
) -> Result<[u8; 64], String> {
    // Create SigningKey from the secret key bytes
    let signing_key = ed25519_dalek::SigningKey::try_from(sender_secret.as_slice())
        .map_err(|e| format!("Invalid secret key: {:?}", e))?;
    
    // Create message to sign: encrypted_bundle || nonce || round_id
    let mut message = Vec::with_capacity(encrypted_bundle.len() + nonce.len() + round_id.len());
    message.extend_from_slice(encrypted_bundle);
    message.extend_from_slice(nonce);
    message.extend_from_slice(round_id);
    
    let signature = signing_key.sign(&message);
    Ok(signature.to_bytes())
}

/// Verify signature on encrypted bundle
/// 
/// # Arguments
/// * `encrypted_bundle` - Encrypted data
/// * `nonce` - Nonce used for encryption
/// * `round_id` - Round ID as bytes
/// * `sender_pubkey` - Sender's Ed25519 public key (32 bytes)
/// * `signature` - 64-byte Ed25519 signature
/// 
/// # Returns
/// true if signature is valid
pub fn verify_encrypted_bundle(
    encrypted_bundle: &[u8],
    nonce: &[u8],
    round_id: &[u8],
    sender_pubkey: &[u8; 32],
    signature: &[u8; 64],
) -> Result<bool, String> {
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(sender_pubkey)
        .map_err(|e| format!("Invalid public key: {:?}", e))?;
    
    let sig = ed25519_dalek::Signature::from_bytes(signature);
    
    // Recreate message
    let mut message = Vec::with_capacity(encrypted_bundle.len() + nonce.len() + round_id.len());
    message.extend_from_slice(encrypted_bundle);
    message.extend_from_slice(nonce);
    message.extend_from_slice(round_id);
    
    match verifying_key.verify(&message, &sig) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
