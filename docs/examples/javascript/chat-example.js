import axios from "axios";

// Chat completion example for AIGEN.
// Usage:
//   node chat-example.js

const RPC_URL = process.env.AIGEN_RPC_URL || "http://localhost:9944";

async function rpc(method, params = [], id = 1) {
  const payload = { jsonrpc: "2.0", id, method, params };
  const { data } = await axios.post(RPC_URL, payload, {
    headers: { "content-type": "application/json" }
  });

  if (data && data.error) {
    const err = new Error(data.error.message || "RPC error");
    err.code = data.error.code;
    err.data = data.error.data;
    throw err;
  }

  return data.result;
}

async function chatCompletion(messages, modelId = "mistral-7b", options = {}) {
  const params = {
    messages,
    model_id: modelId,
    stream: options.stream || false,
    max_tokens: options.maxTokens || 1000,
    temperature: options.temperature,
    transaction: options.transaction // For pay-per-use
  };

  return await rpc("chatCompletion", [params]);
}

async function main() {
  console.log("AIGEN Chat Completion Example");
  console.log("RPC:", RPC_URL);

  try {
    // Example 1: Simple chat completion (requires subscription)
    console.log("\n=== Example 1: Simple Chat ===");
    const messages1 = [
      { role: "user", content: "Hello! Can you explain what blockchain is in simple terms?" }
    ];
    
    const response1 = await chatCompletion(messages1);
    console.log("Response:", response1.choices[0].message.content);
    console.log("Usage:", response1.usage);
    console.log("Ad injected:", response1.ad_injected);

    // Example 2: Multi-turn conversation
    console.log("\n=== Example 2: Multi-turn Conversation ===");
    const messages2 = [
      { role: "system", content: "You are a helpful assistant that explains complex topics simply." },
      { role: "user", content: "What is proof of work?" },
      { role: "assistant", content: "Proof of work is like a puzzle that computers solve to validate transactions." },
      { role: "user", content: "Can you give me a real-world analogy?" }
    ];
    
    const response2 = await chatCompletion(messages2, "mistral-7b", { maxTokens: 200 });
    console.log("Response:", response2.choices[0].message.content);

    // Example 3: Code generation
    console.log("\n=== Example 3: Code Generation ===");
    const messages3 = [
      { role: "user", content: "Write a simple Python function to calculate fibonacci numbers" }
    ];
    
    const response3 = await chatCompletion(messages3, "codegen-16b");
    console.log("Generated code:\n", response3.choices[0].message.content);

    // Example 4: Pay-per-use (requires transaction)
    console.log("\n=== Example 4: Pay-per-use ===");
    console.log("For pay-per-use, you need to create a signed transaction with payment.");
    console.log("See docs/API.md for detailed transaction format.");

  } catch (error) {
    console.error("Error:", error.message);
    if (error.code === 2007) {
      console.log("Payment required - you need an active subscription or pay-per-use transaction");
    } else if (error.code === 2003) {
      console.log("Insufficient tier - upgrade your subscription for this model");
    } else if (error.code === 2004) {
      console.log("Quota exceeded - wait for reset or upgrade subscription");
    }
  }
}

// Export for use as a module
export async function chatCompletion(messages, modelId = "mistral-7b", options = {}) {
  return await chatCompletion(messages, modelId, options);
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error("Error:", e);
    process.exitCode = 1;
  });
}