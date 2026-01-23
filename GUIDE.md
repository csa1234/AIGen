# AIGEN Blockchain Noob Startup Guide

## What You Need Before Starting

### System Requirements
- **RAM:** 4GB minimum (8GB recommended)
- **Storage:** 10GB free disk space
- **OS:** Windows 10/11, macOS, or Linux

### Required Software
1. **Rust** - Install from https://rustup.rs/
2. **Docker Desktop** - Install from https://www.docker.com/products/docker-desktop/
3. **Git** - Install from https://git-scm.com/

---

## Step 1: Install Prerequisites

### Install Rust
```powershell
# Download and run rustup-init.exe from https://rustup.rs/
# Follow the installation prompts
# Restart PowerShell after installation
```

### Install Docker Desktop
1. Download Docker Desktop from the official website
2. Install with default settings
3. Start Docker Desktop (it will run in background)
4. Verify installation:
```powershell
docker --version
docker info
```

---

## Step 2: Get the Code

```powershell
# If you haven't already cloned the repository
git clone <repository-url>
cd AIGEN
```

---

## Step 3: Build the Project

```powershell
# Build the entire blockchain project
cargo build --release

# This will take a few minutes and download dependencies
# You should see "Finished release profile" at the end
```

---

## Step 4: Test Single Node (Quick Start)

### Option A: Using Cargo (Easier)

```powershell
# Initialize a node
cargo run -p node --bin node -- init --node-id my-node

# Start the node (this will keep running)
cargo run -p node --bin node -- start
```

### Option B: Using Built Binary

```powershell
# Initialize a node
.\target\release\node.exe init --node-id my-node

# Start the node (this will keep running)
.\target\release\node.exe start
```

### Test if Node is Working

**Open a NEW PowerShell window** (leave the node running in the first window):

```powershell
# Test the RPC API
$body = @{
    jsonrpc = "2.0"
    id = 1
    method = "getChainInfo"
    params = @()
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body
```

**Expected Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "chain_id": 13044767616725935409,
    "height": 0,
    "latest_block_hash": "0xe7a898896df73c59c4d0dc6e3e7e1d7a...",
    "shutdown_status": false,
    "total_supply": 0
  },
  "id": 1
}
```

---

## Step 4.5: Initialize with AI Model (Optional)

### Register Core AI Model

Before initializing nodes with AI capabilities, register the model metadata:

```powershell
# Example: Register Mistral-7B model
# (In production, this would be done via governance/admin RPC)
# For testing, models are pre-registered in the genesis configuration
```

### Initialize AI Worker Node

```powershell
# Initialize node with core AI model
cargo run -p node --bin node -- init --node-id ai-worker-1 --model mistral-7b --role worker

# Start the AI worker node
cargo run -p node --bin node -- start
```

**What happens during startup:**
1. Node checks if `mistral-7b` exists in model registry
2. Queries network for model shards
3. Downloads missing shards (with 30-second timeout)
4. Verifies shard integrity (SHA-256)
5. Loads model into memory
6. Announces shard availability to network

**Expected output:**
```
loading core model: mistral-7b
downloading 4 missing shards for core model
core model shards downloaded successfully
loading core model into memory...
core model loaded successfully: mistral-7b
core AI model ready for inference
```

### Initialize Non-Worker Node

```powershell
# Regular node without AI inference
cargo run -p node --bin node -- init --node-id full-node-1

# Or specify model but not as worker (lazy loading)
cargo run -p node --bin node -- init --node-id full-node-2 --model mistral-7b
```

**Worker vs Non-Worker:**
- **Worker nodes** (`--role worker`): Require core model on startup, fail if unavailable
- **Non-worker nodes**: Continue without core model, load on-demand via RPC

### Troubleshooting Model Loading

**Timeout errors:**
```powershell
# Increase timeout via environment variable
$env:AIGEN_MODEL_DOWNLOAD_TIMEOUT_SECS = "60"
cargo run -p node --bin node -- start
```

**Missing model metadata:**
```
Error: core model 'mistral-7b' not found in registry
```
Solution: Register model metadata via admin RPC or genesis configuration

**Insufficient peers:**
```
warning: core model redundancy below target (2 < 5)
```
Solution: Start more nodes with the same model to increase redundancy

## Step 5: Multi-Node Testnet (Full Experience)

## Step 5: Multi-Node Testnet (Full Experience)

### Windows PowerShell Method

```powershell
# Go to docker directory
cd docker

