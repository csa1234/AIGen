#!/usr/bin/env python3
# Copyright (c) 2025-present Cesar Saguier Antebi
#
# This file is part of AIGEN Blockchain.
#
# Licensed under the Business Source License 1.1 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     `https://github.com/yourusername/aigen/blob/main/LICENSE`
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""
AIGEN Python SDK Example - Comprehensive demonstration of all SDK features
"""

import asyncio
import sys
from aigen_sdk import AIGENClient, AIGENError, QuotaExceededError, InsufficientTierError, PaymentRequiredError


def print_section(title):
    """Print a formatted section header"""
    print(f"\n{'='*60}")
    print(f"🚀 {title}")
    print(f"{'='*60}\n")


def print_error(error):
    """Print formatted error information"""
    print(f"❌ Error: {error}")
    if hasattr(error, 'code'):
        print(f"🔍 Error code: {error.code}")
    if hasattr(error, 'details'):
        print(f"📊 Error details: {error.details}")
    print()


async def main():
    """Main demonstration function"""
    print("🚀 AIGEN Python SDK Example\n")
    
    client = AIGENClient('http://localhost:9944', 'ws://localhost:9944')
    
    try:
        print_section("1. Connecting to AIGEN Network")
        await client.connect()
        print("✅ Connected successfully!\n")
        
        print_section("2. Checking Quota")
        quota = client.check_quota()
        print(f"📊 Quota: {quota.used}/{quota.limit} (Reset: {quota.reset_date})\n")
        
        print_section("3. Getting Tier Information")
        tier_info = client.get_tier_info()
        print(f"🏆 Current tier: {tier_info.current_tier}")
        print(f"📋 Available tiers: {', '.join(tier_info.available_tiers)}")
        print(f"📈 Tier limits: {tier_info.tier_limits}")
        print()
        
        print_section("4. Listing Available Models")
        models = client.list_models()
        print(f"🤖 Available models ({len(models)}):")
        for model in models:
            print(f"   - {model.name} ({model.id}): {model.description}")
            print(f"     Tier required: {model.tier_required}, Max tokens: {model.max_tokens}")
        print()
        
        print_section("5. Getting Specific Model Info")
        model_info = client.get_model_info('mistral-7b')
        print(f"🔍 Mistral 7B details:")
        print(f"   Name: {model_info.name}")
        print(f"   Description: {model_info.description}")
        print(f"   Tier required: {model_info.tier_required}")
        print(f"   Max tokens: {model_info.max_tokens}")
        print(f"   Context length: {model_info.context_length}")
        print()
        
        print_section("6. Simple Chat Completion")
        messages = [
            {"role": "user", "content": "Hello! Can you introduce yourself and explain what AIGEN is?"}
        ]
        
        print(f"💬 User: {messages[0]['content']}")
        response = client.chat_completion(messages, model_id='mistral-7b', max_tokens=512, temperature=0.7)
        
        print(f"🤖 Assistant: {response.choices[0]['message']['content']}")
        print(f"📊 Usage: {response.usage} tokens")
        print()
        
        print_section("7. Streaming Chat Completion")
        conversation = [
            {"role": "user", "content": "Tell me a short story about a blockchain developer who discovers a magical smart contract."}
        ]
        
        print(f"💬 User: {conversation[0]['content']}")
        print("\n🤖 Assistant (streaming response):")
        
        token_count = 0
        start_time = time.time()
        
        def on_chunk(delta):
            nonlocal token_count
            if delta.get('content'):
                print(delta['content'], end='', flush=True)
                token_count += 1
        
        def on_complete(result):
            end_time = time.time()
            duration = end_time - start_time
            print(f"\n\n✅ Stream completed!")
            print(f"📊 Tokens: {token_count}")
            print(f"⏱️  Duration: {duration:.2f}s")
            print(f"⚡ Speed: {token_count/duration:.1f} tokens/sec")
            print(f"🏁 Finish reason: {result['finish_reason']}")
        
        def on_error(error):
            print(f"\n❌ Stream error: {error}")
        
        await client.stream_chat_completion(
            conversation,
            on_chunk=on_chunk,
            on_complete=on_complete,
            on_error=on_error,
            model_id='mistral-7b',
            max_tokens=1024,
            temperature=0.8
        )
        
        print_section("8. Demonstrating Error Handling")
        try:
            client.chat_completion([
                {"role": "user", "content": "Test message"}
            ], model_id='non-existent-model')
        except InsufficientTierError as e:
            print("❌ Insufficient tier error caught:")
            print_error(e)
        except QuotaExceededError as e:
            print("❌ Quota exceeded error caught:")
            print_error(e)
            print("💡 Tip: Wait for quota reset or upgrade your subscription")
        except PaymentRequiredError as e:
            print("❌ Payment required error caught:")
            print_error(e)
            print("💡 Tip: Add a payment transaction to continue")
        except AIGENError as e:
            print("❌ General AIGEN error caught:")
            print_error(e)
        
        print_section("9. Wallet Authentication Example")
        try:
            import ed25519
            
            print("🔑 Generating wallet key pair...")
            signing_key, verifying_key = ed25519.create_keypair()
            public_key = verifying_key.to_ascii(encoding="hex").decode('ascii')
            
            print(f"📍 Public key: {public_key[:16]}...")
            
            address = client.derive_address(verifying_key.to_bytes())
            print(f"🏠 Derived address: {address}")
            
            payment_tx = client.create_payment_transaction(50.0, {
                "service": "ai-inference",
                "model": "mistral-7b"
            })
            print(f"💳 Created payment transaction: {payment_tx}")
            
            signature = client.sign_transaction(payment_tx, signing_key.to_bytes())
            print(f"✍️ Transaction signature: {signature[:32]}...")
            
            print("✅ Wallet authentication setup complete!")
            print("💡 In a real scenario, you would sign the transaction and include it in API calls")
            
        except ImportError:
            print("⚠️  Wallet features require ed25519 package")
            print("📦 Install with: pip install ed25519")
        except Exception as e:
            print(f"⚠️  Wallet setup error: {e}")
        print()
        
        print_section("10. Batch Processing Example")
        try:
            batch_data = [
                {"input": "Translate to French: Hello world"},
                {"input": "Translate to Spanish: Good morning"},
                {"input": "Translate to German: Good evening"}
            ]
            
            print("📦 Submitting batch request...")
            batch_job = client.submit_batch_request('normal', 'mistral-7b', batch_data)
            print(f"🎯 Batch job submitted: {batch_job.id}")
            print(f"📊 Status: {batch_job.status}")
            print(f"📅 Created: {batch_job.created_at}")
            
            print("\n⏳ Checking batch status...")
            status = client.get_batch_status(batch_job.id)
            print(f"📋 Current status: {status.status}")
            if status.result:
                print(f"📊 Result: {status.result}")
            if status.error:
                print(f"❌ Error: {status.error}")
            
            print("\n📋 Listing user jobs...")
            jobs = client.list_user_jobs()
            print(f"📊 Total jobs: {len(jobs)}")
            for job in jobs[:3]:  # Show first 3 jobs
                print(f"   - {job.id}: {job.status} ({job.model_id})")
            if len(jobs) > 3:
                print(f"   ... and {len(jobs) - 3} more jobs")
                
        except Exception as e:
            print(f"⚠️  Batch processing not available or error occurred: {e}")
        print()
        
        print_section("11. Async Streaming Example")
        try:
            async_prompt = [
                {"role": "user", "content": "Write a haiku about artificial intelligence and blockchain technology."}
            ]
            
            print(f"💬 User: {async_prompt[0]['content']}")
            print("\n🤖 Assistant (async streaming):")
            
            token_count = 0
            start_time = time.time()
            
            async for delta in client.async_stream_chat_completion(
                async_prompt,
                model_id='mistral-7b',
                max_tokens=256,
                temperature=0.7
            ):
                if delta.get('content'):
                    print(delta['content'], end='', flush=True)
                    token_count += 1
            
            end_time = time.time()
            duration = end_time - start_time
            
            print(f"\n\n✅ Async stream completed!")
            print(f"📊 Tokens: {token_count}")
            print(f"⏱️  Duration: {duration:.2f}s")
            print(f"⚡ Speed: {token_count/duration:.1f} tokens/sec")
            
        except Exception as e:
            print(f"⚠️  Async streaming error: {e}")
        print()
        
        print_section("12. Final Quota Check")
        final_quota = client.check_quota()
        print(f"📊 Final quota: {final_quota.used}/{final_quota.limit}")
        tokens_used = final_quota.used - quota.used
        print(f"📈 Used {tokens_used} tokens during this session")
        
        if tokens_used > 0:
            print(f"💡 Average tokens per request: {tokens_used / 3:.1f}")
        
    except Exception as e:
        print(f"❌ Fatal error: {e}")
        if isinstance(e, AIGENError):
            print_error(e)
        
    finally:
        print("\n👋 Disconnecting...")
        client.disconnect()
        print("✅ Disconnected successfully!")
        print("\n🎉 SDK demonstration completed!")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n\n👋 Interrupted by user")
        sys.exit(0)
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}")
        sys.exit(1)
