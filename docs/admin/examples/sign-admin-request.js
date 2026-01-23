// CEO Signature Examples for AIGEN Admin Dashboard
// This script demonstrates how to generate CEO signatures for various admin operations
//
// IMPORTANT: Never share your CEO private key. Use hardware wallets for production.

const ethers = require('ethers');

// CEO private key (replace with your actual CEO private key)
const CEO_PRIVATE_KEY = 'your_ceo_private_key_here';
const NETWORK_MAGIC = 0x41494745; // AIGE in hex

async function signAdminHealthRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const message = `admin_health:${NETWORK_MAGIC}:${timestamp}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Admin Health Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { signature, timestamp });
}

async function signGetMetricsRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const message = `admin_metrics:${NETWORK_MAGIC}:${timestamp}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Get Metrics Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { 
        signature, 
        timestamp,
        includeAi: true,
        includeNetwork: true,
        includeBlockchain: true
    });
}

async function signInitNewModelRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const modelId = 'gpt-4-turbo';
    const message = `admin_initNewModel:${NETWORK_MAGIC}:${timestamp}:modelId=${modelId}`;
    const signature = await wallet.signMessage(message);
    
    const request = {
        modelId: modelId,
        name: 'GPT-4 Turbo',
        version: '1.0.0',
        totalSize: 10737418240, // 10GB
        shardCount: 8,
        verificationHashes: [
            '0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef',
            '0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890'
        ],
        isCoreModel: true,
        minimumTier: 1,
        isExperimental: false,
        signature: signature,
        timestamp: timestamp
    };
    
    console.log('=== Initialize New Model Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', request);
}

async function signApproveModelUpgradeRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const proposalId = 'proposal-123';
    const message = `admin_approveModelUpgrade:${NETWORK_MAGIC}:${timestamp}:proposalId=${proposalId}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Approve Model Upgrade Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { proposalId, signature, timestamp });
}

async function signRejectUpgradeRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const proposalId = 'proposal-123';
    const reason = 'Security vulnerability found in new version';
    const message = `admin_rejectUpgrade:${NETWORK_MAGIC}:${timestamp}:proposalId=${proposalId}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Reject Upgrade Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { proposalId, reason, signature, timestamp });
}

async function signSubmitGovVoteRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const proposalId = 'gov-proposal-456';
    const vote = 'approve';
    const comment = 'This proposal aligns with network goals';
    const message = `admin_submitGovVote:${NETWORK_MAGIC}:${timestamp}:proposalId=${proposalId}:vote=${vote}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Submit Governance Vote Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { proposalId, vote, comment, signature, timestamp });
}

async function signApproveSIPRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const proposalId = 'sip-789';
    const message = `admin_approveSIP:${NETWORK_MAGIC}:${timestamp}:proposalId=${proposalId}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Approve SIP Request ===');
    console.log('Message:', message);
    console.log('Signature:', signature);
    console.log('Request:', { proposalId, signature });
}

async function signVetoSIPRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const proposalId = 'sip-789';
    const message = `admin_vetoSIP:${NETWORK_MAGIC}:${timestamp}:proposalId=${proposalId}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Veto SIP Request ===');
    console.log('Message:', message);
    console.log('Signature:', signature);
    console.log('Request:', { proposalId, signature });
}

async function signShutdownRequest() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const reason = 'Critical security vulnerability detected';
    const nonce = 1;
    const message = `admin_shutdown:${timestamp}:${reason}:${nonce}`;
    const signature = await wallet.signMessage(message);
    
    console.log('=== Emergency Shutdown Request ===');
    console.log('Message:', message);
    console.log('Timestamp:', timestamp);
    console.log('Signature:', signature);
    console.log('Request:', { timestamp, reason, nonce, signature });
}

async function signTypedDataExample() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    
    const domain = {
        name: 'AIGEN Admin',
        version: '1',
        chainId: 1,
        verifyingContract: '0x0000000000000000000000000000000000000000'
    };
    
    const types = {
        AdminRequest: [
            { name: 'action', type: 'string' },
            { name: 'timestamp', type: 'uint256' },
            { name: 'networkMagic', type: 'uint256' }
        ]
    };
    
    const value = {
        action: 'health',
        timestamp: Math.floor(Date.now() / 1000),
        networkMagic: NETWORK_MAGIC
    };
    
    const signature = await wallet.signTypedData(domain, types, value);
    
    console.log('=== Typed Data Signature Example ===');
    console.log('Domain:', domain);
    console.log('Types:', types);
    console.log('Value:', value);
    console.log('Signature:', signature);
}

async function verifySignatureExample() {
    const wallet = new ethers.Wallet(CEO_PRIVATE_KEY);
    const timestamp = Math.floor(Date.now() / 1000);
    const message = `admin_health:${NETWORK_MAGIC}:${timestamp}`;
    const signature = await wallet.signMessage(message);
    
    // Recover signer address from signature
    const recoveredAddress = ethers.verifyMessage(message, signature);
    const originalAddress = wallet.address;
    
    console.log('=== Signature Verification Example ===');
    console.log('Message:', message);
    console.log('Signature:', signature);
    console.log('Original Address:', originalAddress);
    console.log('Recovered Address:', recoveredAddress);
    console.log('Signature Valid:', recoveredAddress.toLowerCase() === originalAddress.toLowerCase());
}

async function runAllExamples() {
    console.log('\n========================================');
    console.log('AIGEN CEO Signature Examples');
    console.log('========================================\n');
    
    await signAdminHealthRequest();
    console.log('\n---\n');
    
    await signGetMetricsRequest();
    console.log('\n---\n');
    
    await signInitNewModelRequest();
    console.log('\n---\n');
    
    await signApproveModelUpgradeRequest();
    console.log('\n---\n');
    
    await signRejectUpgradeRequest();
    console.log('\n---\n');
    
    await signSubmitGovVoteRequest();
    console.log('\n---\n');
    
    await signApproveSIPRequest();
    console.log('\n---\n');
    
    await signVetoSIPRequest();
    console.log('\n---\n');
    
    await signShutdownRequest();
    console.log('\n---\n');
    
    await signTypedDataExample();
    console.log('\n---\n');
    
    await verifySignatureExample();
    
    console.log('\n========================================');
    console.log('All examples completed!');
    console.log('========================================\n');
}

// Run all examples if executed directly
if (require.main === module) {
    runAllExamples().catch(console.error);
}

module.exports = {
    signAdminHealthRequest,
    signGetMetricsRequest,
    signInitNewModelRequest,
    signApproveModelUpgradeRequest,
    signRejectUpgradeRequest,
    signSubmitGovVoteRequest,
    signApproveSIPRequest,
    signVetoSIPRequest,
    signShutdownRequest,
    signTypedDataExample,
    verifySignatureExample
};
