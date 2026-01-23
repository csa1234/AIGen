# RPC API Usage Guide

## Table of Contents

- [Introduction](#introduction)
- [Public API Methods](#public-api-methods)
- [Real-Time AI Inference](#real-time-ai-inference)
- [CEO API Methods](#ceo-api-methods)
- [WebSocket Subscriptions](#websocket-subscriptions)
- [Complete Working Examples](#complete-working-examples)
- [OpenAI-Compatible Endpoints](#openai-compatible-endpoints)
- [Streaming Chat Completion](#streaming-chat-completion)
- [SDK Usage](#sdk-usage)
- [Authentication and Payments](#authentication-and-payments)
- [Rate Limiting and Quotas](#rate-limiting-and-quotas)
- [Error Handling](#error-handling)
- [Playground](#playground)

## Introduction

AIGEN exposes a **JSON-RPC 2.0** API.

- Default endpoint: `http://localhost:9944`

### JSON-RPC request format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "getChainInfo",
  "params": []
}
```

### JSON-RPC response format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { }
}
```

### Error handling

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "Method not found"
  }
}
```

## Public API Methods

For all curl calls below:

```bash
RPC=http://localhost:9944
```

### `getChainInfo()`

- Signature: `getChainInfo()`
- Params: none
- Returns: chain status (height, best hash, peers, etc.)

curl:

```bash
curl -s -X POST "$RPC" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
```

JavaScript (fetch):

```js
const res = await fetch("http://localhost:9944", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getChainInfo", params: [] })
});
console.log(await res.json());
```

Response example:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "height": 0,
    "bestHash": "0x...",
    "peers": 0
  }
}
```

### `getBlock(block_height?, block_hash?)`

- Signature: `getBlock(block_height?, block_hash?)`
- Params:
  - `block_height` (optional)
  - `block_hash` (optional)
- Returns: block structure

curl (by height):

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[0,null]}'
```

curl (by hash):

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getBlock","params":[null,"0x..."]}'
```

### `getBalance(address)`

- Signature: `getBalance(address)`
- Params:
  - `address` (string)
- Returns: balance

curl:

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getBalance","params":["0xabc..."]}'
```

### `submitTransaction(transaction)`

- Signature: `submitTransaction(transaction)`
- Params:
  - `transaction` (object)
- Returns: `SubmitTransactionResponse` (`tx_hash`, `status`)

#### Transaction schema (matches `node/src/rpc/types.rs`)

The RPC expects a `TransactionResponse` object (used as input as well):

- `sender` (string)
- `sender_public_key` (string, **required** by the server, 32-byte hex like `0x...`)
- `receiver` (string)
- `amount` (u64)
- `timestamp` (i64)
- `nonce` (u64)
- `priority` (bool)
- `tx_hash` (string, 32-byte hex)
- `signature` (string, 64-byte hex)
- `fee_base` (u64)
- `fee_priority` (u64)
- `fee_burn` (u64)
- `chain_id` (u64)
- `payload` (optional string, hex bytes)
- `ceo_signature` (optional string, 64-byte hex)

#### `tx_hash` derivation

The node rejects the request unless `tx_hash` matches the transaction contents.

Derive `tx_hash` as **SHA-256** over the UTF-8 JSON bytes of the following object:

```json
{
  "sender": "<sender>",
  "receiver": "<receiver>",
  "amount": 1,
  "timestamp": 1730000000,
  "nonce": 1,
  "priority": false,
  "chain_id": 1,
  "fee": {
    "base_fee": 1,
    "priority_fee": 0,
    "burn_amount": 0
  },
  "payload": null
}
```

If `payload` is present, the hash commits to **base64-encoded** payload bytes (not hex) in the `payload` field.

#### `signature` and sender binding

- `signature` must be an Ed25519 signature over the raw `tx_hash` bytes.
- `sender_public_key` is required by the server and must be 32 bytes.
- The server derives the sender address from `sender_public_key` and rejects the request if it does not match `sender`.

curl:

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"submitTransaction","params":[{"sender":"0x...","sender_public_key":"0x...","receiver":"0x...","amount":1,"signature":"0x...","timestamp":1730000000,"nonce":1,"priority":false,"tx_hash":"0x...","fee_base":1,"fee_priority":0,"fee_burn":0,"chain_id":1,"payload":null,"ceo_signature":null}]}'
```

### `getPendingTransactions(limit?)`

- Signature: `getPendingTransactions(limit?)`
- Params:
  - `limit` (optional number)
- Returns: list of mempool transactions

curl:

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getPendingTransactions","params":[50]}'
```

### `health()`

- Signature: `health()`
- Params: none
- Returns: health status

curl:

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"health","params":[]}'
```

## Real-Time AI Inference

### chatCompletion

OpenAI-compatible chat completion endpoint for real-time LLM inference.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "chatCompletion",
  "params": {
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Hello!"}
    ],
    "model_id": "mistral-7b",
    "stream": false,
    "max_tokens": 1000,
    "transaction": {
      "sender": "0x...",
      "receiver": "0x...",
      "amount": 1,
      "payload": "{\"user_address\":\"0x...\",\"model_id\":\"mistral-7b\",\"max_tokens\":1000}",
      "signature": "0x...",
      "timestamp": 1234567890,
      "nonce": 1,
      "chain_id": 1
    }
  },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "chatcmpl-123",
    "model_id": "mistral-7b",
    "choices": [{
      "index": 0,
      "message": {"role": "assistant", "content": "Hello! How can I help?"},
      "finish_reason": "stop"
    }],
    "usage": {
      "prompt_tokens": 20,
      "completion_tokens": 10,
      "total_tokens": 30
    },
    "ad_injected": false
  },
  "id": 1
}
```

**Pricing:**
- Subscription users: Deducted from monthly quota
- Pay-per-use: 1 AIGEN per 1000 tokens
- Free tier: 10 requests/month with ads

**Streaming:**
Use `subscribeChatCompletion` WebSocket subscription for streaming responses.

**curl example:**
```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "chatCompletion",
    "params": {
      "messages": [
        {"role": "user", "content": "Explain blockchain in simple terms"}
      ],
      "model_id": "mistral-7b",
      "stream": false
    },
    "id": 1
  }'
```

## CEO API Methods

⚠️ **Security warning**: CEO methods must require **CEO signature verification** (Ed25519). Never expose these endpoints publicly without strict network controls.

### Signature format and message construction

CEO request signatures are always verified against the hardcoded CEO key.

#### Shutdown command message format

```
shutdown:{network_magic}:{timestamp}:{nonce}:{reason}
```

`network_magic` is taken from the node's config/genesis state and is **not** provided in the RPC request.

#### SIP decision message formats

The exact bytes to sign are defined by:

- `SipProposal::message_to_sign_for_approval`
- `SipProposal::message_to_sign_for_veto`

### `submitShutdown(request)`

- Signature: `submitShutdown(request)`
- Params:
  - `request` (object) with the shape: `{timestamp, reason, nonce, signature}`

`signature` must be produced over the shutdown message format above.

curl:

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"submitShutdown","params":[{"timestamp":1730000000,"reason":"...","nonce":1,"signature":"0x..."}]}'
```

### `approveSIP(request)`

- Signature: `approveSIP(request)`
- Params:
  - `request` (object) with the shape: `{proposal_id, signature}`

`signature` must be generated over the proposal's approval message as defined by `SipProposal::message_to_sign_for_approval`.

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"approveSIP","params":[{"proposal_id":"123","signature":"0x..."}]}'
```

### `vetoSIP(request)`

- Signature: `vetoSIP(request)`

Payload shape is `{proposal_id, signature}`.

`signature` must be generated over the proposal's veto message as defined by `SipProposal::message_to_sign_for_veto`.

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"vetoSIP","params":[{"proposal_id":"123","signature":"0x..."}]}'
```

### `getSIPStatus(proposal_id)`

- Signature: `getSIPStatus(proposal_id)`
- Params:
  - `proposal_id` (string/number)

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getSIPStatus","params":["123"]}'
```

## WebSocket Subscriptions

WebSocket subscriptions allow real-time updates.

### Methods to document

- `subscribeNewBlocks`
- `subscribeNewTransactions`
- `subscribeShutdown`

### `wscat` example

```bash
# Install wscat (Node.js)
npm i -g wscat

# Connect
wscat -c ws://localhost:9944

# Subscribe to new blocks
{"jsonrpc":"2.0","id":1,"method":"subscribeNewBlocks","params":[]}
```

### JavaScript WebSocket example

```js
const ws = new WebSocket("ws://localhost:9944");
ws.onopen = () => {
  ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "subscribeNewBlocks", params: [] }));
};
ws.onmessage = (ev) => {
  console.log("WS message:", ev.data);
};
```

## Complete Working Examples

- JavaScript examples: `docs/examples/javascript/`
- Python examples: `docs/examples/python/`
- curl examples: `docs/examples/curl/`

What you'll find there:

- A minimal JSON-RPC client implementation
- Transaction submission example
- WebSocket subscription example
- Error handling patterns

## OpenAI-Compatible Endpoints

The `chatCompletion` endpoint is designed to be compatible with OpenAI's Chat Completions API, making it easy to integrate with existing applications.

### Key Differences from OpenAI API

1. **Model ID**: Use `model_id` instead of `model`
2. **Transaction Field**: Optional transaction field for pay-per-use payments
3. **Ad Injection**: Response includes `ad_injected` flag for free tier users
4. **Quota System**: Built-in quota management instead of separate billing

### Request Format

```json
{
  "jsonrpc": "2.0",
  "method": "chatCompletion",
  "params": {
    "messages": [
      {"role": "user", "content": "Hello!"}
    ],
    "model_id": "mistral-7b",
    "max_tokens": 2048,
    "temperature": 0.7,
    "stream": false,
    "transaction": {
      "type": "payment",
      "amount": 10,
      "timestamp": 1700000000,
      "nonce": 12345,
      "payload": {"service": "ai-inference"}
    }
  },
  "id": 1
}
```

### Response Format

```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "chatcmpl-abc123",
    "model_id": "mistral-7b",
    "choices": [{
      "index": 0,
      "message": {"role": "assistant", "content": "Hello! How can I help you?"},
      "finish_reason": "stop"
    }],
    "usage": {
      "prompt_tokens": 10,
      "completion_tokens": 8,
      "total_tokens": 18
    },
    "ad_injected": false
  },
  "id": 1
}
```

### Subscription Flow Example

```bash
# Check current tier and quota
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"checkQuota","params":[]}'

