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
