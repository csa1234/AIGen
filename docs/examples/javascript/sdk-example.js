import { AIGENClient, AIGENError, QuotaExceededError, InsufficientTierError, PaymentRequiredError } from './aigen-sdk.js';

async function main() {
    console.log('🚀 AIGEN JavaScript SDK Example\n');
    
    const client = new AIGENClient('http://localhost:9944', 'ws://localhost:9944');
    
    try {
        console.log('1. Connecting to AIGEN network...');
        await client.connect();
        console.log('✅ Connected successfully!\n');
        
        console.log('2. Checking quota...');
        const quota = await client.checkQuota();
        console.log(`📊 Quota: ${quota.used}/${quota.limit} (Reset: ${quota.reset_date})\n`);
        
        console.log('3. Getting tier information...');
        const tierInfo = await client.getTierInfo();
        console.log(`🏆 Current tier: ${tierInfo.current_tier}`);
        console.log(`📋 Available tiers: ${tierInfo.available_tiers.join(', ')}`);
        console.log(`📈 Tier limits:`, tierInfo.tier_limits);
        console.log();
        
        console.log('4. Listing available models...');
        const models = await client.listModels();
        console.log(`🤖 Available models (${models.length}):`);
        models.forEach(model => {
            console.log(`   - ${model.name} (${model.id}): ${model.description}`);
            console.log(`     Tier required: ${model.tier_required}, Max tokens: ${model.max_tokens}`);
        });
        console.log();
        
        console.log('5. Getting specific model info...');
        const modelInfo = await client.getModelInfo('mistral-7b');
        console.log(`🔍 Mistral 7B details:`, modelInfo);
        console.log();
        
        console.log('6. Simple chat completion...');
        const messages = [
            { role: 'user', content: 'Hello! Can you introduce yourself and explain what AIGEN is?' }
        ];
        
        console.log('💬 User:', messages[0].content);
        const response = await client.chatCompletion(messages, {
            modelId: 'mistral-7b',
            maxTokens: 512,
            temperature: 0.7
        });
        
        console.log('🤖 Assistant:', response.choices[0].message.content);
        console.log(`📊 Usage: ${response.usage.total_tokens} tokens\n`);
        
        console.log('7. Streaming chat completion...');
        const streamMessages = [
            { role: 'user', content: 'Write a short poem about blockchain technology in exactly 4 lines.' }
        ];
        
        console.log('💬 User:', streamMessages[0].content);
        console.log('🤖 Assistant (streaming):');
        
        let fullResponse = '';
        await client.streamChatCompletion(
            streamMessages,
            {
                modelId: 'mistral-7b',
                maxTokens: 256,
                temperature: 0.8
            },
            (delta) => {
                if (delta.content) {
                    process.stdout.write(delta.content);
                    fullResponse += delta.content;
                }
            },
            (result) => {
                console.log('\n✅ Stream completed!');
            },
            (error) => {
                console.error('❌ Stream error:', error.message);
            }
        );
        
        console.log('\n');
        
        console.log('8. Demonstrating error handling...');
        try {
            await client.chatCompletion([
                { role: 'user', content: 'Test message' }
            ], {
                modelId: 'non-existent-model'
            });
        } catch (error) {
            if (error instanceof InsufficientTierError) {
                console.log('❌ Insufficient tier error caught:', error.message);
            } else if (error instanceof QuotaExceededError) {
                console.log('❌ Quota exceeded error caught:', error.message);
            } else if (error instanceof PaymentRequiredError) {
                console.log('❌ Payment required error caught:', error.message);
            } else {
                console.log('❌ General error caught:', error.message);
            }
        }
        console.log();
        
        console.log('9. Wallet authentication example...');
        try {
            const nacl = await import('tweetnacl');
            
            const keyPair = nacl.sign.keyPair();
            const publicKey = Buffer.from(keyPair.publicKey).toString('hex');
            const privateKey = keyPair.secretKey;
            
            console.log('🔑 Generated wallet key pair');
            console.log(`📍 Public key: ${publicKey}`);
            
            const address = client.deriveAddress(keyPair.publicKey);
            console.log(`🏠 Derived address: ${address}`);
            
            const paymentTx = client.createPaymentTransaction(100, { service: 'ai-inference' });
            console.log('💳 Created payment transaction:', paymentTx);
            
            const signature = client.signTransaction(paymentTx, privateKey);
            console.log(`✍️ Transaction signature: ${signature.substring(0, 16)}...`);
            
        } catch (error) {
            console.log('⚠️  Wallet features require tweetnacl package');
        }
        console.log();
        
        console.log('10. Batch processing example...');
        try {
            const batchData = [
                { input: 'Translate to French: Hello world' },
                { input: 'Translate to Spanish: Good morning' },
                { input: 'Translate to German: Good evening' }
            ];
            
            console.log('📦 Submitting batch request...');
            const batchJob = await client.submitBatchRequest('normal', 'mistral-7b', batchData);
            console.log(`🎯 Batch job submitted: ${batchJob.id}`);
            console.log(`📊 Status: ${batchJob.status}`);
            
            console.log('⏳ Checking batch status...');
            const status = await client.getBatchStatus(batchJob.id);
            console.log(`📋 Current status: ${status.status}`);
            
            console.log('📋 Listing user jobs...');
            const jobs = await client.listUserJobs();
            console.log(`📊 Total jobs: ${jobs.length}`);
            
        } catch (error) {
            console.log('⚠️  Batch processing not available or error occurred:', error.message);
        }
        console.log();
        
        console.log('11. Final quota check...');
        const finalQuota = await client.checkQuota();
        console.log(`📊 Final quota: ${finalQuota.used}/${finalQuota.limit}`);
        console.log(`📈 Used ${finalQuota.used - quota.used} tokens during this session`);
        
    } catch (error) {
        console.error('❌ Fatal error:', error.message);
        console.error('📋 Error details:', error);
        
        if (error instanceof AIGENError) {
            console.error('🔍 Error code:', error.code);
            console.error('📊 Error details:', error.details);
        }
        
    } finally {
        console.log('\n👋 Disconnecting...');
        client.disconnect();
        console.log('✅ Disconnected successfully!');
    }
}

if (import.meta.url === `file://${process.argv[1]}`) {
    main().catch(console.error);
}