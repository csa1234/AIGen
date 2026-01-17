# Genesis Key Security Model

This document describes the security model for the AIGEN Genesis CEO Key as implemented in the `genesis` crate.

## 1. Key Generation

- The CEO Ed25519 keypair **must** be generated offline on an air-gapped machine.
- Use a hardware security module (HSM) or dedicated key-generation environment.
- The private key must never touch an internet-connected system in plaintext form.

## 2. Hardcoding Process

- The CEO **public key** is embedded as a compile-time constant in `genesis/src/config.rs` as `CEO_PUBLIC_KEY_HEX`.
- The CEO **wallet address** is embedded as `CEO_WALLET = "0x0000000000000000000000000000000000000001"`.
- `GenesisConfig::default()` parses and validates the public key at startup.
- Any change to these constants requires recompiling the node software and creates a **new genesis configuration**.

## 3. Immutability Guarantees

- CEO controls are enforced by hardcoded constants and Ed25519 signature verification.
- The global shutdown flag in `genesis::shutdown` is a one-way `AtomicBool` that, once set, cannot be unset in production builds.
- SIP approval/veto is mediated exclusively through `CeoSignature` verification and the `SipRegistry`.
- All critical operations (transactions, consensus, network) are required to check `genesis::check_shutdown()` or `genesis::is_shutdown()`.

## 4. Emergency Shutdown Protocol

1. CEO constructs a `ShutdownCommand` offline, including:
   - `timestamp`
   - `reason`
   - `nonce` (monotonic, per-network)
   - `network_magic` (the unique identifier for the target network)
2. CEO signs the command with the Genesis private key.
3. The signed command is broadcast to the network using the `network::broadcast_shutdown` API.
4. Each node:
   - Verifies the CEO signature with `genesis::verify_ceo_signature`.
   - Confirms that the command’s `network_magic` matches the local `GenesisConfig`.
   - Checks the nonce to prevent replays.
   - Calls `genesis::emergency_shutdown`, which sets the global shutdown flag.
5. Once triggered, shutdown is permanent for that network instance.

## 5. SIP Approval Process

- AI and human contributors submit System Improvement Proposals (SIPs) recorded as `SipProposal` entries.
- Proposals are stored in the `SipRegistry` with `SipStatus::Pending`.
- Deployment to mainnet is only allowed when:
  - CEO approves with a valid `CeoSignature` via `approve_sip`.
  - `can_deploy_sip` returns `true` and shutdown is not active.
- Veto is performed with `veto_sip`, permanently blocking deployment.

## 6. Threat Model

### Key Theft

- **Risk**: Compromise of the CEO private key.
- **Mitigation**:
  - Store private key in HSM or hardware wallet.
  - Use multi-factor operational controls for signing.
  - Restrict physical and logical access to signing environment.

### Replay Attacks

- **Risk**: Reuse of old shutdown or SIP commands on the same or another network.
- **Mitigation**:
  - Each `ShutdownCommand` includes a `nonce` and `network_magic`.
  - `ShutdownRegistry` tracks used nonces and rejects duplicates.

### Timing Attacks

- **Risk**: Inferring CEO wallet or key material from comparison timing.
- **Mitigation**:
  - CEO wallet comparisons use constant-time string comparison.

## 7. Key Rotation

- Ed25519 key rotation is **not** transparent:
  - Changing `CEO_PUBLIC_KEY_HEX` and `CEO_WALLET` changes the effective Genesis authority.
  - A key rotation therefore requires a new genesis block and full network restart.
- Operators must coordinate migrations as a hard fork event.

## 7.5 Offline Tooling

- The workspace provides an offline tooling package at `scripts/` (`aigen-scripts`).
- These tools are designed to run on an **air-gapped** machine and support:
  - Generating a CEO Ed25519 keypair and encrypting the private key for offline storage.
  - Signing `ShutdownCommand` JSON payloads.
  - Signing SIP approval/veto payloads.

### Encrypted key storage format

- The encrypted CEO private key is stored as a portable JSON document:
  - `version`: format version
  - `salt`: 32-byte hex salt
  - `nonce`: 12-byte hex AEAD nonce
  - `ciphertext`: hex-encoded ciphertext

### Encryption parameters

- KDF: Argon2id
  - memory: 64 MiB
  - iterations: 3
  - parallelism: 4
- AEAD: ChaCha20Poly1305 (authenticated encryption)

### Password policy recommendations

- Use a high-entropy passphrase (recommended: 5+ random words or 20+ random characters).
- Do not reuse passwords.
- Store the passphrase separately from the encrypted key file.

## 8. Backup and Recovery

- Maintain **encrypted** backups of the CEO private key in geographically separate secure locations.
- Use Shamir Secret Sharing or equivalent schemes if multiple parties must jointly reconstruct the key.
- Periodically test recovery procedures in an offline environment.

## 9. Critical Prime Directive

As per `spec.md`, the Genesis CEO controls are **IMMUTABLE | HIGHEST PRIORITY**:

- The network runs autonomously, but the Genesis Key holder can always:
  - Halt the network via emergency shutdown.
  - Veto or approve SIPs.
  - Enforce administrative overrides through signed commands.

Any fork that removes or weakens these controls is **not** considered canonical AIGEN.