# Upgrade to Pro tier (requires CEO signature for admin operations)
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"subscribeTier","params":[{"tier":"pro","duration_months":1}]}'

# Make chat completion request
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":1,
    "method":"chatCompletion",
    "params":{
      "messages":[{"role":"user","content":"Explain blockchain simply"}],
      "model_id":"mistral-7b",
      "max_tokens":200
    }
  }'
```

### Pay-per-Use Flow Example

```bash
# Create payment transaction (client-side)
# Sign transaction with private key
# Include transaction in chat completion request

curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":1,
    "method":"chatCompletion",
    "params":{
      "messages":[{"role":"user","content":"Write a Python function"}],
      "model_id":"codegen-16b",
      "transaction":{
        "sender":"0x123...",
        "receiver":"0x456...",
        "amount":50,
        "signature":"0xabc...",
        "timestamp":1700000000,
        "nonce":12345
      }
    }
  }'
```

## Streaming Chat Completion

The `subscribeChatCompletion` WebSocket subscription provides real-time streaming responses.

### WebSocket Connection

```javascript
const ws = new WebSocket("ws://localhost:9944");

ws.onopen = () => {
  // Subscribe to chat completion
  ws.send(JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "subscribeChatCompletion",
    params: {
      messages: [
        {role: "user", content: "Tell me a story"}
      ],
      model_id: "mistral-7b",
      max_tokens: 500,
      subscription: "sub_123"
    }
  }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  
  if (data.method === "subscription" && data.params.subscription === "sub_123") {
    const result = data.params.result;
    
    if (result.delta) {
      // Streaming chunk
      console.log(result.delta.content);
    } else if (result.finish_reason) {
      // Stream completed
      console.log("Stream finished:", result.finish_reason);
    } else if (result.error) {
      // Error occurred
      console.error("Stream error:", result.error);
    }
  }
};
```

### Chunk Format

Each streaming chunk contains:

```json
{
  "delta": {
    "content": "Hello",
    "role": "assistant"
  }
}
```

Final message:

```json
{
  "finish_reason": "stop",
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 25,
    "total_tokens": 35
  }
}
```

### Error Handling

```json
{
  "error": {
    "code": 2004,
    "message": "Monthly quota exceeded"
  }
}
```

### Reconnection Handling

The WebSocket connection supports automatic reconnection with exponential backoff:

```javascript
let reconnectAttempts = 0;
const maxReconnectAttempts = 5;

