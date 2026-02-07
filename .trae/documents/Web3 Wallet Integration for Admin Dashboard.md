## Scope
- Implement modular Web3 wallet integration for the admin dashboard at http://localhost:8081.
- Support MetaMask, Coinbase Wallet, Trust Wallet, Brave Wallet (EVM), WalletConnect (multi-wallet), and Phantom (Solana beta detection with EVM fallback).
- Add connection manager, global Web3 context, UI for connect/select/disconnect, robust error handling, network validation, and event listeners.
- Gate metrics fetching behind wallet/CEO readiness with retries and loading states. Provide tests and documentation.

## Architecture
- Provider abstraction (EIP-1193 compatible): BaseProvider with connect, disconnect, getAccounts, getChainId, switchChain, addChain, on/off events.
- Concrete providers:
  - MetaMaskProvider (window.ethereum.isMetaMask)
  - CoinbaseProvider (window.ethereum.isCoinbaseWallet; optional SDK lazy-load if available)
  - TrustProvider (window.ethereum.isTrust)
  - BraveProvider (window.ethereum.isBraveWallet)
  - WalletConnectProvider (lazy-load WalletConnect v2 via CDN; creates EIP-1193 compatible provider)
  - PhantomProvider (detect window.phantom || window.solana; if EVM available, use; else mark unsupported for EVM flows)
- Web3Manager: detection, provider selection, automatic fallback, connection lifecycle, and network validation.
- Web3Context: global store (singleton) with subscribe/unsubscribe, state: { connected, address, chainId, network, providerName, accounts, error }.

## Files to Add
- docs/admin/web3/base-provider.js
- docs/admin/web3/providers/metamask.js
- docs/admin/web3/providers/coinbase.js
- docs/admin/web3/providers/trust.js
- docs/admin/web3/providers/brave.js
- docs/admin/web3/providers/walletconnect.js (lazy CDN loader)
- docs/admin/web3/providers/phantom.js (Solana detection; EVM fallback)
- docs/admin/web3/web3-manager.js
- docs/admin/web3/web3-context.js
- docs/admin/ui/wallet-modal.html (template) and docs/admin/ui/wallet-ui.js (Connect button, modal, badge, disconnect)
- docs/admin/tests/wallet-integration.html (manual harness with mock providers)
- docs/admin/WEB3.md (architecture & config)

## Changes to Existing Files
- index.html:
  - Include new scripts: web3-context.js, web3-manager.js, provider modules, wallet-ui.js; optional CDN for WalletConnect/Web3Modal if selected.
  - Add wallet selection modal container and connected badge; keep existing settings modal.
- style.css: styles for wallet modal, badges, network indicator, loading states.
- wallet.js: refactor into a thin wrapper that delegates to Web3Manager (preserve current API for backwards compatibility: connect, getAddress, signMessage → use context provider where applicable).
- app.js:
  - Initialize Web3Manager and Web3Context before RPC connects.
  - Gate health/metrics calls: require CEO private key present; if absent, show actionable message; otherwise perform signed calls.
  - Add retry logic: exponential backoff (e.g., 3 attempts at 1s, 2s, 4s) for getHealth/getMetrics; show loading placeholders.
  - Update walletButton to open wallet modal; on connect, update UI with truncated address and network indicator; on disconnect, revert.
  - Subscribe to context events (accountsChanged, chainChanged, disconnect) and revalidate network + refresh metrics.

## Network Support & Validation
- Supported EVM networks mapping (id → name): Ethereum(1), Polygon(137), BNB(56), Arbitrum(42161), Optimism(10), Sepolia(11155111) for testing.
- Validate required network via configuration (default any EVM). If mismatch, prompt to switch using wallet_switchEthereumChain (add chain if 4902).
- Chain configs for addChain: chainId, chainName, rpcUrls, nativeCurrency, blockExplorerUrls.

## Error Handling & UX
- Uniform error codes/messages for: user rejection (4001), chain not added (4902), provider not found, connection timeouts.
- Non-blocking toasts with statuses; modal shows guidance if wallet missing.
- Fallbacks: if no wallet detected, continue with read-only RPC functionality; wallet-only actions show disabled tooltips.

## Event Listeners
- Implement on/off handlers for accountsChanged, chainChanged, disconnect across providers.
- Web3Context rebroadcasts events so app.js only subscribes once.

## Metrics Initialization Fix
- Before calling getHealth/getMetrics, ensure:
  - Web3Context.initialized === true
  - CEO private key present (localStorage.ceoPrivateKey) and usable; otherwise show “Configure CEO key in Settings” and skip signed calls.
- Retry with backoff; on success, update charts; on failure, surface error and keep ‘Checking…’ until next refresh.

## Testing
- Manual harness docs/admin/tests/wallet-integration.html allowing to inject mock EIP-1193 providers to simulate flows.
- Scenarios:
  - Connect/Disconnect each supported wallet
  - Account switch, network switch
  - WalletConnect QR flow
  - Error paths: user rejection, chain not added, provider unavailable
  - Metrics gating with/without CEO key
- Optional: lightweight Jasmine test runner via CDN for unit tests of Web3Manager state transitions.

## Documentation
- WEB3.md: provider architecture, wallet support matrix, network config, usage examples, and maintenance notes.
- Update docs/admin/README.md with setup steps and troubleshooting.

## Security Considerations
- Avoid storing wallet secrets; rely on EIP-1193 requests.
- CEO ed25519 key remains localStorage-based per current design; add prominent warnings and recommend hardware/offline management for production.

## Rollout Plan
- Implement files incrementally; wire UI; verify on localhost with MetaMask, then WalletConnect.
- Validate metrics fix; confirm event handling; finalize tests; ship documentation.

## Acceptance Criteria
- Users can pick and connect major wallets; address and network shown.
- App reacts to account/network changes; disconnect resets state.
- Health & Metrics fetch only when CEO key present; retries and clear messaging.
- Tests demonstrate each flow succeeds/fails appropriately.

Please confirm to proceed with implementation changes and wiring in the admin dashboard.