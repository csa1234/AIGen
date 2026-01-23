import os
import requests
import json

# Chat completion example for AIGEN.
# Usage:
#   python chat_example.py

RPC_URL = os.environ.get("AIGEN_RPC_URL", "http://localhost:9944")


def rpc(method, params=None, request_id=1):
    if params is None:
        params = []

    payload = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    r = requests.post(RPC_URL, json=payload, headers={"content-type": "application/json"}, timeout=30)
    r.raise_for_status()
    data = r.json()

    if "error" in data and data["error"]:
        error_code = data["error"].get("code", -32603)
        error_message = data["error"].get("message", "RPC error")
        raise RuntimeError(f"RPC Error {error_code}: {error_message}")

    return data.get("result")


def chat_completion(messages, model_id="mistral-7b", options=None):
    if options is None:
        options = {}
    
    params = {
        "messages": messages,
        "model_id": model_id,
        "stream": options.get("stream", False),
        "max_tokens": options.get("max_tokens", 1000),
        "temperature": options.get("temperature"),
        "transaction": options.get("transaction")  # For pay-per-use
    }
    
    return rpc("chatCompletion", [params])


class AIGENClient:
    def __init__(self, rpc_url="http://localhost:9944"):
        self.rpc_url = rpc_url
    
    def chat_completion(self, messages, model_id="mistral-7b", **options):
        return chat_completion(messages, model_id, options)


def main():
    print("AIGEN Chat Completion Example")
    print("RPC:", RPC_URL)
    
    try:
        # Example 1: Simple chat completion (requires subscription)
        print("\n=== Example 1: Simple Chat ===")
        messages1 = [
            {"role": "user", "content": "Hello! Can you explain what blockchain is in simple terms?"}
        ]
        
        response1 = chat_completion(messages1)
        print("Response:", response1["choices"][0]["message"]["content"])
        print("Usage:", response1["usage"])
        print("Ad injected:", response1["ad_injected"])
        
        # Example 2: Multi-turn conversation
        print("\n=== Example 2: Multi-turn Conversation ===")
        messages2 = [
            {"role": "system", "content": "You are a helpful assistant that explains complex topics simply."},
            {"role": "user", "content": "What is proof of work?"},
            {"role": "assistant", "content": "Proof of work is like a puzzle that computers solve to validate transactions."},
            {"role": "user", "content": "Can you give me a real-world analogy?"}
        ]
        
        response2 = chat_completion(messages2, "mistral-7b", {"max_tokens": 200})
        print("Response:", response2["choices"][0]["message"]["content"])
        
        # Example 3: Code generation
        print("\n=== Example 3: Code Generation ===")
        messages3 = [
            {"role": "user", "content": "Write a simple Python function to calculate fibonacci numbers"}
        ]
        
        response3 = chat_completion(messages3, "codegen-16b")
        print("Generated code:\n", response3["choices"][0]["message"]["content"])
        
        # Example 4: Using the client class
        print("\n=== Example 4: Using Client Class ===")
        client = AIGENClient(RPC_URL)
        
        messages4 = [
            {"role": "user", "content": "What are the benefits of decentralized systems?"}
        ]
        
        response4 = client.chat_completion(messages4)
        print("Client response:", response4["choices"][0]["message"]["content"])
        
        # Example 5: Pay-per-use (requires transaction)
        print("\n=== Example 5: Pay-per-use ===")
        print("For pay-per-use, you need to create a signed transaction with payment.")
        print("See docs/API.md for detailed transaction format.")
        
    except RuntimeError as e:
        print(f"Error: {e}")
        if "RPC Error 2007" in str(e):
            print("Payment required - you need an active subscription or pay-per-use transaction")
        elif "RPC Error 2003" in str(e):
            print("Insufficient tier - upgrade your subscription for this model")
        elif "RPC Error 2004" in str(e):
            print("Quota exceeded - wait for reset or upgrade subscription")


if __name__ == "__main__":
    main()