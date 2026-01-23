#!/usr/bin/env python3
"""
AIGEN Chat Example with Python SDK - Demonstrates chat completion using the enhanced SDK
"""

import os
import asyncio
import sys
from aigen_sdk import AIGENClient, AIGENError, QuotaExceededError, InsufficientTierError, PaymentRequiredError


def print_section(title):
    """Print a formatted section header"""
    print(f"\n{'='*60}")
    print(f"🤖 {title}")
    print(f"{'='*60}\n")


async def main():
    """Main demonstration function"""
    print("🤖 AIGEN Chat Example with Python SDK\n")
    
    # Use environment variable or default
    rpc_url = os.environ.get("AIGEN_RPC_URL", "http://localhost:9944")
    
    client = AIGENClient(rpc_url)
    
    try:
        print("🔗 Connecting to AIGEN network...")
        await client.connect()
        print("✅ Connected successfully!\n")
        
        print("📊 Checking quota...")
        quota = client.check_quota()
        print(f"📈 Quota: {quota.used}/{quota.limit} (Reset: {quota.reset_date})\n")
        
        print("🏆 Getting tier information...")
        tier_info = client.get_tier_info()
        print(f"Current tier: {tier_info.current_tier}")
        print(f"Available tiers: {', '.join(tier_info.available_tiers)}\n")
        
        print("🤖 Listing available models...")
        models = client.list_models()
        print(f"Available models ({len(models)}):")
        for model in models:
            print(f"  - {model.name} ({model.id}): {model.description}")
        print()
        
        print_section("Example 1: Simple Chat")
        messages1 = [
            {"role": "user", "content": "Hello! Can you explain what blockchain is in simple terms?"}
        ]
        
        print(f"💬 User: {messages1[0]['content']}")
        response1 = client.chat_completion(messages1, model_id="mistral-7b", max_tokens=200, temperature=0.7)
        
        print(f"🤖 Assistant: {response1.choices[0]['message']['content']}")
        print(f"📊 Usage: {response1.usage} tokens")
        print(f"💰 Ad injected: {response1.ad_injected}")
        print()
        
        print_section("Example 2: Multi-turn Conversation")
        messages2 = [
            {"role": "system", "content": "You are a helpful assistant that explains complex topics simply."},
            {"role": "user", "content": "What is proof of work?"},
            {"role": "assistant", "content": "Proof of work is like a puzzle that computers solve to validate transactions."},
            {"role": "user", "content": "Can you give me a real-world analogy?"}
        ]
        
        print(f"💬 User: {messages2[-1]['content']}")
        response2 = client.chat_completion(messages2, model_id="mistral-7b", max_tokens=200)
        
        print(f"🤖 Assistant: {response2.choices[0]['message']['content']}")
        print()
        
        print_section("Example 3: Code Generation")
        messages3 = [
            {"role": "user", "content": "Write a simple Python function to calculate fibonacci numbers with error handling"}
        ]
        
        print(f"💬 User: {messages3[0]['content']}")
        response3 = client.chat_completion(messages3, model_id="codegen-16b", max_tokens=300, temperature=0.3)
        
        print("🤖 Generated code:")
        print("```python")
        print(response3.choices[0]['message']['content'])
        print("```")
        print()
        
        print_section("Example 4: Error Handling")
        try:
            client.chat_completion([
                {"role": "user", "content": "Test message"}
            ], model_id="non-existent-model")
        except InsufficientTierError as e:
            print("❌ Insufficient tier error:")
            print(f"   Message: {e}")
            print(f"   Code: {e.code}")
        except QuotaExceededError as e:
            print("❌ Quota exceeded error:")
            print(f"   Message: {e}")
            print(f"   Code: {e.code}")
            print("💡 Tip: Wait for quota reset or upgrade your subscription")
        except PaymentRequiredError as e:
            print("❌ Payment required error:")
            print(f"   Message: {e}")
            print(f"   Code: {e.code}")
            print("💡 Tip: Add a payment transaction to continue")
        except AIGENError as e:
            print("❌ General AIGEN error:")
            print(f"   Message: {e}")
            print(f"   Code: {e.code}")
            print(f"   Details: {e.details}")
        print()
        
        print_section("Example 5: Wallet Authentication Demo")
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
        
        print_section("Final Quota Check")
        final_quota = client.check_quota()
        print(f"📊 Final quota: {final_quota.used}/{final_quota.limit}")
        print(f"📈 Used {final_quota.used - quota.used} tokens during this session")
        
    except Exception as e:
        print(f"❌ Fatal error: {e}")
        if isinstance(e, AIGENError):
            print(f"🔍 Error code: {e.code}")
            print(f"📊 Error details: {e.details}")
        
    finally:
        print("\n👋 Disconnecting...")
        client.disconnect()
        print("✅ Disconnected successfully!")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n\n👋 Interrupted by user")
        sys.exit(0)
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}")
        sys.exit(1)
