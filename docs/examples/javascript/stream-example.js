import { AIGENClient, AIGENError } from './aigen-sdk.js';

async function main() {
    console.log('🌊 AIGEN Streaming Chat Example\n');
    
    const client = new AIGENClient('http://localhost:9944', 'ws://localhost:9944');
    
    try {
        console.log('🔗 Connecting to AIGEN network...');
        await client.connect();
        console.log('✅ Connected successfully!\n');
        
        console.log('📊 Checking quota...');
        const quota = await client.checkQuota();
        console.log(`📈 Quota: ${quota.used}/${quota.limit}\n`);
        
        const conversation = [
            { role: 'user', content: 'Tell me a short story about a blockchain developer who discovers a magical smart contract.' }
        ];
        
        console.log('💬 User:', conversation[0].content);
        console.log('\n🤖 Assistant (streaming response):\n');
        
        let tokenCount = 0;
        let startTime = Date.now();
        
        await client.streamChatCompletion(
            conversation,
            {
                modelId: 'mistral-7b',
                maxTokens: 1024,
                temperature: 0.8
            },
            (delta) => {
                if (delta.content) {
                    process.stdout.write(delta.content);
                    tokenCount++;
                }
            },
            (result) => {
                const endTime = Date.now();
                const duration = (endTime - startTime) / 1000;
                
                console.log('\n\n✅ Stream completed!');
                console.log(`📊 Tokens: ${tokenCount}`);
                console.log(`⏱️  Duration: ${duration.toFixed(2)}s`);
                console.log(`⚡ Speed: ${(tokenCount / duration).toFixed(1)} tokens/sec`);
                console.log(`🏁 Finish reason: ${result.finish_reason}`);
            },
            (error) => {
                console.error('\n❌ Stream error:', error.message);
            }
        );
        
        console.log('\n' + '='.repeat(60));
        
        const codePrompt = [
            { role: 'user', content: 'Write a simple JavaScript function that calculates the factorial of a number with error handling.' }
        ];
        
        console.log('\n💬 User:', codePrompt[0].content);
        console.log('\n🤖 Assistant (streaming code):\n');
        
        tokenCount = 0;
        startTime = Date.now();
        
        await client.streamChatCompletion(
            codePrompt,
            {
                modelId: 'mistral-7b',
                maxTokens: 512,
                temperature: 0.3
            },
            (delta) => {
                if (delta.content) {
                    process.stdout.write(delta.content);
                    tokenCount++;
                }
            },
            (result) => {
                const endTime = Date.now();
                const duration = (endTime - startTime) / 1000;
                
                console.log('\n\n✅ Code generation completed!');
                console.log(`📊 Tokens: ${tokenCount}`);
                console.log(`⏱️  Duration: ${duration.toFixed(2)}s`);
                console.log(`⚡ Speed: ${(tokenCount / duration).toFixed(1)} tokens/sec`);
            },
            (error) => {
                console.error('\n❌ Stream error:', error.message);
            }
        );
        
        console.log('\n' + '='.repeat(60));
        
        const translationPrompt = [
            { role: 'user', content: 'Translate this to French, Spanish, and German: "The future of AI is decentralized and powered by blockchain technology."' }
        ];
        
        console.log('\n💬 User:', translationPrompt[0].content);
        console.log('\n🤖 Assistant (streaming translations):\n');
        
        tokenCount = 0;
        startTime = Date.now();
        
        await client.streamChatCompletion(
            translationPrompt,
            {
                modelId: 'mistral-7b',
                maxTokens: 768,
                temperature: 0.2
            },
            (delta) => {
                if (delta.content) {
                    process.stdout.write(delta.content);
                    tokenCount++;
                }
            },
            (result) => {
                const endTime = Date.now();
                const duration = (endTime - startTime) / 1000;
                
                console.log('\n\n✅ Translation completed!');
                console.log(`📊 Tokens: ${tokenCount}`);
                console.log(`⏱️  Duration: ${duration.toFixed(2)}s`);
            },
            (error) => {
                console.error('\n❌ Stream error:', error.message);
            }
        );
        
    } catch (error) {
        console.error('❌ Fatal error:', error.message);
        
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