function connect() {
  const ws = new WebSocket("ws://localhost:9944");
  
  ws.onclose = () => {
    if (reconnectAttempts < maxReconnectAttempts) {
      reconnectAttempts++;
      const delay = 1000 * Math.pow(2, reconnectAttempts - 1);
      setTimeout(connect, delay);
    }
  };
  
  ws.onopen = () => {
    reconnectAttempts = 0;
    // Resubscribe to previous subscriptions
  };
}
```

## SDK Usage

### JavaScript SDK

#### Installation

```bash
cd docs/examples/javascript
npm install
```

#### Basic Usage

```javascript
import { AIGENClient } from './aigen-sdk.js';

const client = new AIGENClient('http://localhost:9944');

await client.connect();

// Simple chat
const response = await client.chatCompletion([
  { role: 'user', content: 'Hello!' }
]);

console.log(response.choices[0].message.content);
```

#### Streaming Chat

```javascript
await client.streamChatCompletion(
  [{ role: 'user', content: 'Tell me a story' }],
  { modelId: 'mistral-7b' },
  (chunk) => process.stdout.write(chunk.delta.content),
  () => console.log('\nDone!'),
  (error) => console.error('Error:', error)
);
```

#### Error Handling

```javascript
import { QuotaExceededError, InsufficientTierError } from './aigen-sdk.js';

