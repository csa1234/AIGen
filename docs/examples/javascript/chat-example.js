import { AIGENClient, AIGENError, QuotaExceededError, InsufficientTierError, PaymentRequiredError } from './aigen-sdk.js';

async function main() {
    console.log("🤖 AIGEN Chat Example with SDK\n");
    
    const client = new AIGENClient('http://localhost:9944');
    
    try {
        console.log("🔗 Connecting to AIGEN network...");
        await client.connect();
        console.log("✅ Connected successfully!\n");
        
        console.log("📊 Checking quota...");
        const quota = await client.checkQuota();
        console.log(`📈 Quota: ${quota.used}/${quota.limit} (Reset: ${quota.reset_date})\n`);
        
        console.log("🏆 Getting tier information...");
        const tierInfo = await client.getTierInfo();
        console.log(`Current tier: ${tierInfo.current_tier}`);
        console.log(`Available tiers: ${tierInfo.available_tiers.join(', ')}\n`);
        
        console.log("🤖 Listing available models...");
        const models = await client.listModels();
        console.log(`Available models (${models.length}):`);
        models.forEach(model => {
            console.log(`  - ${model.name} (${model.id}): ${model.description}`);
        });
        console.log();
        
        console.log("=== Example 1: Simple Chat ===");
        const messages1 = [
            { role: "user", content: "Hello! Can you explain what blockchain is in simple terms?" }
        ];
        
        console.log("💬 User:", messages1[0].content);
        const response1 = await client.chatCompletion(messages1, {
            modelId: "mistral-7b",
            maxTokens: 200,
            temperature: 0.7
        });
        
        console.log("🤖 Assistant:", response1.choices[0].message.content);
        console.log("📊 Usage:", response1.usage);
        console.log("💰 Ad injected:", response1.ad_injected);
        console.log();
        
        console.log("=== Example 2: Multi-turn Conversation ===");
        const messages2 = [
            { role: "system", content: "You are a helpful assistant that explains complex topics simply." },
            { role: "user", content: "What is proof of work?" },
            { role: "assistant", content: "Proof of work is like a puzzle that computers solve to validate transactions." },
            { role: "user", content: "Can you give me a real-world analogy?" }
        ];
        
        console.log("💬 User:", messages2[messages2.length - 1].content);
        const response2 = await client.chatCompletion(messages2, {
            modelId: "mistral-7b",
            maxTokens: 200
        });
        
        console.log("🤖 Assistant:", response2.choices[0].message.content);
        console.log();
        
        console.log("=== Example 3: Code Generation ===");
        const messages3 = [
            { role: "user", content: "Write a simple Python function to calculate fibonacci numbers with error handling" }
        ];
        
        console.log("💬 User:", messages3[0].content);
        const response3 = await client.chatCompletion(messages3, {
            modelId: "codegen-16b",
            maxTokens: 300,
            temperature: 0.3
        });
        
        console.log("🤖 Generated code:");
        console.log("```python");
        console.log(response3.choices[0].message.content);
        console.log("```");
        console.log();
        
        console.log("=== Example 4: Error Handling ===");
        try {
            await client.chatCompletion([
                { role: "user", content: "Test message" }
            ], {
                modelId: "non-existent-model"
            });
        } catch (error) {
            if (error instanceof InsufficientTierError) {
                console.log("❌ Insufficient tier error:", error.message);
            } else if (error instanceof QuotaExceededError) {
                console.log("❌ Quota exceeded error:", error.message);
                console.log("💡 Tip: Wait for quota reset or upgrade your subscription");
            } else if (error instanceof PaymentRequiredError) {
                console.log("❌ Payment required error:", error.message);
                console.log("💡 Tip: Add a payment transaction to continue");
            } else {
                console.log("❌ General error:", error.message);
            }
            console.log("🔍 Error code:", error.code);
            console.log("📊 Error details:", error.details);
        }
        console.log();
        
        console.log("=== Example 5: Wallet Authentication Demo ===");
        try {
            const nacl = await import('tweetnacl');
            
            console.log("🔑 Generating wallet key pair...");
            const keyPair = nacl.sign.keyPair();
            const publicKey = Buffer.from(keyPair.publicKey).toString('hex');
            
            console.log(`📍 Public key: ${publicKey.substring(0, 16)}...`);
            
            const address = client.deriveAddress(keyPair.publicKey);
            console.log(`🏠 Derived address: ${address}`);
            
            const paymentTx = client.createPaymentTransaction(50, { 
                service: "ai-inference",
                model: "mistral-7b"
            });
            console.log("💳 Created payment transaction:", paymentTx);
            
            console.log("✅ Wallet authentication setup complete!");
            console.log("💡 In a real scenario, you would sign the transaction and include it in API calls");
            
        } catch (error) {
            console.log("⚠️  Wallet features require tweetnacl package");
            console.log("📦 Install with: npm install tweetnacl");
        }
        console.log();
        
        console.log("=== Final Quota Check ===");
        const finalQuota = await client.checkQuota();
        console.log(`📊 Final quota: ${finalQuota.used}/${finalQuota.limit}`);
        console.log(`📈 Used ${finalQuota.used - quota.used} tokens during this session`);
        
    } catch (error) {
        console.error("❌ Fatal error:", error.message);
        
        if (error instanceof AIGENError) {
            console.error("🔍 Error code:", error.code);
            console.error("📊 Error details:", error.details);
        }
        
    } finally {
        console.log("\n👋 Disconnecting...");
        client.disconnect();
        console.log("✅ Disconnected successfully!");
    }
}

if (import.meta.url === `file://${process.argv[1]}`) {
    main().catch((e) => {
        console.error("Error:", e);
        process.exitCode = 1;
    });
}