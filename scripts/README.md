# AIGEN Scripts

## Overview

This crate provides **offline** CEO key management tooling for the AIGEN Genesis control layer.

Binaries:

- `generate-ceo-keypair` — generate a new Ed25519 CEO keypair and encrypt the private key
- `sign-shutdown-command` — sign an emergency `ShutdownCommand` JSON
- `sign-sip-approval` — sign a SIP approval or veto payload

Canonical invocation (from the workspace root):

- `cargo run -p aigen-scripts --bin generate-ceo-keypair -- <args>`
- `cargo run -p aigen-scripts --bin sign-shutdown-command -- <args>`
- `cargo run -p aigen-scripts --bin sign-sip-approval -- <args>`

## Security Model

- Run these tools on an **air-gapped** machine.
- Treat the encrypted private key file as highly sensitive.
- Do not move plaintext private key material onto internet-connected systems.

## Installation

Build the scripts package:

- `cargo build --release --package aigen-scripts`

## Workflows

### Initial key generation

1. On an air-gapped machine:
   - `cargo run -p aigen-scripts --bin generate-ceo-keypair -- --output-dir . --key-name ceo_key`
2. Store:
   - `ceo_key.encrypted.json` in secure offline storage (multiple backups)
   - `ceo_key.pub.hex` with operational documentation
3. Update `genesis/src/config.rs`:
   - `pub const CEO_PUBLIC_KEY_HEX: &str = "<hex>";`
4. Recompile and redeploy nodes.

### Emergency shutdown

1. Choose a monotonic `nonce` (unique per network).
2. Sign offline:

   - `cargo run -p aigen-scripts --bin sign-shutdown-command -- --key-file ./ceo_key.encrypted.json --reason "Emergency maintenance" --nonce 1 --network-magic 2711576721 --output shutdown_cmd.json`

3. Deliver the JSON to operators via a secure channel.

### SIP approval / veto

1. Review SIP content offline.
2. Compute `proposal_hash` as a 32-byte SHA-256 digest (hex encoded).
3. Sign offline:

   - `cargo run -p aigen-scripts --bin sign-sip-approval -- --key-file ./ceo_key.encrypted.json --proposal-id "SIP-001" --proposal-hash "<64 hex chars>" --action approve --output sip_001_approval.json`

## Encrypted Key Format

The encrypted key file is JSON:

- `version`: format version (currently `1`)
- `salt`: 32-byte random salt (hex)
- `nonce`: 12-byte random AEAD nonce (hex)
- `ciphertext`: AEAD ciphertext (hex)

Encryption:

- KDF: Argon2id (m=64MiB, t=3, p=4)
- AEAD: ChaCha20Poly1305

## Troubleshooting

- Wrong password: decryption fails with `decryption failed`.
- Invalid JSON / fields: parsing fails during `load_encrypted_key`.

## Testing

- Unit/integration tests are under `scripts/tests`.
- The `--no-encrypt` flag is available for isolated testing only.
