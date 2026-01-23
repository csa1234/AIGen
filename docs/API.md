# RPC API Usage Guide

## Table of Contents

- [Introduction](#introduction)
- [Public API Methods](#public-api-methods)
- [Real-Time AI Inference](#real-time-ai-inference)
- [CEO API Methods](#ceo-api-methods)
- [WebSocket Subscriptions](#websocket-subscriptions)
- [Complete Working Examples](#complete-working-examples)

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
- `sender_public_key` (string, **required** by server, 32-byte hex like `0x...`)
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

`network_magic` is taken from the node’s config/genesis state and is **not** provided in the RPC request.

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

`signature` must be generated over the proposal’s approval message as defined by `SipProposal::message_to_sign_for_approval`.

```bash
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"approveSIP","params":[{"proposal_id":"123","signature":"0x..."}]}'
```

### `vetoSIP(request)`

- Signature: `vetoSIP(request)`

Payload shape is `{proposal_id, signature}`.

`signature` must be generated over the proposal’s veto message as defined by `SipProposal::message_to_sign_for_veto`.

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

What you’ll find there:

- A minimal JSON-RPC client implementation
- Transaction submission example
- WebSocket subscription example
- Error handling patterns