# Build Docker image
docker build -t aigen-node:latest ..

# Create data directories
mkdir data/bootstrap
mkdir data/full-node-1
mkdir data/full-node-2
mkdir data/miner
mkdir data/validator

# Generate keypairs for each node
docker run --rm -v "${PWD}/data/bootstrap:/data" aigen-node:latest keygen --output /data/node_keypair.bin
docker run --rm -v "${PWD}/data/full-node-1:/data" aigen-node:latest keygen --output /data/node_keypair.bin
docker run --rm -v "${PWD}/data/full-node-2:/data" aigen-node:latest keygen --output /data/node_keypair.bin
docker run --rm -v "${PWD}/data/miner:/data" aigen-node:latest keygen --output /data/node_keypair.bin
docker run --rm -v "${PWD}/data/validator:/data" aigen-node:latest keygen --output /data/node_keypair.bin

# Get bootstrap peer ID
$bootstrapPeer = docker run --rm -v "${PWD}/data/bootstrap:/data" aigen-node:latest keygen --output /data/_tmp.bin --show-peer-id 2>&1
Write-Host "Bootstrap peer ID: $bootstrapPeer"

# Start the network
docker-compose up -d
```

### Alternative: Use WSL (Linux Subsystem)

If you have WSL installed:
```powershell
wsl
cd /mnt/d/Code/AIGEN/docker
./scripts/init-network.sh
./scripts/start-network.sh
```

### Test Multi-Node Network

```powershell
# Test different nodes (each has different RPC port)
$ports = @(9944, 9945, 9946, 9947, 9948)
foreach ($port in $ports) {
    Write-Host "Testing port $port..."
    try {
        $body = @{jsonrpc="2.0";id=1;method="getChainInfo";params=@()} | ConvertTo-Json
        $response = Invoke-RestMethod -Uri "http://localhost:$port" -Method POST -ContentType "application/json" -Body $body
        Write-Host "✅ Port $port: Chain height $($response.result.height)"
    } catch {
        Write-Host "❌ Port $port: Not responding"
    }
}
```

---

## Step 6: Monitor Your Network

### Check Node Status
```powershell
# See running containers
docker ps

# View logs for all nodes
docker-compose logs -f

# View logs for specific node
docker-compose logs -f bootstrap-node
```

### Monitor Network Health
```powershell
# Real-time monitoring script
while ($true) {
    Clear-Host
    Write-Host "=== AIGEN Network Status ===" -ForegroundColor Green
    Write-Host "Time: $(Get-Date)"
    
    $ports = @(9944, 9945, 9946, 9947, 9948)
    $nodeNames = @("Bootstrap", "Full-1", "Full-2", "Miner", "Validator")
    
    for ($i = 0; $i -lt $ports.Length; $i++) {
        try {
            $body = @{jsonrpc="2.0";id=$i;method="getChainInfo";params=@()} | ConvertTo-Json
            $response = Invoke-RestMethod -Uri "http://localhost:$($ports[$i])" -Method POST -ContentType "application/json" -Body $body -TimeoutSec 2
            Write-Host "$($nodeNames[$i]): Height $($response.result.height) | Shutdown: $($response.result.shutdown_status)" -ForegroundColor Green
        } catch {
            Write-Host "$($nodeNames[$i]): Offline" -ForegroundColor Red
        }
    }
    
    Start-Sleep 5
}
```

---

## Step 7: Common Troubleshooting

### Node Won't Start
```powershell
# Check if port is already in use
netstat -an | findstr 9944

# Kill existing node process
taskkill /f /im node.exe

# Try different port
.\target\release\node.exe start --rpc-port 9945
```

### Docker Issues
```powershell
# Check Docker status
docker info

# Clean up Docker
docker system prune -a

# Rebuild image
docker build -t aigen-node:latest .. --no-cache
```

### Network Connection Issues
```powershell
# Check if nodes can communicate
docker network ls
docker network inspect aigen_default

# Restart network
docker-compose down
docker-compose up -d
```

### Build Errors
```powershell
# Clean build
cargo clean
cargo build --release

