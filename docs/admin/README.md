# AIGEN Admin Dashboard

A comprehensive web-based admin interface for managing the AIGEN blockchain network, AI models, health monitoring, and governance operations.

## Overview

The admin dashboard provides a centralized interface for:
- Blockchain Explorer: View blocks, transactions, and search by hash
- AI Models: Initialize models, approve/reject upgrades, and manage model registry
- Health & Metrics: Real-time monitoring with interactive charts
- Governance: Vote on proposals, manage SIPs, and control emergency shutdown

## Access Requirements

### Prerequisites
- **CEO Wallet Required**: All admin operations require CEO signature verification
- **Web3 Wallet**: MetaMask (recommended), WalletConnect, or compatible wallet
- **AIGEN Node**: Running node with RPC enabled on port 9944 (default)
- **Modern Browser**: Chrome, Firefox, Safari, or Edge with JavaScript enabled

### CEO Wallet Setup

1. Install MetaMask browser extension
2. Import CEO private key (generated during genesis keypair creation):
   ```bash
   # CEO keypair is generated in scripts/generate_ceo_keypair.rs
   # Keep this private key secure and never share it
   ```
3. Connect wallet to dashboard
4. Verify CEO address in dashboard settings

## Setup Instructions

### Local Development

#### Using Python HTTP Server
```bash
cd docs/admin
python -m http.server 8081
# Open http://localhost:8081 in browser
```

#### Using Node.js HTTP Server
```bash
cd docs/admin
npx http-server -p 8081
# Open http://localhost:8081 in browser
```

#### Using Docker
```bash
# Start network with static file server
docker-compose up -d

# Access admin dashboard: http://localhost:8080/admin
```

### Configuration

1. Open the dashboard in your browser
2. Click the settings icon (gear) in the header
3. Configure:
   - **RPC URL**: AIGEN node RPC endpoint (default: `http://localhost:9944`)
   - **WebSocket URL**: AIGEN node WebSocket endpoint (default: `ws://localhost:9944`)
   - **CEO Wallet Address**: Your CEO wallet address for signature verification
   - **Theme**: Dark mode (default) or light mode

## Features

### Blockchain Explorer

- **View Latest Blocks**: Paginated list of recent blocks with height, hash, timestamp, and transaction count
- **View Transactions**: Detailed transaction information for each block
- **Search**: Search for blocks or transactions by hash
- **Real-time Updates**: Auto-refreshes every 30 seconds (configurable)

### AI Models Management

- **Model Registry**: View all registered AI models with status indicators
- **Initialize New Model**: Create new AI models with:
  - Model ID, name, and version
  - Total size and shard count
  - Verification hashes
  - Core model flag and minimum tier
  - Experimental flag
- **Model Upgrades**: Approve or reject model upgrade proposals
- **Load Models**: Trigger model loading on nodes

### Health & Metrics

- **Node Health**:
  - RPC status
  - Peer count
  - Block synchronization status
- **AI Health**:
  - Inference service status
  - Model cache status
  - Batch processing status
- **Blockchain Health**:
  - Chain height
  - Finality status
  - Consensus status
- **Interactive Charts**:
  - AI inference time trend
  - Cache hit rate
  - Network bandwidth
  - Blockchain growth
- **Auto-refresh**: Configurable intervals (10s, 30s, 60s, manual)

### Governance

- **Governance Proposals**: View and vote on governance proposals
- **Vote Submission**: Submit approve/reject/abstain votes with comments
- **SIP Management**: Approve or veto System Improvement Proposals (SIPs)
- **Emergency Shutdown**: Trigger network shutdown with reason and nonce

## Security

### CEO Signature Verification

All admin operations require CEO signature verification:
- `getHealth`: Fetch node health metrics
- `getMetrics`: Fetch detailed metrics
- `initNewModel`: Initialize new AI models
- `approveModelUpgrade`: Approve model upgrades
- `rejectUpgrade`: Reject model upgrades
- `submitGovVote`: Submit governance votes
- `approveSIP`: Approve SIP proposals
- `vetoSIP`: Veto SIP proposals
- `submitShutdown`: Trigger emergency shutdown

### Signature Format

CEO signatures follow the format:
```
admin_{action}:{network_magic}:{timestamp}:{key1}={value1}:{key2}={value2}...
```

Example for health check:
```
admin_health:1094795573:1706054400
```

### Wallet Security Best Practices

1. **Never share CEO private key**
2. **Use hardware wallet** for production environments
3. **Verify CEO address** before signing any request
4. **Use secure connection** (HTTPS) when accessing dashboard remotely
5. **Enable 2FA** on MetaMask if available
6. **Regular backups** of wallet seed phrase

## Common Admin Tasks

### Initialize a New AI Model

1. Navigate to **AI Models** tab
2. Click **Initialize New Model** button
3. Fill in model details:
   - Model ID: Unique identifier (e.g., `gpt-4-turbo`)
   - Name: Human-readable name
   - Version: Semantic version (e.g., `1.0.0`)
   - Total Size: Model size in bytes
   - Shard Count: Number of model shards
   - Verification Hashes: Comma-separated hash list
   - Core Model: Mark as core model
   - Minimum Tier: Required node tier (1-3)
   - Experimental: Mark as experimental
4. Click **Initialize Model**
5. Sign the request with CEO wallet
6. Model appears in registry after approval

### Approve a Model Upgrade

1. Navigate to **AI Models** tab
2. Scroll to **Model Upgrade Proposals** section
3. Find the proposal to approve
4. Click **Approve** button
5. Sign the request with CEO wallet
6. Proposal status updates to "approved"

### Monitor Node Health

1. Navigate to **Health & Metrics** tab
2. View health cards for:
   - Node Health (RPC, peers, sync)
   - AI Health (inference, cache, batch)
   - Blockchain Health (height, finality, consensus)
