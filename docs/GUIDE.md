<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# AIGEN Getting Started Guide

This guide will help you get started with the AIGEN blockchain AI platform, including setting up the interactive playground and using the comprehensive SDKs.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Windows 11 Specific Setup](#windows-11-specific-setup)
- [Quick Start](#quick-start)
- [Interactive Playground](#interactive-playground)
- [Admin Dashboard](#admin-dashboard)
- [Using the JavaScript SDK](#using-the-javascript-sdk)
- [Using the Python SDK](#using-the-python-sdk)
- [API Reference](#api-reference)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)

## Prerequisites

Before you begin, ensure you have the following installed:

- **Rust** (latest stable version)
- **Node.js** (16 or later) for JavaScript examples
- **Python** (3.8 or later) for Python examples
- **Git** for cloning the repository

## Windows 11 Specific Setup

For Windows 11 users with JAN AI AI model deployment, follow these enhanced requirements:

### Enhanced System Requirements for Windows 11
- **RAM:** 16GB minimum (32GB recommended for AI model loading)
- **Storage:** 50GB free disk space (20GB for project + 30GB for AI models)
- **GPU:** NVIDIA GPU with CUDA support (recommended but not required)
- **OS:** Windows 11 with latest updates

### Additional Windows 11 Prerequisites
1. **Visual Studio Build Tools** - Required for some Rust crates
2. **WSL2** - For better Linux compatibility (recommended)
3. **Docker Desktop** with WSL2 integration

### Windows 11 Installation Steps

```powershell
# Install Visual Studio Build Tools (run as Administrator)
# Download from: https://visualstudio.microsoft.com/visual-cpp-build-tools/
# Select "C++ build tools" during installation

# Install WSL2 (recommended)
wsl --install
# This will install WSL2 and Ubuntu by default

# Enable Windows features for Docker
dism.exe /online /enable-feature /featurename:Microsoft-Windows-Subsystem-Linux /all /norestart
dism.exe /online /enable-feature /featurename:VirtualMachinePlatform /all /norestart

# Restart your computer after these changes

# Install development tools using winget
winget install Rustlang.Rust.MSVC
winget install Git.Git
winget install Docker.DockerDesktop
winget install Microsoft.VisualStudioCode

# Configure Docker Desktop for WSL2
# Open Docker Desktop → Settings → Resources → WSL Integration
# Enable "Use WSL 2 based engine"
# Enable integration with your WSL distributions
```

### JAN AI Model Integration for Windows 11

```powershell
# Clone the repository
git clone <repository-url>
cd AIGEN

# Create models directory for your JAN AI model
mkdir -p models

# Place your JAN AI model files in the models directory
# Ensure your model is in a compatible format (ONNX, Safetensors, etc.)

# Build the project with Windows optimizations
cargo build --release

# Initialize node with JAN AI model as worker
cargo run -p node --bin node -- init --node-id jan-worker-1 --model jan-model --role worker

# Start the node
cargo run -p node --bin node -- start
```

### Windows 11 Performance Optimization

1. **Docker Settings:**
   - Allocate 8GB+ RAM to Docker
   - Increase CPU allocation
   - Use WSL2 backend for better performance

2. **Windows Defender Exclusions:**
   - Add AIGEN project folder to exclusions
   - Exclude Docker containers from real-time scanning

3. **GPU Acceleration (if available):**
   - Install NVIDIA CUDA Toolkit
   - Configure Docker Desktop to access GPU
   - Ensure model loading uses GPU

### Test JAN AI Model on Windows 11

```powershell
# Test the RPC API
$body = @{
    jsonrpc = "2.0"
    id = 1
    method = "getChainInfo"
    params = @()
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body

# Test JAN AI model availability
$body = @{
    jsonrpc = "2.0"
    id = 2
    method = "getModelInfo"
    params = @(@{model_id = "jan-model"})
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body

# Test chat completion with JAN AI model
$body = @{
    jsonrpc = "2.0"
    id = 3
    method = "chatCompletion"
    params = @{
        messages = @(
            @{role = "user"; content = "Hello JAN AI model!"}
        )
        model_id = "jan-model"
        stream = $false
    }
} | ConvertTo-Json -Depth 10

Invoke-RestMethod -Uri "http://localhost:9944" -Method POST -ContentType "application/json" -Body $body
```

## Quick Start

### 1. Clone and Build the Project

```bash
# Clone the repository
git clone https://github.com/your-org/aigen.git
cd aigen

# Build the project
cargo build --release

# Run the node
cargo run --release
```

The node will start on:
- **RPC endpoint**: `http://localhost:9944`
- **WebSocket endpoint**: `ws://localhost:9944`

### 2. Test the Connection

```bash
# Test RPC connection
curl -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}'
```

## Interactive Playground

The AIGEN AI Playground provides a ChatGPT-style web interface for testing and experimenting with AI models.

### Access the Playground

Open `docs/playground/index.html` in your browser:

```bash
# Option 1: Open directly (file://)
start docs/playground/index.html  # Windows
open docs/playground/index.html   # macOS
xdg-open docs/playground/index.html  # Linux

# Option 2: Serve with Python (recommended for WebSocket support)
cd docs/playground
python -m http.server 8080
# Open http://localhost:8080 in browser
```

### Playground Features

- **Real-time Chat**: ChatGPT-style interface with streaming responses
- **Model Selection**: Switch between mistral-7b, llama2-13b, codegen-16b
- **Tier Management**: View quota, upgrade subscription
- **Settings**: Configure RPC/WebSocket endpoints, adjust temperature/max_tokens
- **Export**: Save chat history to JSON or Markdown

### Quick Start with Playground

1. **Ensure node is running** (see Step 1)
2. **Open playground** in browser
3. **Select model and tier** from the sidebar
4. **Start chatting!** Type your message and press Enter

### Playground Configuration

- **RPC URL**: Configure in Settings (default: `http://localhost:9944`)
- **WebSocket URL**: Configure in Settings (default: `ws://localhost:9944`)
- **Max Tokens**: Adjust response length (default: 2048)
- **Temperature**: Control randomness (default: 0.7)
- **Theme**: Toggle between dark and light modes

## Admin Dashboard

The AIGEN Admin Dashboard provides a comprehensive interface for managing the blockchain network, AI models, health monitoring, and governance operations.

### Accessing the Admin Dashboard

```bash
# Serve with Python
cd docs/admin
python -m http.server 8081
# Open http://localhost:8081 in browser

# Or use Docker (recommended)
docker-compose up -d
# Access admin: http://localhost:8080/admin
```

### CEO Wallet Setup

1. Install MetaMask browser extension
2. Import CEO private key (from genesis keypair generation)
3. Connect wallet to dashboard
4. Verify CEO address in dashboard settings

### Dashboard Features

#### Blockchain Explorer
- View latest blocks with pagination
- Search blocks and transactions by hash
- Real-time block updates

#### AI Models
- Initialize new AI models
- Approve/reject model upgrades
- View model registry and status
- Load models on nodes

#### Health & Metrics
- Real-time node health monitoring
- AI service health tracking
- Blockchain health indicators
- Interactive charts for metrics visualization

#### Governance
- View governance proposals
- Submit votes on proposals
- Approve/veto SIPs
- Emergency shutdown control

### Common Admin Tasks

#### Initialize a New AI Model

1. Navigate to **AI Models** tab
2. Click **Initialize New Model**
3. Fill in model details and sign with CEO wallet
4. Model appears in registry after approval

#### Monitor Node Health

1. Navigate to **Health & Metrics** tab
2. View health cards for node, AI, and blockchain status
3. Monitor charts for trends
4. Adjust refresh interval as needed

#### Submit a Governance Vote

1. Navigate to **Governance** tab
2. Enter proposal ID, vote type, and comment
3. Submit and sign with CEO wallet

### Security Notes

- **CEO Signature Verification**: All admin operations require CEO signature
- **Wallet Protection**: Never share CEO private key
- **Secure Connection**: Use HTTPS when accessing dashboard remotely
- **Verification**: Always verify CEO address before signing requests

### Full Documentation

For detailed documentation, see [docs/admin/README.md](admin/README.md)

## Using the JavaScript SDK

### Installation

```bash
cd docs/examples/javascript
npm install
```

### Basic Usage

```javascript
import { AIGENClient } from './aigen-sdk.js';

const client = new AIGENClient('http://localhost:9944');

async function main() {
    // Connect to AIGEN network
    await client.connect();
    
    // Simple chat completion
    const response = await client.chatCompletion([
        { role: 'user', content: 'Hello! How can you help me?' }
    ]);
    
    console.log(response.choices[0].message.content);
    
    // Disconnect when done
    client.disconnect();
}

main().catch(console.error);
```

### Streaming Chat

```javascript
await client.streamChatCompletion(
    [{ role: 'user', content: 'Tell me a story' }],
    { modelId: 'mistral-7b' },
    (chunk) => process.stdout.write(chunk.delta.content),
    () => console.log('\nDone!'),
    (error) => console.error('Error:', error)
);
```

### Running Examples

```bash
# Basic chat example
npm run chat

# Streaming example
npm run stream

# Full SDK demonstration
npm run sdk
```

### Error Handling

```javascript
import { QuotaExceededError, InsufficientTierError } from './aigen-sdk.js';

try {
    await client.chatCompletion(messages);
} catch (error) {
    if (error instanceof QuotaExceededError) {
        console.log('Quota exceeded! Upgrade your tier or wait for reset.');
    } else if (error instanceof InsufficientTierError) {
        console.log('Tier too low! Upgrade to access this model.');
    }
}
```

## Using the Python SDK

### Installation

```bash
cd docs/examples/python

# Create virtual environment (recommended)
python -m venv .venv
source .venv/bin/activate  # Linux/Mac
# or
.venv\Scripts\activate     # Windows

# Install dependencies
pip install -r requirements.txt
```

### Basic Usage

```python
from aigen_sdk import AIGENClient

client = AIGENClient('http://localhost:9944')
client.connect()

# Simple chat completion
response = client.chat_completion([
    {'role': 'user', 'content': 'Hello! How can you help me?'}
])

print(response.choices[0]['message']['content'])

# Disconnect when done
client.disconnect()
```

### Streaming Chat

```python
import asyncio

async def stream_chat():
    client = AIGENClient('http://localhost:9944', 'ws://localhost:9944')
    await client.connect()
    
    def on_chunk(delta):
        print(delta['content'], end='', flush=True)
    
    def on_complete(result):
        print('\nDone!')
    
    await client.stream_chat_completion(
        [{'role': 'user', 'content': 'Tell me a story'}],
        on_chunk=on_chunk,
        on_complete=on_complete,
        model_id='mistral-7b'
    )

asyncio.run(stream_chat())
```

### Async Streaming

```python
async for delta in client.async_stream_chat_completion(
    messages,
    model_id='mistral-7b'
):
    print(delta['content'], end='', flush=True)
```

### Running Examples

```bash
# Basic chat example
python chat_example.py

# Streaming example
python stream_example.py

# Full SDK demonstration
python sdk_example.py
```

## API Reference

### Core Endpoints

- **`chatCompletion`**: Send chat messages and get AI responses
- **`subscribeChatCompletion`**: WebSocket streaming for real-time responses
- **`checkQuota`**: Check current usage and limits
- **`getTierInfo`**: Get subscription tier information
- **`listModels`**: List available AI models

### Error Codes

| Code | Description | Resolution |
|------|-------------|------------|
| 2003 | Insufficient tier | Upgrade subscription |
| 2004 | Quota exceeded | Wait for reset or upgrade |
| 2007 | Payment required | Add payment transaction |
| 2008 | Context length exceeded | Reduce input size |

### Models Available

- **mistral-7b**: General purpose language model
- **llama2-13b**: Larger language model (Pro tier required)
- **codegen-16b**: Code generation and programming tasks

## Examples

### Check Quota and Tier

```javascript
const quota = await client.checkQuota();
console.log(`Used: ${quota.used}/${quota.limit}`);

const tierInfo = await client.getTierInfo();
console.log(`Current tier: ${tierInfo.current_tier}`);
```

### Model Information

```javascript
const models = await client.listModels();
models.forEach(model => {
    console.log(`${model.name}: ${model.description}`);
});
```

### Wallet Authentication

```javascript
// Generate key pair (client-side)
const keyPair = nacl.sign.keyPair();
const address = client.deriveAddress(keyPair.publicKey);

// Create and sign payment transaction
const tx = client.createPaymentTransaction(100, {
    service: 'ai-inference',
    model: 'mistral-7b'
});

const signature = client.signTransaction(tx, keyPair.secretKey);
```

### Batch Processing

```javascript
const job = await client.submitBatchRequest('normal', 'mistral-7b', [
    { input: 'Translate: Hello world' },
    { input: 'Translate: Good morning' }
]);

// Check job status
const status = await client.getBatchStatus(job.id);
```

## Troubleshooting

### Connection Issues

**Problem**: "Failed to connect to RPC" or "Connection refused"

**Solutions**:
1. Ensure the AIGEN node is running
2. Check the RPC URL is correct (default: `http://localhost:9944`)
3. Verify firewall settings allow connections to port 9944
4. Check if the port is already in use

### WebSocket Issues

**Problem**: Streaming not working or connection drops

**Solutions**:
1. Use HTTP server instead of file:// protocol for playground
2. Check WebSocket URL is correct (default: `ws://localhost:9944`)
3. Verify browser supports WebSocket connections
4. Check for proxy/firewall blocking WebSocket

### Quota Issues

**Problem**: "Quota exceeded" error

**Solutions**:
1. Check current usage with `checkQuota()`
2. Wait for monthly quota reset (1st of each month)
3. Upgrade to higher tier subscription
4. Use pay-per-use with valid payment transaction

### Model Access Issues

**Problem**: "Insufficient tier" error

**Solutions**:
1. Check model requirements with `getModelInfo(modelId)`
2. Upgrade subscription tier if needed
3. Use models available for your current tier
4. Consider pay-per-use for occasional premium model usage

### SDK Installation Issues

**JavaScript**:
```bash
# Clear npm cache
npm cache clean --force

# Reinstall dependencies
rm -rf node_modules package-lock.json
npm install
```

**Python**:
```bash
# Recreate virtual environment
rm -rf .venv
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Next Steps

1. **Explore the Playground**: Test different models and settings
2. **Try the Examples**: Run the provided SDK examples
3. **Build Your App**: Integrate AIGEN into your project
4. **Read the API Docs**: Learn about advanced features
5. **Join the Community**: Get help and share your projects

## Support

- **Documentation**: Check the [API documentation](API.md)
- **Examples**: Browse the [examples directory](examples/)
- **Issues**: Report bugs on the GitHub repository
- **Community**: Join discussions and get help

---

**Happy Building!** 🚀