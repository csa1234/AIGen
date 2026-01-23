# AIGEN Getting Started Guide

This guide will help you get started with the AIGEN blockchain AI platform, including setting up the interactive playground and using the comprehensive SDKs.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Interactive Playground](#interactive-playground)
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