# Admin Dashboard Web3 Integration

## Overview
- Optional EIP-1193 provider architecture (EVM wallets) controlled via configuration for environments that need user identity bridging. For AIGEN’s non-EVM core, admin flows do not require EVM wallets.
- Global Web3Context maintains connection state and rebroadcasts wallet events.
- Web3Manager coordinates provider detection, connection, network switching, and disconnection.

## Files
- web3/base-provider.js: abstract base with common APIs and event bus.
- web3/providers/*: concrete connectors (metamask, coinbase, trust, brave, phantom, walletconnect).
- web3/web3-context.js: global state { connected, address, chainId, network, providerName }.
- web3/web3-manager.js: detection, connection lifecycle, ensureNetwork, addChain.
- ui/wallet-ui.js: connect button behavior, wallet selection modal, disconnect handling.

## Usage
- Index.html loads all scripts; wallet button opens selection modal.
- Web3Manager auto-picks the first available provider when no preference is supplied; WalletConnect is used as a fallback.
- Subscribe to context changes: `__web3Context.subscribe(fn)`.

## Network Support
- Preconfigured chains: Ethereum(1), Polygon(137), BNB(56), Arbitrum(42161), Optimism(10), Sepolia(11155111).
- `ensureNetwork(chainId)` attempts `wallet_switchEthereumChain` and optionally `wallet_addEthereumChain` using `addChainConfigs`.
- Environment defaults:
  - Localhost → Sepolia (11155111)
  - Production domains → Ethereum mainnet (1)
  - Dev local RPC → Hardhat (31337) available via `addChainConfigs`
  - Override required chain id via `localStorage.web3RequiredChainId`.

## Metrics Gating
- Health & Metrics require CEO ed25519 signature (tweetnacl). The dashboard checks `localStorage.ceoPrivateKey` and skips signed calls when absent, showing guidance.
- Retry logic with exponential backoff for RPC calls to improve resilience.

## Wallet Support Matrix
- MetaMask/Coinbase/Trust/Brave: Injected EIP-1193; enabled only if `web3.enableEvm` is true.
- WalletConnect: QR flow via v1 UMD; enabled only if `web3.enableEvm` is true.
- Phantom:
  - Uses EVM when `window.ethereum.isPhantom` is present.
  - If Phantom shows “Solana account can’t use Ethereum”, switch to an EVM-compatible account or enable Phantom EVM.
  - Automatic network injection: if chain is missing (code 4902), the dashboard adds the chain config and switches automatically when possible.

## Configuration
- WalletConnect RPC defaults:
  - 1: https://cloudflare-eth.com
  - 137: https://polygon-rpc.com
  - 56: https://bsc-dataseed.binance.org
- Override by editing `web3-manager.js` options.
- Required chain choice can be customized per environment via `localStorage.web3RequiredChainId` and the `addChainConfigs` map.
 - Preferred production setup: provide a `config.json` or a `window.__AIGEN_CONFIG` object via the hosting server:
   ```json
   {
     "web3": {
      "enableEvm": false,
       "requiredChainId": 1,
       "walletConnectRpc": { "1": "https://your-mainnet-rpc" }
     },
     "chains": {
       "polygonRpc": "https://your-polygon-rpc",
       "bscRpc": "https://your-bsc-rpc"
     }
   }
   ```
 - The dashboard loads configuration from [config.js](file:///d:/Code/AIGEN/docs/admin/config.js), using `config.json` if present or `window.__AIGEN_CONFIG`.

## Security Notes
- No wallet secrets stored; operations use provider requests.
- CEO private key remains client-side for compatibility with current RPC; use strong operational controls in production.

## Testing
- Use the live dashboard at [index.html](file:///d:/Code/AIGEN/docs/admin/index.html) with real wallets and production RPCs.
- Validate flows against your production chain: connect, account switch, network switch, disconnect.
- Verify Health & Metrics after configuring the CEO key in Settings.

## Maintenance
- Add new providers by extending BaseProvider and registering in `web3-manager.js`.