try {
  await client.chatCompletion(messages);
} catch (error) {
  if (error instanceof QuotaExceededError) {
    console.log('Quota exceeded!');
  } else if (error instanceof InsufficientTierError) {
    console.log('Tier too low!');
  }
}
```

### Python SDK

#### Installation

```bash
cd docs/examples/python
pip install -r requirements.txt
```

#### Basic Usage

```python
from aigen_sdk import AIGENClient

client = AIGENClient('http://localhost:9944')
client.connect()

# Simple chat
response = client.chat_completion([
    {'role': 'user', 'content': 'Hello!'}
])

print(response.choices[0]['message']['content'])
```

#### Streaming Chat

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

#### Async Streaming

```python
async for delta in client.async_stream_chat_completion(
    messages,
    model_id='mistral-7b'
):
    print(delta['content'], end='', flush=True)
```

## Authentication and Payments

### Wallet-Based Authentication

AIGEN uses Ed25519 key pairs for wallet authentication and transaction signing.

#### Key Generation

```python
import ed25519

# Generate key pair
signing_key, verifying_key = ed25519.create_keypair()
public_key = verifying_key.to_ascii(encoding="hex").decode('ascii')

# Derive address from public key
address = client.derive_address(verifying_key.to_bytes())
```

#### Transaction Signing

```python
# Create payment transaction
tx = client.create_payment_transaction(
    amount=100,
    payload={'service': 'ai-inference', 'model': 'mistral-7b'}
)

# Sign transaction
signature = client.sign_transaction(tx, signing_key.to_bytes())
```

#### Using Signed Transactions

```python
# Include signed transaction in API call
response = client.chat_completion(
    messages,
    transaction=tx,
    model_id='mistral-7b'
)
```

### Payment Transaction Structure

```json
{
  "type": "payment",
  "amount": 100,
  "timestamp": 1700000000,
  "nonce": 12345,
  "payload": {
    "service": "ai-inference",
    "model": "mistral-7b",
    "user_address": "0x123..."
  }
}
```

### Security Best Practices

1. **Never expose private keys** in client-side code
2. **Use environment variables** for sensitive configuration
3. **Validate transaction receipts** before trusting payment
4. **Implement proper key management** in production
5. **Use HTTPS/WSS** for all network communications
6. **Implement rate limiting** on client-side
7. **Monitor for suspicious activity** in transaction patterns

## Rate Limiting and Quotas

### Tier-Based Rate Limits

| Tier | Monthly Limit | Features |
|------|---------------|----------|
| Free | 10 requests | Basic models, ads |
| Basic | 100 requests | All models |
| Pro | 1,000 requests | All models, priority |
| Unlimited | Pay-per-use | All models, priority, no limits |

### Quota Management

#### Check Current Quota

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"checkQuota","params":[]}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "used": 45,
    "limit": 100,
    "reset_date": "2024-02-01T00:00:00Z"
  },
  "id": 1
}
```

#### Get Tier Information

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTierInfo","params":[]}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "current_tier": "basic",
    "available_tiers": ["free", "basic", "pro", "unlimited"],
    "tier_limits": {
      "free": 10,
      "basic": 100,
      "pro": 1000,
      "unlimited": -1
    }
  },
  "id": 1
}
```

### Quota Reset Windows

- **Monthly Reset**: 1st of each month at 00:00 UTC
- **Tier Upgrades**: Immediate quota increase
- **Tier Downgrades**: Effective next billing cycle

### Handling Quota Exceeded

When quota is exceeded, you'll receive error code `2004`:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": 2004,
    "message": "Monthly quota exceeded",
    "data": {
      "used": 100,
      "limit": 100,
      "reset_date": "2024-02-01T00:00:00Z"
    }
  },
  "id": 1
}
```

