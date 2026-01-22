use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use libp2p::identity::Keypair;

pub fn generate_keypair() -> Keypair {
    Keypair::generate_ed25519()
}

pub fn keypair_exists(path: &Path) -> bool {
    path.exists()
}

pub fn default_keypair_path(data_dir: &Path) -> PathBuf {
    data_dir.join("node_keypair.bin")
}

pub fn save_keypair(keypair: &Keypair, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create keypair parent directory: {}", parent.display()))?;
    }

    let bytes = keypair
        .to_protobuf_encoding()
        .context("failed to encode keypair")?;

    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write keypair file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("failed to set permissions on keypair file: {}", path.display()))?;
    }

    #[cfg(windows)]
    {
        // Best-effort: Windows ACL hardening is not implemented here without extra dependencies.
        // The file will inherit the directory ACLs.
    }

    Ok(())
}

pub fn load_keypair(path: &Path) -> Result<Keypair> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read keypair file: {}", path.display()))?;

    let keypair = Keypair::from_protobuf_encoding(&bytes)
        .map_err(|e| anyhow!("failed to decode keypair protobuf: {e}"))?;

    validate_keypair(&keypair)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .with_context(|| format!("failed to stat keypair file: {}", path.display()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            eprintln!("keypair file permissions are too permissive: path={}, mode={:o}", path.display(), mode);
        }
    }

    Ok(keypair)
}

fn validate_keypair(keypair: &Keypair) -> Result<()> {
    let msg = b"aigen-keypair-validation".as_ref();
    let sig = keypair
        .sign(msg)
        .map_err(|e| anyhow!("keypair failed to sign test message: {e}"))?;
    let pk = keypair.public();
    if !pk.verify(msg, &sig) {
        return Err(anyhow!("keypair signature verification failed"));
    }
    Ok(())
}
