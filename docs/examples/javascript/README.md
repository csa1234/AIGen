<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# AIGEN JavaScript SDK

A comprehensive JavaScript SDK for interacting with the AIGEN blockchain AI platform. This SDK provides a simple and powerful interface for chat completions, streaming responses, model management, and wallet authentication.

## Features

- **🤖 Chat Completions**: Simple and streaming chat completion support
- **🔗 Multiple Models**: Support for Mistral 7B, Llama 2 13B, CodeGen 16B, and more
- **💳 Wallet Integration**: Built-in wallet authentication and transaction signing
- **📊 Quota Management**: Real-time quota tracking and tier management
- **⚡ Streaming**: Real-time token-by-token streaming responses
- **🔄 Auto-retry**: Automatic retry with exponential backoff
- **🛡️ Error Handling**: Comprehensive error types and handling
- **📦 Batch Processing**: Support for batch AI requests
- **🔧 TypeScript**: Full TypeScript support with type definitions

## Installation

```bash
npm install
```

## Quick Start

### Basic Usage

```javascript
import { AIGENClient } from './aigen-sdk.js';

const client = new AIGENClient('http://localhost:9944');

// Simple chat completion
const response = await client.chatCompletion([
    { role: 'user', content: 'Hello! How can you help me?' }
]);

console.log(response.choices[0].message.content);
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

## SDK Methods

### Core Methods

#### `constructor(rpcUrl, wsUrl?, walletAddress?)`

Creates a new AIGEN client instance.

```javascript
const client = new AIGENClient(
    'http://localhost:9944',           // RPC URL
    'ws://localhost:9944',             // WebSocket URL (optional)
    '0x123...abc'                      // Wallet address (optional)
);
```

#### `connect()`

Establishes connection to the AIGEN network.

```javascript
await client.connect();
```

#### `chatCompletion(messages, options?)`

Sends a chat completion request.

```javascript
const response = await client.chatCompletion([
    { role: 'user', content: 'Hello!' }
], {
    modelId: 'mistral-7b',
    maxTokens: 2048,
    temperature: 0.7,
    stream: false
});
```

#### `streamChatCompletion(messages, options, onChunk, onComplete?, onError?)`

Sends a streaming chat completion request.

```javascript
await client.streamChatCompletion(
    messages,
    options,
    (delta) => {
        // Called for each token
        console.log(delta.content);
    },
    (result) => {
        // Called when complete
        console.log('Done!', result);
    },
    (error) => {
        // Called on error
        console.error('Error:', error);
    }
);
```

### Model Management

#### `listModels()`

Lists all available AI models.

```javascript
const models = await client.listModels();
console.log(models);
// Output: [{ id: 'mistral-7b', name: 'Mistral 7B', ... }]
```

#### `getModelInfo(modelId)`

Gets detailed information about a specific model.

```javascript
const info = await client.getModelInfo('mistral-7b');
console.log(info);
```

#### `loadModel(modelId, transaction?)`

Loads a model (for models not currently loaded).

```javascript
await client.loadModel('llama2-13b');
```

### Tier and Quota Management

#### `checkQuota()`

Checks current quota usage.

```javascript
const quota = await client.checkQuota();
console.log(`Used: ${quota.used}/${quota.limit}`);
```

#### `getTierInfo()`

Gets current tier information.

```javascript
const tierInfo = await client.getTierInfo();
console.log(`Current tier: ${tierInfo.current_tier}`);
```

#### `subscribeTier(tier, durationMonths?, transaction?)`

Subscribes to a tier.

```javascript
await client.subscribeTier('pro', 1);
```

### Batch Processing

#### `submitBatchRequest(priority, modelId, inputData, transaction?)`

Submits a batch processing request.

```javascript
const job = await client.submitBatchRequest('normal', 'mistral-7b', [
    { input: 'Translate: Hello world' },
    { input: 'Translate: Good morning' }
]);
```

#### `getBatchStatus(jobId)`

Checks the status of a batch job.

```javascript
const status = await client.getBatchStatus(job.id);
console.log(status);
```

#### `listUserJobs(status?)`

Lists user's batch jobs.

```javascript
const jobs = await client.listUserJobs('pending');
```

### Wallet Authentication

#### `signTransaction(tx, privateKey)`

Signs a transaction with a private key.

```javascript
const signature = client.signTransaction(tx, privateKey);
```

#### `deriveAddress(publicKey)`

Derives an address from a public key.

```javascript
const address = client.deriveAddress(publicKey);
```

#### `createPaymentTransaction(amount, payload?)`

Creates a payment transaction.

```javascript
const tx = client.createPaymentTransaction(100, {
    service: 'ai-inference',
    model: 'mistral-7b'
});
```

## Error Handling

The SDK provides specific error types for different scenarios:

```javascript
import { 
    AIGENError, 
    QuotaExceededError, 
    InsufficientTierError, 
    PaymentRequiredError 
} from './aigen-sdk.js';