**Resolution strategies:**
1. **Wait for reset**: Quota resets monthly
2. **Upgrade tier**: Immediate access to higher limits
3. **Use pay-per-use**: Switch to unlimited tier
4. **Optimize usage**: Reduce token consumption

## Error Handling

### Error Code Reference

| Code | Name | Description | Resolution |
|------|------|-------------|------------|
| 2003 | Insufficient Tier | Current tier doesn't support feature | Upgrade subscription tier |
| 2004 | Quota Exceeded | Monthly quota limit reached | Wait for reset or upgrade |
| 2007 | Payment Required | Payment needed for operation | Add payment transaction |
| 2008 | Context Length Exceeded | Input too long for model | Reduce input tokens |
| -32601 | Method Not Found | Invalid RPC method | Check method name |
| -32602 | Invalid Params | Invalid parameters | Validate input format |
| -32603 | Internal Error | Server error | Retry or contact support |

### Common Error Scenarios

#### Insufficient Tier Error

**Scenario**: Trying to use a premium model with a basic tier.

**Error:**
```json
{
  "error": {
    "code": 2003,
    "message": "Current tier does not support this model",
    "data": {
      "current_tier": "basic",
      "required_tier": "pro",
      "model": "llama2-13b"
    }
  }
}
```

**Resolution**: Upgrade to Pro tier or use a model available for your current tier.

#### Context Length Exceeded

**Scenario**: Input message too long for the model's context window.

**Error:**
```json
{
  "error": {
    "code": 2008,
    "message": "Context length exceeded",
    "data": {
      "input_tokens": 8500,
      "max_context": 8192,
      "model": "mistral-7b"
    }
  }
}
```

**Resolution**: Reduce input length or use a model with larger context window.

#### Payment Required

**Scenario**: Pay-per-use request without valid payment transaction.

**Error:**
```json
{
  "error": {
    "code": 2007,
    "message": "Payment required for this operation",
    "data": {
      "estimated_cost": 25,
      "current_balance": 0
    }
  }
}
```

**Resolution**: Add a valid payment transaction to the request.

### Retry Strategies

#### Exponential Backoff

```python
import time
import random

def exponential_backoff(attempt, base_delay=1, max_delay=60):
    delay = min(base_delay * (2 ** attempt) + random.uniform(0, 1), max_delay)
    return delay

# Usage
for attempt in range(max_retries):
    try:
        response = client.chat_completion(messages)
        break
    except AIGENError as e:
        if e.code in ['-32603', 'REQUEST_ERROR']:  # Retryable errors
            delay = exponential_backoff(attempt)
            time.sleep(delay)
            continue
        else:
            raise
```

#### Circuit Breaker Pattern

```python
class CircuitBreaker:
    def __init__(self, failure_threshold=5, timeout=60):
        self.failure_threshold = failure_threshold
        self.timeout = timeout
        self.failure_count = 0
        self.last_failure_time = None
        self.state = 'closed'  # closed, open, half-open
    
    def call(self, func, *args, **kwargs):
        if self.state == 'open':
            if time.time() - self.last_failure_time > self.timeout:
                self.state = 'half-open'
            else:
                raise Exception("Circuit breaker is open")
        
        try:
            result = func(*args, **kwargs)
            if self.state == 'half-open':
                self.state = 'closed'
                self.failure_count = 0
            return result
        except Exception as e:
            self.failure_count += 1
            self.last_failure_time = time.time()
            
            if self.failure_count >= self.failure_threshold:
                self.state = 'open'
            raise

# Usage
circuit_breaker = CircuitBreaker()

try:
    response = circuit_breaker.call(client.chat_completion, messages)
except Exception as e:
    print("Service unavailable due to circuit breaker")
```

## Playground

The AIGEN AI Playground provides an interactive web interface for testing and experimenting with the AI models.

### Access the Playground

Open `docs/playground/index.html` in your browser:

```bash
# Option 1: Open directly (file://)
start docs/playground/index.html  # Windows
open docs/playground/index.html   # macOS
xdg-open docs/playground/index.html  # Linux

# Option 2: Serve with Python
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

### Quick Start

1. Ensure node is running (see Step 4)
2. Open playground in browser
3. Select model and tier
4. Start chatting!

The playground includes:
- Responsive design for mobile and desktop
- Dark/light theme toggle
- Real-time quota tracking
- Connection status indicators
- Error handling with user-friendly messages
- Export functionality for chat history