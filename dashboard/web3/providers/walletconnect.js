// Copyright (c) 2025-present Cesar Saguier Antebi
//
// WalletConnect (v1 UMD) provider wrapper with lazy CDN loader.
// Uses Cloudflare Ethereum RPC for mainnet by default; users can override via config.

class WalletConnectLazyProvider extends (window.AigenBaseProvider || class {}) {
  constructor(options = {}) {
    super();
    this.name = 'WalletConnect';
    this._options = options;
    this._provider = null;
  }
  isAvailable() { return true; }
  async _ensureLoaded() {
    if (window.WalletConnectProvider) return;
    await new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdn.jsdelivr.net/npm/@walletconnect/web3-provider@1.8.0/dist/umd/index.min.js';
      script.async = true;
      script.onload = () => resolve();
      script.onerror = (e) => reject(new Error('Failed to load WalletConnect provider'));
      document.head.appendChild(script);
    });
  }
  async connect() {
    await this._ensureLoaded();
    const rpc = this._options.rpc || {
      1: 'https://cloudflare-eth.com',
      137: 'https://polygon-rpc.com',
      56: 'https://bsc-dataseed.binance.org'
    };
    this._provider = new window.WalletConnectProvider({
      rpc,
      qrcode: true
    });
    await this._provider.enable();
    this._attach();
    const accounts = await this.getAccounts();
    const chainId = await this.getChainId();
    return { accounts, chainId };
  }
  async disconnect() {
    try { await this._provider.disconnect(); } catch {}
    this._provider = null;
  }
  async request(args) { return await this._provider.request(args); }
  async getAccounts() {
    return await this._provider.request({ method: 'eth_accounts' });
  }
  async getChainId() {
    const hex = await this._provider.request({ method: 'eth_chainId' });
    return typeof hex === 'string' ? parseInt(hex, 16) : hex;
  }
  async switchChain(chainId) {
    const hex = '0x' + chainId.toString(16);
    await this._provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
  }
  async addChain(cfg) {
    await this._provider.request({ method: 'wallet_addEthereumChain', params: [cfg] });
  }
  _attach() {
    this._provider.on('accountsChanged', (accs) => this._emit('accountsChanged', accs));
    this._provider.on('chainChanged', (id) => {
      const n = typeof id === 'string' ? parseInt(id, 16) : id;
      this._emit('chainChanged', n);
    });
    this._provider.on('disconnect', (e) => this._emit('disconnect', e));
  }
}

if (typeof window !== 'undefined') window.AigenWalletConnectProvider = WalletConnectLazyProvider;
if (typeof module !== 'undefined' && module.exports) module.exports = WalletConnectLazyProvider;
