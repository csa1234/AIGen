<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# Local Testing Guide

## Table of Contents

- [Prerequisites](#prerequisites)
- [Single Node Testing](#single-node-testing)
- [Multi-Node Local Testing](#multi-node-local-testing)
- [Docker-Based Testing](#docker-based-testing)
- [Testing Workflow Diagram](#testing-workflow-diagram)
 
 ## License Notice
 
 Testing and development use of AIGEN Blockchain is permitted under the **Business Source License 1.1**. Production use requires a commercial license.
 
 ## Prerequisites

### Required software

- Rust (stable toolchain)
- Cargo (bundled with Rust)
- Git
- ONNX Runtime (required for model inference/verification tests; set `ORT_DYLIB_PATH` if using a custom install)

### Install Rust

- Windows:

```powershell
# Install Rust via rustup
winget install Rustlang.Rustup
```

- macOS:

```bash
brew install rustup-init
rustup-init
```

- Linux:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Install Git

- Windows:

```powershell
winget install Git.Git
```

- macOS:

```bash
brew install git
```

- Linux (Debian/Ubuntu):

```bash
sudo apt-get update
sudo apt-get install -y git
```

### Verify installations

```bash
rustc --version
cargo --version
git --version
```

## Single Node Testing

### 1) Build the project

What this does: compiles all workspace crates and produces release binaries.

```bash
cargo build --release
```

### 2) Run unit tests

What this does: runs all unit/integration tests across the workspace.

```bash
cargo test
```

### 3) Initialize a single node

What this does: creates a node identity/keypair and an initial config/data directory.

```bash
cargo run --release --bin node -- init
```

If you want an explicit node ID (recommended for multi-node work):

```bash
cargo run --release --bin node -- init --node-id my-node
```

### 4) Start the node

What this does: starts networking (P2P) and the JSON-RPC server.

```bash
cargo run --release --bin node -- start
```

### 5) Test RPC endpoints (curl)

Default endpoint:

- `http://localhost:9944`

Health check:

```bash
curl -s -X POST http://localhost:9944 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"health","params":[]}'
```

Chain info:

```bash
curl -s -X POST http://localhost:9944 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
```

### 6) Verify node health / chain info

What to look for:

- **Healthy RPC**: `health()` returns `ok: true` (or equivalent)
- **Chain status**: `getChainInfo()` returns height, best hash, peers (or equivalent)

### 7) Stop the node gracefully

- Press `Ctrl+C` in the terminal running the node.
- Wait for shutdown logs confirming that services stopped cleanly.

## Multi-Node Local Testing

This section describes a **manual 3-node setup** on a single machine.

### Ports to plan for

- P2P port: `9000` (each node must use a different port locally)
- RPC port: `9944` (each node must use a different port locally)

Example layout:

- Node A: P2P `9000`, RPC `9944`
- Node B: P2P `9001`, RPC `9945`
- Node C: P2P `9002`, RPC `9946`

### 1) Initialize 3 nodes

```bash
cargo run --release --bin node -- init --node-id node-a
cargo run --release --bin node -- init --node-id node-b
cargo run --release --bin node -- init --node-id node-c
```

### 2) Create config files

Create one config per node (example TOML skeletons). Adjust paths to match your environment.

`node-a.toml`:

```toml
[node]
id = "node-a"
data_dir = "./data/node-a"

[network]
p2p_listen_addr = "0.0.0.0:9000"
bootstrap_peers = []

[rpc]
listen_addr = "127.0.0.1:9944"
```

`node-b.toml`:

```toml
[node]
id = "node-b"
data_dir = "./data/node-b"

[network]
p2p_listen_addr = "0.0.0.0:9001"
bootstrap_peers = ["/ip4/127.0.0.1/tcp/9000"]

[rpc]
listen_addr = "127.0.0.1:9945"
```

`node-c.toml`:

```toml
[node]
id = "node-c"
data_dir = "./data/node-c"

[network]
p2p_listen_addr = "0.0.0.0:9002"
bootstrap_peers = ["/ip4/127.0.0.1/tcp/9000"]

[rpc]
listen_addr = "127.0.0.1:9946"
```

### 3) Start 3 nodes

In three terminals:

```bash
cargo run --release --bin node -- start --config ./node-a.toml
```

```bash
cargo run --release --bin node -- start --config ./node-b.toml
```

```bash
cargo run --release --bin node -- start --config ./node-c.toml
```

### 4) Test peer discovery and connectivity

- Check logs for peer connections.
- Query each node’s `getChainInfo()` and compare:

```bash
curl -s -X POST http://localhost:9944 -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
curl -s -X POST http://localhost:9945 -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
curl -s -X POST http://localhost:9946 -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
```

### 5) Submit a transaction and verify propagation

⚠️ The exact transaction format depends on your client-side signing and the `submitTransaction(transaction)` schema.

General flow:

1. Create/sign transaction
2. `submitTransaction` to Node A
3. Query `getPendingTransactions` from Node B/C

Example submission:

```bash
curl -s -X POST http://localhost:9944 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"submitTransaction","params":[{"sender":"0x...","sender_public_key":"0x...","receiver":"0x...","amount":1,"signature":"0x...","timestamp":1730000000,"nonce":1,"priority":false,"tx_hash":"0x...","fee_base":1,"fee_priority":0,"fee_burn":0,"chain_id":1,"payload":null,"ceo_signature":null}]}'
```

What this means:

- `sender_public_key` is **required** and must be 32-byte hex.
- `tx_hash` must be computed from the transaction contents (the node will reject mismatches).
- `signature` must be an Ed25519 signature over the raw `tx_hash` bytes.
- `sender` must match the address derived from `sender_public_key`.

### 6) Monitor network health

- Use `health()` periodically on all nodes.
- Watch peer counts and mempool size via `getPendingTransactions(limit)`.

## Docker-Based Testing

For a **ready-made 5-node local testnet**, use the Docker tooling:

- See `docker/README.md` for the authoritative guide.

Quick start (from repo root):

```bash
cd docker
./scripts/init-network.sh
./scripts/start-network.sh
```

Suggested testing scenarios:

- ✅ **Transaction flow**: submit to one node and confirm propagation.
- ✅ **Consensus behavior**: observe block production and finality signals.
- ✅ **Controlled shutdown**: stop the network and verify clean exit.

## Testing Workflow Diagram

```mermaid
    participant Dev as Developer
    participant Build as Cargo Build
    participant Test as Cargo Test
    participant Node as AIGEN Node
    participant RPC as RPC API
    
    Dev->>Build: cargo build --release
    Build-->>Dev: Binary ready
    Dev->>Test: cargo test
    Test-->>Dev: All tests pass
    Dev->>Node: aigen-node init
    Node-->>Dev: Config & keypair created
    Dev->>Node: aigen-node start
    Node->>Node: Initialize blockchain
    Node->>RPC: Start RPC server
    Dev->>RPC: curl getChainInfo
    RPC-->>Dev: Chain status
```

## Test Categories

- Unit tests: crate-level modules
- Integration tests: cross-crate interactions (tests/ directory)
- End-to-end tests: complete flows (node init, model loading, inference)
- Network tests: P2P model distribution and events

## Running Tests

```bash
cargo test --workspace
cargo test -p model
cargo test -p network
cargo test -p consensus
cargo test --test ai_node_integration_test
```

## Test Environment Setup

- ONNX Runtime: set ORT_DYLIB_PATH to onnxruntime DLL path
- Large shard tests: set AIGEN_RUN_LARGE_SHARD_TEST=1 to enable full shard flows
- Temporary directories: tests use std::env::temp_dir(), cleaned automatically

## Test Patterns

- Async tests: #[tokio::test]
- Shutdown tests: reset_shutdown_for_tests() and emergency_shutdown()
- Test harnesses: reusable setup for registry/storage/engine
- Temporary files: helper functions generating identity ONNX models

## Troubleshooting Tests

- ONNX Runtime not found: download manually and set environment variable
- Disk space errors: skip large file operations or increase free space
- Timeout errors: increase test timeout or use multi_thread flavor

## CI/CD Integration

- Run cargo test --workspace in CI
- Cache ORT runtime downloads to speed up execution
- Collect test artifacts and logs for debugging