# Update dependencies
cargo update
```

---

## Step 8: What You Can Do Now

### Explore the API
```powershell
# Available methods (try these):
$body = @{jsonrpc="2.0";id=1;method="getChainInfo";params=@()} | ConvertTo-Json
$body = @{jsonrpc="2.0";id=2;method="getPeers";params=@()} | ConvertTo-Json
$body = @{jsonrpc="2.0";id=3;method="getBlocks";params=@(@{"from":0;"to":10})} | ConvertTo-Json

# Test each method
Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body
```

### Test AI Inference (If Model Loaded)

```powershell
# Check if core model is loaded
$body = @{
    jsonrpc = "2.0"
    id = 1
    method = "getModelInfo"
    params = @(@{model_id = "mistral-7b"})
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body

# Submit inference request (requires subscription or payment)
$body = @{
    jsonrpc = "2.0"
    id = 2
    method = "chatCompletion"
    params = @{
        messages = @(
            @{role = "user"; content = "Hello, AI!"}
        )
        model_id = "mistral-7b"
        stream = $false
    }
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body
```

### Submit Transactions (Advanced)
```powershell
# Create a test transaction (this is complex, see docs/API.md)
$transaction = @{
    "from" = "0x..."
    "to" = "0x..."
    "value" = "1000"
    "data" = "0x"
} | ConvertTo-Json -Depth 10

$body = @{
    jsonrpc = "2.0"
    id = 4
    method = "submitTransaction"
    params = @($transaction)
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body
```

---

## Step 9: Stop Everything

### Stop Single Node
```powershell
# Press Ctrl+C in the node terminal
# Or close the PowerShell window
```

### Stop Multi-Node Network
```powershell
cd docker
docker-compose down

# Clean up data (optional)
docker-compose down -v
```

---

## Using the Chat API

### With Subscription

```bash
# Basic chat completion (requires active subscription)
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "chatCompletion",
    "params": {
      "messages": [
        {"role": "user", "content": "Hello!"}
      ],
      "model_id": "mistral-7b",
      "stream": false
    },
    "id": 1
  }'
```

### Pay-Per-Use (No Subscription)

For pay-per-use, you need to create and sign a payment transaction:

1. **Create payment transaction** with chat payload:
```bash
# First, create a transaction with the chat payment payload
# This is a simplified example - see docs/API.md for full details
```

2. **Include transaction in chatCompletion request**:
```bash
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
  "jsonrpc": "2.0",
  "method": "chatCompletion",
  "params": {
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Explain blockchain in simple terms."}
    ],
    "model_id": "mistral-7b",
    "stream": false,
    "max_tokens": 500,
    "transaction": {
      "sender": "0x...",
      "receiver": "0x...",
      "amount": 1,
      "payload": "{\"user_address\":\"0x...\",\"model_id\":\"mistral-7b\",\"max_tokens\":500}",
      "signature": "0x...",
      "timestamp": 1234567890,
      "nonce": 1,
      "chain_id": 1
    }
  },
  "id": 1
}'
```

**Pricing:**
- **Subscription users**: Deducted from monthly quota
- **Pay-per-use**: 1 AIGEN per 1000 tokens
- **Free tier**: 10 requests/month with ads

**Available Models:**
- `mistral-7b` - General purpose language model
- `llama2-13b` - Larger language model (requires Pro tier)
- `codegen-16b` - Code generation model (requires Pro tier)

See `docs/examples/javascript/chat-example.js` and `docs/examples/python/chat_example.py` for SDK examples.

---

## What You've Accomplished

✅ **Built a blockchain from source code**  
✅ **Run your own node**  
✅ **Tested RPC API**  
✅ **Created a multi-node network**  
✅ **Monitored blockchain activity**  

## Next Steps

1. **Read the Documentation:** Check the `docs/` folder for detailed API docs
2. **Experiment with Transactions:** Try submitting different transaction types
3. **Explore CEO Controls:** Learn about the Genesis key management
4. **Understand the AI Vision:** Read `spec.md` to understand the Proof-of-Intelligence concept

## Need Help?

- **Issues:** Check `docs/TROUBLESHOOTING.md`
- **API Reference:** See `docs/API.md`
- **Security:** Read `docs/GENESIS_KEY_SECURITY.md`
- **Community:** Open an issue on GitHub with your error logs

**Congratulations! You're now running an AIGEN blockchain node! 🎉**