try {
    await client.chatCompletion(messages);
} catch (error) {
    if (error instanceof QuotaExceededError) {
        console.log('Quota exceeded!');
    } else if (error instanceof InsufficientTierError) {
        console.log('Tier too low!');
    } else if (error instanceof PaymentRequiredError) {
        console.log('Payment required!');
    } else if (error instanceof AIGENError) {
        console.log('AIGEN error:', error.message);
    }
}
```

## Configuration Options

### Chat Completion Options

```javascript
const options = {
    modelId: 'mistral-7b',        // Model to use
    maxTokens: 2048,              // Maximum tokens to generate
    temperature: 0.7,             // Randomness (0.0-2.0)
    stream: false,                // Enable streaming
    transaction: { ... }          // Payment transaction
};
```

### Retry Configuration

The SDK automatically retries failed requests with exponential backoff:

- **Max Retries**: 3 attempts
- **Base Delay**: 1000ms
- **Max Delay**: 10000ms
- **Backoff Multiplier**: 2x

## Examples

### Running Examples

```bash
# Basic chat example
npm run chat

# Streaming example
npm run stream

# Full SDK demonstration
npm run sdk
```

### Custom Implementation

```javascript
import { AIGENClient } from './aigen-sdk.js';

async function myAIChat() {
    const client = new AIGENClient('http://localhost:9944');
    
    try {
        await client.connect();
        
        const quota = await client.checkQuota();
        console.log(`Quota: ${quota.used}/${quota.limit}`);
        
        const response = await client.chatCompletion([
            { role: 'user', content: 'Hello, AI!' }
        ]);
        
        console.log(response.choices[0].message.content);
        
    } catch (error) {
        console.error('Error:', error.message);
    } finally {
        client.disconnect();
    }
}

myAIChat();
```

## TypeScript Support

The SDK includes TypeScript definitions for enhanced development experience:

```typescript
import { AIGENClient, ChatMessage, ChatCompletionOptions } from './aigen-sdk.js';

const client = new AIGENClient('http://localhost:9944');
const messages: ChatMessage[] = [
    { role: 'user', content: 'Hello!' }
];

const options: ChatCompletionOptions = {
    modelId: 'mistral-7b',
    maxTokens: 2048
};
```

## Best Practices

1. **Always Connect First**: Call `connect()` before making API calls
2. **Handle Errors**: Use try-catch blocks with specific error types
3. **Check Quota**: Monitor quota usage to avoid unexpected limits
4. **Use Streaming**: For better user experience with long responses
5. **Disconnect Properly**: Always call `disconnect()` when done
6. **Retry Logic**: The SDK handles retries automatically, but implement your own for critical operations

## Troubleshooting

### Connection Issues

```javascript
try {
    await client.connect();
} catch (error) {
    console.error('Connection failed:', error.message);
    // Check if AIGEN node is running
    // Verify RPC/WS URLs
}
```

### Quota Issues

```javascript
const quota = await client.checkQuota();
if (quota.used >= quota.limit) {
    console.log('Quota exceeded!');
    // Upgrade tier or wait for reset
}
```

### Model Issues

```javascript
try {
    await client.chatCompletion(messages, { modelId: 'premium-model' });
} catch (error) {
    if (error instanceof InsufficientTierError) {
        console.log('Upgrade tier for this model');
    }
}
```

## Support

For issues and questions:
- Check the [API Documentation](../API.md)
- Review the [Getting Started Guide](../GUIDE.md)
- Test with the [Interactive Playground](../../playground/)
- Open an issue in the repository

---

**Happy Coding!** 🚀