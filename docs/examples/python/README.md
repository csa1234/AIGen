<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# AIGEN Python SDK

A comprehensive Python SDK for interacting with the AIGEN blockchain AI platform. This SDK provides a simple and powerful interface for chat completions, streaming responses, model management, wallet authentication, and batch processing.

## Features

- **🤖 Chat Completions**: Simple and streaming chat completion support
- **🔗 Multiple Models**: Support for Mistral 7B, Llama 2 13B, CodeGen 16B, and more
- **💳 Wallet Integration**: Built-in wallet authentication and transaction signing
- **📊 Quota Management**: Real-time quota tracking and tier management
- **⚡ Streaming**: Real-time token-by-token streaming responses with async support
- **🔄 Auto-retry**: Automatic retry with exponential backoff using tenacity
- **🛡️ Error Handling**: Comprehensive error types and handling
- **📦 Batch Processing**: Support for batch AI requests
- **🔄 Async Support**: Full async/await support for non-blocking operations
- **🔧 Type Hints**: Complete type annotations for better IDE support

## Installation

### Using Virtual Environment (Recommended)

```bash
# Create virtual environment
python -m venv .venv

# Activate virtual environment
# Linux/Mac:
source .venv/bin/activate
# Windows:
.venv\Scripts\activate

# Install dependencies
pip install -r requirements.txt
```

### Direct Installation

```bash
pip install requests websockets ed25519 aiohttp tenacity typing-extensions
```

## Quick Start

### Basic Usage

```python
from aigen_sdk import AIGENClient

client = AIGENClient('http://localhost:9944')

# Simple chat completion
response = client.chat_completion([
    {'role': 'user', 'content': 'Hello! How can you help me?'}
])

print(response.choices[0]['message']['content'])
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

## SDK Methods

### Core Methods

#### `__init__(rpc_url, ws_url=None, wallet_address=None)`

Creates a new AIGEN client instance.

```python
client = AIGENClient(
    'http://localhost:9944',     # RPC URL
    'ws://localhost:9944',       # WebSocket URL (optional)
    '0x123...abc'               # Wallet address (optional)
)
```

#### `connect()`

Establishes connection to the AIGEN network.

```python
await client.connect()
```

#### `chat_completion(messages, **options)`

Sends a chat completion request.

```python
response = client.chat_completion([
    {'role': 'user', 'content': 'Hello!'}
], model_id='mistral-7b', max_tokens=2048, temperature=0.7)
```

#### `async_chat_completion(messages, **options)`

Asynchronous chat completion.

```python
response = await client.async_chat_completion([
    {'role': 'user', 'content': 'Hello!'}
], model_id='mistral-7b')
```

#### `stream_chat_completion(messages, on_chunk, on_complete=None, on_error=None, **options)`

Streaming chat completion with callbacks.

```python
await client.stream_chat_completion(
    messages,
    on_chunk=lambda delta: print(delta['content'], end=''),
    on_complete=lambda result: print('\nDone!'),
    on_error=lambda error: print(f'Error: {error}'),
    model_id='mistral-7b'
)
```

#### `async_stream_chat_completion(messages, **options)`

Async generator for streaming chat completion.

```python
async for delta in client.async_stream_chat_completion(
    messages,
    model_id='mistral-7b'
):
    print(delta['content'], end='', flush=True)
```

### Model Management

#### `list_models()`

Lists all available AI models.

```python
models = client.list_models()
for model in models:
    print(f"{model.name}: {model.description}")
```

#### `get_model_info(model_id)`

Gets detailed information about a specific model.

```python
info = client.get_model_info('mistral-7b')
print(f"Model: {info.name}, Tier: {info.tier_required}")
```

#### `load_model(model_id, transaction=None)`

Loads a model (for models not currently loaded).

```python
await client.load_model('llama2-13b')
```

### Tier and Quota Management

#### `check_quota()`

Checks current quota usage.

```python
quota = client.check_quota()
print(f"Used: {quota.used}/{quota.limit}")
```

#### `get_tier_info()`

Gets current tier information.

```python
tier_info = client.get_tier_info()
print(f"Current tier: {tier_info.current_tier}")
```

#### `subscribe_tier(tier, duration_months=1, transaction=None)`

Subscribes to a tier.

```python
await client.subscribe_tier('pro', 1)
```

### Batch Processing

#### `submit_batch_request(priority, model_id, input_data, transaction=None)`

Submits a batch processing request.

```python
job = await client.submit_batch_request('normal', 'mistral-7b', [
    {'input': 'Translate: Hello world'},
    {'input': 'Translate: Good morning'}
])
```

#### `get_batch_status(job_id)`

Checks the status of a batch job.

```python
status = await client.get_batch_status(job.id)
print(f"Job status: {status.status}")
```

#### `list_user_jobs(status=None)`

Lists user's batch jobs.

```python
jobs = await client.list_user_jobs('pending')
for job in jobs:
    print(f"{job.id}: {job.status}")
