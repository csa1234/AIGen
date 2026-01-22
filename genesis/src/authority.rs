use crate::config::{parse_ceo_public_key_bytes, CEO_WALLET};
use crate::types::{CeoSignature, GenesisError};
use ed25519_dalek::VerifyingKey;
use once_cell::sync::Lazy;

pub struct CeoAuthority {
    verifying_key: VerifyingKey,
}

impl CeoAuthority {
    pub fn new() -> Result<Self, GenesisError> {
        let bytes = parse_ceo_public_key_bytes()?;
        let verifying_key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| GenesisError::InvalidPublicKey)?;
        Ok(Self { verifying_key })
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

static CEO_AUTHORITY: Lazy<CeoAuthority> = Lazy::new(|| {
    CeoAuthority::new().expect("CEO_PUBLIC_KEY_HEX must be a valid Ed25519 public key")
});

pub fn verify_ceo_signature(message: &[u8], signature: &CeoSignature) -> Result<(), GenesisError> {
    use ed25519_dalek::Verifier;

    CEO_AUTHORITY
        .verifying_key()
        .verify(message, signature.inner())
        .map_err(|_| GenesisError::InvalidSignature)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut res = 0u8;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        res |= x ^ y;
    }
    res == 0
}

pub fn is_ceo_wallet(address: &str) -> bool {
    constant_time_eq(address, CEO_WALLET)
}

pub trait CeoTransactable {
    fn sender_address(&self) -> &str;
    fn is_priority(&self) -> bool;
    fn ceo_signature(&self) -> Option<&CeoSignature>;
    fn message_to_sign(&self) -> Vec<u8>;
}

pub fn verify_ceo_transaction<T: CeoTransactable>(tx: &T) -> Result<(), GenesisError> {
    if !is_ceo_wallet(tx.sender_address()) {
        return Err(GenesisError::UnauthorizedCaller);
    }
    if !tx.is_priority() {
        return Err(GenesisError::UnauthorizedCaller);
    }

    let sig = tx.ceo_signature().ok_or(GenesisError::InvalidSignature)?;

    verify_ceo_signature(&tx.message_to_sign(), sig)
}

#[macro_export]
macro_rules! require_ceo {
    ($address:expr, $signature:expr, $message:expr) => {{
        if !$crate::authority::is_ceo_wallet($address) {
            return Err($crate::types::GenesisError::UnauthorizedCaller);
        }
        $crate::authority::verify_ceo_signature($message, $signature)
    }};
}
