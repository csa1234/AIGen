// Copyright (c) 2025-present Cesar Saguier Antebi
//
// MetaMask EIP-1193 provider wrapper

class MetaMaskProvider extends (window.AigenBaseProvider || class {}) {
  constructor() {
    super();
    this.name = 'MetaMask';
    this.ethereum = typeof window !== 'undefined' ? window.ethereum : null;
  }
  isAvailable() {
    return !!(this.ethereum && this.ethereum.isMetaMask);
  }
  async connect() {
    if (!this.isAvailable()) throw new Error('MetaMask not available');
    const accounts = await this.ethereum.request({ method: 'eth_requestAccounts' });
    const chainHex = await this.ethereum.request({ method: 'eth_chainId' });
    this._attachEvents();
    return { accounts, chainId: parseInt(chainHex, 16) };
  }
  async disconnect() {}
  async request(args) {
    return this.ethereum.request(args);
  }
  async getAccounts() {
    const accounts = await this.ethereum.request({ method: 'eth_accounts' });
    return accounts || [];
  }
  async getChainId() {
    const chainHex = await this.ethereum.request({ method: 'eth_chainId' });
    return parseInt(chainHex, 16);
  }
  async switchChain(chainId) {
    const hex = '0x' + chainId.toString(16);
    try {
      await this.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
    } catch (err) {
      if (err && err.code === 4902) throw err;
      throw err;
    }
  }
  async addChain(chainConfig) {
    await this.ethereum.request({ method: 'wallet_addEthereumChain', params: [chainConfig] });
  }
  _attachEvents() {
    this.ethereum.removeListener?.('accountsChanged', this._onAccountsChanged);
    this.ethereum.removeListener?.('chainChanged', this._onChainChanged);
    this.ethereum.removeListener?.('disconnect', this._onDisconnect);
    this._onAccountsChanged = (accs) => this._emit('accountsChanged', accs);
    this._onChainChanged = (hex) => this._emit('chainChanged', parseInt(hex, 16));
    this._onDisconnect = (e) => this._emit('disconnect', e);
    this.ethereum.on('accountsChanged', this._onAccountsChanged);
    this.ethereum.on('chainChanged', this._onChainChanged);
    this.ethereum.on('disconnect', this._onDisconnect);
  }
}

if (typeof window !== 'undefined') {
  window.AigenMetaMaskProvider = MetaMaskProvider;
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = MetaMaskProvider;
}
