#!/usr/bin/env python3
"""
AIGEN Python Streaming Example - Demonstrates real-time streaming chat completion
"""

import asyncio
import time
import sys
from aigen_sdk import AIGENClient, AIGENError


def print_section(title):
    """Print a formatted section header"""
    print(f"\n{'='*60}")
    print(f"🌊 {title}")
    print(f"{'='*60}\n")


def print_stream_stats(token_count, start_time, end_time):
    """Print streaming statistics"""
    duration = end_time - start_time
    print(f"\n✅ Stream completed!")
    print(f"📊 Tokens: {token_count}")
    print(f"⏱️  Duration: {duration:.2f}s")
    if duration > 0:
        print(f"⚡ Speed: {token_count/duration:.1f} tokens/sec")
    print()


async def streaming_chat_demo():
    """Demonstrate streaming chat completion"""
    print("🌊 AIGEN Python Streaming Chat Example\n")
    
    client = AIGENClient('http://localhost:9944', 'ws://localhost:9944')
    
    try:
        print("🔗 Connecting to AIGEN network...")
        await client.connect()
        print("✅ Connected successfully!\n")
        
        print("📊 Checking quota...")
        quota = client.check_quota()
        print(f"📈 Quota: {quota.used}/{quota.limit}\n")
        
        # Example 1: Creative Writing
        print_section("Creative Story Generation")
        conversation = [
            {"role": "user", "content": "Tell me a short story about a blockchain developer who discovers a magical smart contract that grants wishes, but with unexpected consequences."}
        ]
        
        print(f"💬 User: {conversation[0]['content']}")
        print("\n🤖 Assistant (streaming response):\n")
        
        token_count = 0
        start_time = time.time()
        
        def on_chunk(delta):
            nonlocal token_count
            if delta.get('content'):
                print(delta['content'], end='', flush=True)
                token_count += 1
        
        def on_complete(result):
            end_time = time.time()
            print_stream_stats(token_count, start_time, end_time)
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
        
        # Example 2: Code Generation
        print_section("Code Generation with Streaming")
        code_prompt = [
            {"role": "user", "content": "Write a Python function that implements a simple blockchain with proof-of-work consensus. Include proper error handling and documentation."}
        ]
        
        print(f"💬 User: {code_prompt[0]['content']}")
        print("\n🤖 Assistant (streaming code):\n")
        
        token_count = 0
        start_time = time.time()
        
        def on_code_chunk(delta):
            nonlocal token_count
            if delta.get('content'):
                print(delta['content'], end='', flush=True)
                token_count += 1
        
        def on_code_complete(result):
            end_time = time.time()
            print_stream_stats(token_count, start_time, end_time)
        
        await client.stream_chat_completion(
            code_prompt,
            on_chunk=on_code_chunk,
            on_complete=on_code_complete,
            on_error=on_error,
            model_id='codegen-16b',
            max_tokens=800,
            temperature=0.3
        )
        
        # Example 3: Translation Task
        print_section("Multi-language Translation")
        translation_prompt = [
            {"role": "user", "content": "Translate this sentence to French, Spanish, and German: 'The future of artificial intelligence is decentralized and powered by blockchain technology.'"}
        ]
        
        print(f"💬 User: {translation_prompt[0]['content']}")
        print("\n🤖 Assistant (streaming translations):\n")
        
        token_count = 0
        start_time = time.time()
        
        def on_translation_chunk(delta):
            nonlocal token_count
            if delta.get('content'):
                print(delta['content'], end='', flush=True)
                token_count += 1
        
        def on_translation_complete(result):
            end_time = time.time()
            print_stream_stats(token_count, start_time, end_time)
        
        await client.stream_chat_completion(
            translation_prompt,
            on_chunk=on_translation_chunk,
            on_complete=on_translation_complete,
            on_error=on_error,
            model_id='mistral-7b',
            max_tokens=600,
            temperature=0.2
        )
        
        # Example 4: Async Streaming Generator
        print_section("Async Streaming Generator")
        async_prompt = [
            {"role": "user", "content": "Write a haiku about the intersection of artificial intelligence and blockchain technology, focusing on decentralization and trust."}
        ]
        
        print(f"💬 User: {async_prompt[0]['content']}")
        print("\n🤖 Assistant (async streaming):\n")
        
        token_count = 0
        start_time = time.time()
        
        try:
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
            print_stream_stats(token_count, start_time, end_time)
            
        except Exception as e:
            print(f"\n❌ Async streaming error: {e}")
        
        # Example 5: Error Handling in Streaming
        print_section("Error Handling Demo")
        error_prompt = [
            {"role": "user", "content": "This should work fine, but let's test error handling."}
        ]
        
        print(f"💬 User: {error_prompt[0]['content']}")
        print("\n🤖 Assistant (testing error handling):\n")
        
        def error_on_chunk(delta):
            if delta.get('content'):
                print(delta['content'], end='', flush=True)
        
        def error_on_complete(result):
            print("\n✅ Stream completed successfully!")
        
        def error_on_error(error):
            print(f"\n❌ Stream error handled: {error}")
            if isinstance(error, AIGENError):
                print(f"🔍 Error code: {error.code}")
        
        await client.stream_chat_completion(
            error_prompt,
            on_chunk=error_on_chunk,
            on_complete=error_on_complete,
            on_error=error_on_error,
            model_id='mistral-7b',
            max_tokens=200,
            temperature=0.5
        )
        
        # Final quota check
        print_section("Final Quota Check")
        final_quota = client.check_quota()
        print(f"📊 Final quota: {final_quota.used}/{final_quota.limit}")
        print(f"📈 Total tokens used in this session: {final_quota.used - quota.used}")
        
    except Exception as e:
        print(f"❌ Fatal error: {e}")
        if isinstance(e, AIGENError):
            print(f"🔍 Error code: {e.code}")
            print(f"📊 Error details: {e.details}")
        
    finally:
        print("\n👋 Disconnecting...")
        client.disconnect()
        print("✅ Disconnected successfully!")


