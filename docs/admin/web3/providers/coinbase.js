// Copyright (c) 2025-present Cesar Saguier Antebi
//
// Coinbase Wallet EIP-1193 provider wrapper

class CoinbaseProvider extends (window.AigenBaseProvider || class {}) {
  constructor() {
    super();
    this.name = 'Coinbase Wallet';
    this.ethereum = typeof window !== 'undefined' ? window.ethereum : null;
  }
  isAvailable() {
    return !!(this.ethereum && this.ethereum.isCoinbaseWallet);
  }
  async connect() {
    if (!this.isAvailable()) throw new Error('Coinbase Wallet not available');
    const accounts = await this.ethereum.request({ method: 'eth_requestAccounts' });
    const chainHex = await this.ethereum.request({ method: 'eth_chainId' });
    this._attachEvents();
    return { accounts, chainId: parseInt(chainHex, 16) };
  }
  async disconnect() {}
  async request(args) { return this.ethereum.request(args); }
  async getAccounts() { return await this.ethereum.request({ method: 'eth_accounts' }) || []; }
  async getChainId() { return parseInt(await this.ethereum.request({ method: 'eth_chainId' }), 16); }
  async switchChain(chainId) {
    const hex = '0x' + chainId.toString(16);
    await this.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
  }
  async addChain(chainConfig) { await this.ethereum.request({ method: 'wallet_addEthereumChain', params: [chainConfig] }); }
  _attachEvents() {
    this._onAccountsChanged = (accs) => this._emit('accountsChanged', accs);
    this._onChainChanged = (hex) => this._emit('chainChanged', parseInt(hex, 16));
    this._onDisconnect = (e) => this._emit('disconnect', e);
    this.ethereum.on('accountsChanged', this._onAccountsChanged);
    this.ethereum.on('chainChanged', this._onChainChanged);
    this.ethereum.on('disconnect', this._onDisconnect);
  }
}

if (typeof window !== 'undefined') window.AigenCoinbaseProvider = CoinbaseProvider;
if (typeof module !== 'undefined' && module.exports) module.exports = CoinbaseProvider;