3. View interactive charts for trends
4. Adjust refresh interval as needed

### Submit a Governance Vote

1. Navigate to **Governance** tab
2. Scroll to **Submit Vote** section
3. Enter:
   - Proposal ID
   - Vote type (approve/reject/abstain)
   - Comment (optional)
4. Click **Submit Vote**
5. Sign the request with CEO wallet

### Trigger Emergency Shutdown

⚠️ **WARNING**: This action cannot be undone

1. Navigate to **Governance** tab
2. Scroll to **Emergency Shutdown** section
3. Enter:
   - Reason for shutdown
   - Nonce (unique identifier)
4. Click **Submit Shutdown**
5. Confirm the action
6. Sign the request with CEO wallet

## Troubleshooting

### Connection Issues

**Problem**: Dashboard cannot connect to AIGEN node

**Solutions**:
- Verify node is running: `docker ps` or check node logs
- Check RPC URL in settings matches node endpoint
- Ensure port 9944 is accessible (check firewall)
- Try WebSocket connection instead of HTTP

**Problem**: WebSocket connection drops frequently

**Solutions**:
- Check node logs for WebSocket errors
- Verify network stability
- Increase reconnection delay in browser console
- Check if node is under heavy load

### Wallet Not Detected

**Problem**: "No Web3 wallet detected" error

**Solutions**:
- Install MetaMask browser extension
- Refresh page after installation
- Check browser compatibility
- Try different wallet (WalletConnect, Coinbase Wallet)

**Problem**: Wallet connected but CEO verification fails

**Solutions**:
- Verify CEO address in settings matches connected wallet
- Ensure correct CEO wallet is imported in MetaMask
- Check wallet is on correct network (if applicable)
- Re-connect wallet and try again

### Signature Verification Failures

**Problem**: "CEO signature verification failed" error

**Solutions**:
- Ensure CEO wallet is connected
- Verify CEO address in settings
- Check timestamp is current (not expired)
- Verify message format matches expected pattern
- Try signing request again

### RPC Errors

**Problem**: "RPC call failed" errors

**Solutions**:
- Check node is running and healthy
- Verify RPC endpoint is correct
- Check node logs for errors
- Ensure node has sufficient resources
- Restart node if necessary

**Problem**: "Unauthorized" errors

**Solutions**:
- Verify CEO wallet is connected
- Check CEO address matches genesis configuration
- Ensure signature is valid and not expired
- Verify network magic is correct

### Chart Rendering Issues

**Problem**: Charts not displaying

**Solutions**:
- Check browser console for errors
- Verify Chart.js CDN is accessible
- Ensure metrics data is being fetched
- Try manual refresh
- Check if ad blockers are blocking CDN requests

## API Reference

### Admin RPC Methods

#### getHealth
```javascript
const { message, timestamp } = rpcClient.formatAdminMessage('health');
const signature = await rpcClient.signMessage(message);
const health = await rpcClient.getHealth(signature, timestamp);
```

#### getMetrics
```javascript
const { message, timestamp } = rpcClient.formatAdminMessage('metrics');
const signature = await rpcClient.signMessage(message);
const metrics = await rpcClient.getMetrics(signature, timestamp, true, true, true);
```

#### initNewModel
```javascript
const request = {
    modelId: 'gpt-4-turbo',
    name: 'GPT-4 Turbo',
    version: '1.0.0',
    totalSize: 10737418240,
    shardCount: 8,
    verificationHashes: ['hash1', 'hash2'],
    isCoreModel: true,
    minimumTier: 1,
    isExperimental: false
};
const { message, timestamp } = rpcClient.formatAdminMessage('initNewModel', { modelId: request.modelId });
const signature = await rpcClient.signMessage(message);
await rpcClient.initNewModel(request, signature);
```

#### approveModelUpgrade
```javascript
const { message, timestamp } = rpcClient.formatAdminMessage('approveModelUpgrade', { proposalId });
const signature = await rpcClient.signMessage(message);
await rpcClient.approveModelUpgrade(proposalId, signature, timestamp);
```

#### submitGovVote
```javascript
const { message, timestamp } = rpcClient.formatAdminMessage('submitGovVote', { proposalId, vote });
const signature = await rpcClient.signMessage(message);
await rpcClient.submitGovVote(proposalId, vote, comment, signature, timestamp);
```

#### submitShutdown
```javascript
const timestamp = Math.floor(Date.now() / 1000);
const message = `admin_shutdown:${timestamp}:${reason}:${nonce}`;
const signature = await rpcClient.signMessage(message);
await rpcClient.submitShutdown(timestamp, reason, nonce, signature);
```

## Development

### File Structure

```
docs/admin/
├── index.html              # Main dashboard HTML
├── app.js                  # Dashboard application logic
├── admin-client.js         # Admin RPC client
├── wallet.js               # Web3 wallet integration
├── charts.js               # Chart.js visualizations
├── style.css               # Dashboard styles
├── README.md               # This file
└── examples/
    └── sign-admin-request.js  # CEO signature examples
```

### Browser Compatibility

- Chrome/Edge: Full support
- Firefox: Full support
- Safari: Full support (may require Web3 wallet compatibility)
- Mobile browsers: Limited support (use desktop for best experience)

### Dependencies

- Chart.js 4.4.0 (CDN)
- Ethers.js 6.9.0 (CDN)
- Marked.js 11.1.1 (CDN)

No build step required - all dependencies loaded via CDN.

## License

See main project LICENSE file.

## Support

For issues, questions, or contributions:
- GitHub Issues: https://github.com/aigen-blockchain/aigen/issues
- Documentation: See main project docs
- Community: Join our Discord server
