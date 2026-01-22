use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedKey {
    #[serde(with = "hex_serde_array_32")]
    pub salt: [u8; 32],
    #[serde(with = "hex_serde_array_12")]
    pub nonce: [u8; 12],
    #[serde(with = "hex_serde_vec")]
    pub ciphertext: Vec<u8>,
    pub version: u8,
}

pub fn encrypt_private_key(secret_key: &[u8; 32], password: &str) -> Result<EncryptedKey> {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);

    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);

    let params =
        Params::new(65_536, 3, 4, Some(32)).map_err(|e| anyhow!("invalid argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut derived_key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), &salt, derived_key.as_mut())
        .map_err(|e| anyhow!("argon2 failure: {e}"))?;

    let cipher = ChaCha20Poly1305::new(Key::from_slice(derived_key.as_ref()));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), secret_key.as_slice())
        .map_err(|e| anyhow!("encryption failure: {e}"))?;

    Ok(EncryptedKey {
        salt,
        nonce,
        ciphertext,
        version: 1,
    })
}

pub fn decrypt_private_key(encrypted: &EncryptedKey, password: &str) -> Result<[u8; 32]> {
    if encrypted.version != 1 {
        bail!("unsupported encrypted key version: {}", encrypted.version);
    }

    let params =
        Params::new(65_536, 3, 4, Some(32)).map_err(|e| anyhow!("invalid argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut derived_key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), &encrypted.salt, derived_key.as_mut())
        .map_err(|e| anyhow!("argon2 failure: {e}"))?;

    let cipher = ChaCha20Poly1305::new(Key::from_slice(derived_key.as_ref()));
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&encrypted.nonce),
            encrypted.ciphertext.as_slice(),
        )
        .map_err(|_| anyhow!("decryption failed"))?;

    if plaintext.len() != 32 {
        bail!("invalid decrypted key length: {}", plaintext.len());
    }

    let mut out = [0u8; 32];
    out.copy_from_slice(&plaintext);
    Ok(out)
}

pub fn save_encrypted_key(path: &Path, encrypted: &EncryptedKey) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let json = serde_json::to_vec_pretty(encrypted).context("failed to serialize encrypted key")?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(())
}

pub fn load_encrypted_key(path: &Path) -> Result<EncryptedKey> {
    let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let key: EncryptedKey =
        serde_json::from_slice(&data).context("failed to parse encrypted key JSON")?;

    if key.ciphertext.is_empty() {
        bail!("invalid encrypted key: empty ciphertext");
    }

    Ok(key)
}

mod hex_serde_array_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let vec = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if vec.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&vec);
        Ok(out)
    }
}

mod hex_serde_array_12 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 12], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 12], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let vec = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if vec.len() != 12 {
            return Err(serde::de::Error::custom("expected 12 bytes"));
        }
        let mut out = [0u8; 12];
        out.copy_from_slice(&vec);
        Ok(out)
    }
}

mod hex_serde_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}