async def performance_comparison():
    """Compare streaming vs non-streaming performance"""
    print_section("Performance Comparison: Streaming vs Non-Streaming")
    
    client = AIGENClient('http://localhost:9944')
    
    try:
        await client.connect()
        
        prompt = [{"role": "user", "content": "Explain quantum computing in simple terms for a 10-year-old."}]
        
        # Non-streaming
        print("⏱️  Testing non-streaming...")
        start_time = time.time()
        response = client.chat_completion(prompt, model_id='mistral-7b', max_tokens=300)
        non_streaming_time = time.time() - start_time
        non_streaming_length = len(response.choices[0]['message']['content'])
        
        print(f"✅ Non-streaming completed in {non_streaming_time:.2f}s")
        print(f"📊 Response length: {non_streaming_length} characters")
        
        # Streaming
        print("\n⏱️  Testing streaming...")
        streaming_content = ""
        start_time = time.time()
        streaming_start = start_time
        
        def perf_on_chunk(delta):
            nonlocal streaming_content
            if delta.get('content'):
                streaming_content += delta['content']
        
        def perf_on_complete(result):
            nonlocal streaming_start
            streaming_time = time.time() - streaming_start
            streaming_length = len(streaming_content)
            print(f"✅ Streaming completed in {streaming_time:.2f}s")
            print(f"📊 Response length: {streaming_length} characters")
            print(f"⚡ Time to first token: {time.time() - start_time:.2f}s")
        
        await client.stream_chat_completion(
            prompt,
            on_chunk=perf_on_chunk,
            on_complete=perf_on_complete,
            model_id='mistral-7b',
            max_tokens=300,
            temperature=0.7
        )
        
        # Small delay to ensure streaming completes
        await asyncio.sleep(0.5)
        
    except Exception as e:
        print(f"❌ Performance comparison error: {e}")
        
    finally:
        client.disconnect()


async def main():
    """Main function"""
    try:
        await streaming_chat_demo()
        await performance_comparison()
        
    except KeyboardInterrupt:
        print("\n\n👋 Interrupted by user")
        sys.exit(0)
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())