```

### Wallet Authentication

#### `sign_transaction(tx, private_key)`

Signs a transaction with a private key.

```python
signature = client.sign_transaction(tx, private_key)
```

#### `derive_address(public_key)`

Derives an address from a public key.

```python
address = client.derive_address(public_key)
```

#### `create_payment_transaction(amount, payload=None)`

Creates a payment transaction.

```python
tx = client.create_payment_transaction(100, {
    'service': 'ai-inference',
    'model': 'mistral-7b'
})
```

## Error Handling

The SDK provides specific error types for different scenarios:

```python
from aigen_sdk import (
    AIGENError, 
    QuotaExceededError, 
    InsufficientTierError, 
    PaymentRequiredError
)

try:
    response = await client.chat_completion(messages)
response = client.chat_completion(messages)
    print('Quota exceeded!')
except InsufficientTierError as e:
    print('Tier too low!')
except PaymentRequiredError as e:
    print('Payment required!')
    print('Payment required!')
except AIGENError as e:
    print(f'AIGEN error: {e.message}')
```

## Configuration Options

### Chat Completion Options

```python
options = {
    'model_id': 'mistral-7b',     # Model to use
    'max_tokens': 2048,           # Maximum tokens to generate
    'temperature': 0.7,          # Randomness (0.0-2.0)
    'transaction': { ... }        # Payment transaction
}
```

### Retry Configuration

The SDK automatically retries failed requests with exponential backoff:

- **Max Retries**: 3 attempts
- **Base Delay**: 1.0 seconds
- **Max Delay**: 10.0 seconds
- **Backoff Multiplier**: 2.0x

## Examples

### Running Examples

```bash
# Basic chat example
python chat_example.py

# Streaming example
python stream_example.py

# Full SDK demonstration
python sdk_example.py
```

### Custom Implementation

```python
import asyncio
from aigen_sdk import AIGENClient

async def my_ai_chat():
    client = AIGENClient('http://localhost:9944')
    
    try:
        await client.connect()
        
        quota = client.check_quota()
        print(f"Quota: {quota.used}/{quota.limit}")
        
        response = await client.async_chat_completion([
            {'role': 'user', 'content': 'Hello, AI!'}
        ])
        
        print(response.choices[0]['message']['content'])
        
    except Exception as e:
        print(f'Error: {e}')
    finally:
        client.disconnect()

asyncio.run(my_ai_chat())
```

### Advanced Streaming Example

```python
import asyncio
from aigen_sdk import AIGENClient

async def advanced_streaming():
    client = AIGENClient('http://localhost:9944', 'ws://localhost:9944')
    await client.connect()
    
    # Using async generator for maximum control
    messages = [{'role': 'user', 'content': 'Write a poem about AI and blockchain'}]
    
    token_count = 0
    start_time = time.time()
    
    async for delta in client.async_stream_chat_completion(
        messages,
        model_id='mistral-7b',
        max_tokens=500,
        temperature=0.8
    ):
        if delta.get('content'):
            print(delta['content'], end='', flush=True)
            token_count += 1
    
    duration = time.time() - start_time
    print(f"\n\nGenerated {token_count} tokens in {duration:.2f}s")
    print(f"Speed: {token_count/duration:.1f} tokens/sec")
    
    client.disconnect()

asyncio.run(advanced_streaming())
```

## Type Hints and Data Classes

The SDK provides comprehensive type hints and data classes:

```python
from aigen_sdk import ChatMessage, ChatCompletionOptions, ModelInfo, QuotaInfo

# Type-safe message creation
message = ChatMessage(role='user', content='Hello!')

# Type-safe options
options = ChatCompletionOptions(
    model_id='mistral-7b',
    max_tokens=2048,
    temperature=0.7
)

# Type-safe response handling
models: List[ModelInfo] = client.list_models()
quota: QuotaInfo = client.check_quota()
```

## Best Practices

1. **Always Connect First**: Call `connect()` before making API calls
2. **Handle Errors**: Use try-except blocks with specific error types
3. **Check Quota**: Monitor quota usage to avoid unexpected limits
4. **Use Async**: For better performance with streaming
5. **Disconnect Properly**: Always call `disconnect()` when done
6. **Use Virtual Environments**: Isolate dependencies
7. **Type Hints**: Leverage type hints for better IDE support

## Troubleshooting

### Connection Issues

```python
try:
    await client.connect()
except AIGENError as e:
    print(f'Connection failed: {e.message}')
    # Check if AIGEN node is running
    # Verify RPC/WS URLs
```

### Quota Issues

```python
quota = client.check_quota()
if quota.used >= quota.limit:
    print('Quota exceeded!')
    # Upgrade tier or wait for reset
```

### Model Issues

```python
try:
    response = await client.chat_completion(messages, model_id='premium-model')
except InsufficientTierError as e:
    print('Upgrade tier for this model')
```

### Import Issues

```bash
# If you get import errors, install missing dependencies
pip install requests websockets ed25519 aiohttp tenacity typing-extensions
```

## Support

For issues and questions:
- Check the [API Documentation](../API.md)
- Review the [Getting Started Guide](../GUIDE.md)
- Test with the [Interactive Playground](../../playground/)
- Open an issue in the repository

---

**Happy Coding!** 🐍🚀
