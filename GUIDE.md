# AIGEN Blockchain Complete Guide

## Table of Contents
1. [Prerequisites & System Requirements](#prerequisites--system-requirements)
2. [Quick Start (5 Minutes)](#quick-start-5-minutes)
3. [Test Model Setup (Development)](#test-model-setup-development)
4. [Production Model Setup (Jan AI / GGUF)](#production-model-setup-jan-ai--gguf)
5. [Admin Dashboard Setup](#admin-dashboard-setup)
6. [Development Environment](#development-environment)
7. [Multi-Node Deployment](#multi-node-deployment)
8. [Production Deployment](#production-deployment)
9. [CEO Operations](#ceo-operations)
10. [API & SDK Usage](#api--sdk-usage)
11. [Troubleshooting](#troubleshooting)
12. [Security & Best Practices](#security--best-practices)

---

## Prerequisites & System Requirements

### System Requirements
- **RAM:** 4GB minimum (8GB recommended)
- **Storage:** 10GB free disk space
- **OS:** Windows 10/11, macOS, or Linux

### Windows 11 Enhanced Requirements (for AI Models)
- **RAM:** 16GB minimum (32GB recommended)
- **Storage:** 50GB free disk space
- **GPU:** NVIDIA GPU with CUDA support (recommended)
- **Software:** Visual Studio Build Tools, WSL2

### Required Software
1. **Rust** - Install from [rustup.rs](https://rustup.rs/)
2. **Docker Desktop** - Install from [docker.com](https://www.docker.com/products/docker-desktop/)
3. **Git** - Install from [git-scm.com](https://git-scm.com/)
4. **Node.js** (16+) & **Python** (3.8+) - For dashboard and SDKs

---

## Quick Start (5 Minutes)

### 1. Clone and Build
```bash
git clone https://github.com/your-org/aigen.git
cd AIGEN
cargo build --release
```

### 2. Initialize and Start Node
```bash
# Initialize a single node
cargo run -p node --bin node -- init --node-id my-node

# Start the node
cargo run -p node --bin node -- start
```

### 3. Verify Connection
```bash
# Check system health
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}'
```

For detailed model setup instructions, see [SETUP.md](SETUP.md).

---

## Test Model Setup (Development)

For development and testing without large model downloads, use the built-in test model script to generate a minimal ONNX identity model.

### Quick Setup

```bash
# Generate test model
cargo run --bin setup-test-model -- --model-id mistral-7b --data-dir ./data
```

### CLI Arguments

| Argument | Description | Default |
|----------|-------------|---------|
| `--model-id` | Unique identifier for the model | `mistral-7b` |
| `--model-name` | Human-readable model name | `Mistral-7B-Test-Identity` |
| `--data-dir` | Base directory for model storage | `./data` |
| `--version` | Model version string | `1.0.0` |

### What It Creates

```
data/
└── models/
    └── mistral-7b/
        ├── model.onnx          # ONNX identity model (echoes input to output)
        ├── manifest.json       # Metadata for auto-registration
        └── shards/
            ├── shard_0.bin   # Model shards
            └── shard_1.bin
```

### Auto-Registration Mechanism

The node automatically discovers and registers models at startup:

1. Scans `data_dir/models/*/` for `manifest.json` files
2. Parses manifest to extract model metadata and shard locations
3. Registers model with `ModelRegistry` (in-memory structure)
4. Registers each shard with the storage backend
5. Sets the core model for inference engine (configured via `core_model_id`)

This is implemented in `node/src/main.rs` lines 157-351 in the `auto_register_local_models` function.

### Verifying Model Registration

```bash
# List registered models
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"listModels","params":[],"id":1}'
```

Expected response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "models": [{
      "id": "mistral-7b",
      "name": "Mistral-7B-Test-Identity",
      "version": "1.0.0",
      "state": "ready"
    }]
  },
  "id": 1
}
```

### Test Inference

```bash
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "method":"runInference",
    "params":{
      "model_id":"mistral-7b",
      "inputs":[{"name":"input","shape":[1,3],"data":[0.5,0.6,0.7]}]
    },
    "id":1
  }'
```

### When to Use Test Models

- Testing inference engine functionality
- Validating model registry and storage
- Development without downloading large models
- CI/CD pipeline testing
- Local debugging and experimentation

Test models are **NOT** for production use - they simply echo inputs.

---

## Production Model Setup (Jan AI / GGUF)

For production use with real AI capabilities, AIGEN supports GGUF models via integration with Jan AI.

### Prerequisites

AIGEN includes a pre-configured Mistral model for AI inference.

### Model Location
- Model file: `model/Mistral/model.gguf`
- Configuration: `model/Mistral/model.yml`

### Setup Instructions

1. **Download Jan AI Desktop** (optional for model management):
   - Visit `https://jan.ai`
   - Download and install Jan AI for your platform
   - Jan AI provides a user-friendly interface for managing GGUF models

2. **Verify Model Files**:
   ```bash
   # Check model exists
   ls -lh model/Mistral/model.gguf
   
   # View model configuration
   cat model/Mistral/model.yml
   ```

3. **Configure Model in AIGEN**:
   ```bash
   # The model is automatically registered during genesis
   # To verify model is available:
   curl -X POST http://localhost:9944 \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","method":"listModels","params":[],"id":1}'
   ```

4. **Initialize Node with Mistral Model**:
   ```bash
   # Initialize as AI worker node
   cargo run -p node --bin node -- init \
     --node-id mistral-worker-1 \
     --model mistral-7b \
     --role worker
   
   # Start the node
   cargo run -p node --bin node -- start
   ```

5. **Test Model Inference**:
   ```bash
   curl -X POST http://localhost:9944 \
     -H "Content-Type: application/json" \
     -d '{
       "jsonrpc":"2.0",
       "method":"chatCompletion",
       "params":{
         "messages":[{"role":"user","content":"Hello!"}],
         "model_id":"mistral-7b",
         "stream":false
       },
       "id":1
     }'
   ```

### Model Configuration Details

The `model/Mistral/model.yml` contains:
- `model_path`: Path to GGUF model file
- `name`: Model identifier
- `size_bytes`: Model size for resource planning
- `embedding`: Whether model supports embeddings

### Using Jan AI for Model Management

Jan AI can be used to:
- Browse and download additional GGUF models
- Test models locally before deploying to AIGEN
- Convert models to GGUF format
- Manage model versions

To use a Jan AI model with AIGEN:
1. Export model from Jan AI to `model/` directory
2. Create corresponding `model.yml` configuration
3. Register model via admin dashboard or genesis configuration
4. Initialize nodes with the new model

### ONNX vs GGUF: Key Differences

| Aspect | Test (ONNX) | Production (GGUF) |
|--------|-------------|-------------------|
| Purpose | Development/Testing | Real AI inference |
| Model Format | ONNX | GGUF |
| Size | ~1 KB | Multi-GB |
| Behavior | Echoes input | Actual AI reasoning |
| Auto-registration | Yes (via manifest.json) | Manual/config-based |
| Use Case | CI/CD, debugging | Production workloads |

For detailed setup instructions, see [SETUP.md](SETUP.md).

---

## Admin Dashboard Setup

The admin dashboard is located at `dashboard/` (root level).

### Quick Start

```bash
# Navigate to dashboard directory
cd dashboard

# Serve with Python (recommended)
py -m http.server 8081

# Or with Node.js
npx http-server -p 8081

# Open http://localhost:8081 in browser
```

### Configuration

1. Copy sample configuration:
   ```bash
   cp dashboard/config.sample.json dashboard/config.json
   ```

2. Edit `dashboard/config.json`:
   ```json
   {
     "rpc": {
       "http": "http://localhost:9944",
       "ws": "ws://localhost:9944"
     },
     "web3": {
       "enableEvm": false
     }
   }
   ```

3. Configure CEO key in dashboard Settings

### Important: AIGEN is NOT EVM-Compatible

AIGEN uses Ed25519 cryptography (like Solana), NOT EVM/SECP256k1 (like Ethereum).

- ❌ MetaMask, Coinbase Wallet, Trust Wallet **CANNOT** be used
- ❌ Not compatible with Ethereum, Polygon, BSC, Sepolia
- ✅ Uses Ed25519 key pairs (same as Solana, Cardano, Polkadot)
- ✅ CEO operations require Ed25519 private key in dashboard settings

For detailed dashboard documentation, see `dashboard/README.md`

---

## Development Environment

### Windows Specifics
- Install Visual Studio Build Tools
- Use WSL2 for best compatibility
- Increase Docker memory allocation (8GB+)

### Running Tests
```bash
# Unit tests
cargo test -p model

# Integration tests
cargo test --test ai_node_integration_test

# All tests
cargo test --workspace
```

---

## Multi-Node Deployment

### Using Docker Compose
```bash
cd docker
docker-compose up -d
```

### Monitor Network
```bash
# Check logs
docker-compose logs -f

# Check node status
docker ps
```

### Network Architecture
- **Bootstrap Node**: Initial discovery point (port 9000)
- **Full Nodes**: Store blockchain history
- **Validators**: Participate in consensus
- **Miners**: Generate new blocks

---

## Production Deployment

> **⚠️ IMPORTANT**: AIGEN Blockchain is licensed under the **Business Source License 1.1**. Production use requires a commercial license.

### Cloud Providers
- **AWS**: EC2 (t3.large/c6i.large), ALB for RPC
- **DigitalOcean**: Droplets (4GB+ RAM)
- **GCP**: Compute Engine or GKE

### Best Practices
- **Security**: Restrict RPC access, allow P2P
- **TLS**: Use Nginx/Traefik for SSL termination
- **Monitoring**: Track RPC latency, peer count, block height
- **Backups**: Regular snapshots of data volumes

See [docs/PRODUCTION_GUIDE.md](docs/PRODUCTION_GUIDE.md) for detailed production setup.

---

## CEO Operations

### Key Generation
Run on an air-gapped machine:
```bash
cargo run -p scripts --bin generate-ceo-keypair
```

### Emergency Shutdown
1. Sign shutdown command offline:
   ```bash
   cargo run -p scripts --bin sign-shutdown-command -- --message "shutdown:..." --key "./ceo-key.enc"
   ```
2. Broadcast via `submitShutdown` RPC

### Governance (SIPs)
- **Approve**: Sign `sip_approve` message and submit `approveSIP`
- **Veto**: Sign `sip_veto` message and submit `vetoSIP`

---

## API & SDK Usage

### JavaScript SDK
```bash
cd docs/examples/javascript
npm install
npm run chat
```

### Python SDK
```bash
cd docs/examples/python
pip install -r requirements.txt
python chat_example.py
```

### Core API Methods
- `chatCompletion`: AI inference
- `subscribeTier`: Manage subscriptions
- `submitBatchRequest`: Batch processing
- `checkQuota`: Usage monitoring

See [docs/API.md](docs/API.md) for full reference.

---

## Troubleshooting

### Common Issues

1. **Node Won't Start**
   - Check if port 9944/9000 is in use
   - Verify config validity
   - Check permissions on data directory

2. **Model Loading Failed**
   - Increase download timeout: `$env:AIGEN_MODEL_DOWNLOAD_TIMEOUT_SECS = "60"`
   - Verify model path and files
   - Ensure peers are connected

3. **RPC Connection Refused**
   - Ensure node is running
   - Check firewall rules
   - Verify CORS settings

4. **"Insufficient Tier" Error**
   - Upgrade subscription
   - Check quota usage

### Getting Help
- Check logs: `docker logs <container_id>`
- Open issue on GitHub

---

## Security & Best Practices

1. **Key Management**: Use hardware wallets or air-gapped machines for CEO keys.
2. **Network Security**: Firewall P2P ports, restrict RPC access.
3. **Updates**: Regularly update nodes to latest release.
4. **Monitoring**: Set up alerts for downtime and abnormal activity.
