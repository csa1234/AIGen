# blockchain-core

## Overview

`blockchain-core` contains the foundational Layer-1 primitives for the AIGEN blockchain. It integrates tightly with the immutable **Genesis CEO control layer** provided by the `genesis` crate.

This crate is intended to be production-grade primitives (types, blocks, transactions, state transitions, and crypto utilities) that other crates (`consensus`, `network`) build upon.

## Architecture

- `types`
  - Centralized primitive newtypes (`Amount`, `Nonce`, `Timestamp`, `BlockHash`, `TxHash`, `ChainId`, etc.)
  - Shared error type: `BlockchainError`

- `transaction`
  - `Transaction` with fee model and replay protection via `chain_id`
  - `TransactionPool` (mempool) with priority ordering (CEO tx first, then by priority fee)

- `block`
  - `BlockHeader` and `Block`
  - Header validation + transaction validation
  - Serialization round-trips with `bincode`

- `state`
  - `AccountState` and `ChainState`
  - Balance + nonce tracking
  - Apply/validate transactions with fee distribution
  - Snapshots for rollback
  - Deterministic `StateRoot`

- `crypto`
  - Ed25519 keypair/signature helpers
  - Transaction hashing and merkle root
  - Merkle proof generation/verification
  - `keccak256` and `blake3_hash` helpers
  - Address derivation and formatting validation

## Genesis CEO Controls

The `genesis` crate enforces immutable CEO authority:

- Kill switch (shutdown) blocks all processing.
- CEO signature is required for privileged actions.
- CEO wallet address is constant: `0x0000000000000000000000000000000000000001`.

In `blockchain-core`:

- Blocks include `shutdown_flag` and optional `ceo_signature`.
- Transactions can be marked `priority` and may carry a `ceo_signature`.

## Tokenomics

Fees follow the split described in `spec.md`:

- 50% to the validator/worker
- 40% burned
- 10% to the dev fund (CEO wallet)

`ChainState::apply_transaction()` debits the sender by `amount + fees` and credits:

- receiver: `amount`
- validator reward address: `validator_share`
- CEO wallet: `dev_share`

Burned value is accounted for as a deduction but is not credited to any account.

## Usage Examples

### Create a chain

```rust
use blockchain_core::Blockchain;

let chain = Blockchain::new();
```

### Create a transaction

```rust
use blockchain_core::Transaction;

let tx = Transaction::new(
    "0x0000000000000000000000000000000000000002".to_string(),
    "0x0000000000000000000000000000000000000003".to_string(),
    10,
    1,
    0,
    false,
).unwrap();
```

### Mempool

```rust
use blockchain_core::{Blockchain, Transaction};

let mut chain = Blockchain::new();
let tx = Transaction::new(
    "0x0000000000000000000000000000000000000002".to_string(),
    "0x0000000000000000000000000000000000000003".to_string(),
    10,
    1,
    0,
    true,
).unwrap();

chain.add_transaction_to_pool(tx).unwrap();
let pending = chain.get_pending_transactions(100);
```

## Testing

Run tests from the workspace root:

```bash
cargo test
```

Tests are located under `blockchain-core/tests/`.

## Future Work

- PoI consensus integration will enhance reward logic and producer selection.
- State root and proof formats can be hardened for light clients.
- Additional validation rules and state persistence interfaces can be added.
