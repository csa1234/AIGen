// Copyright (c) 2025-present Cesar Saguier Antebi
//
// Phantom wallet detection. If Phantom injects EVM (window.ethereum.isPhantom), use it.
// Otherwise, mark as unsupported for EVM flows and surface guidance.

class PhantomProvider extends (window.AigenBaseProvider || class {}) {
  constructor() {
    super();
    this.name = 'Phantom';
    this.ethereum = typeof window !== 'undefined' ? window.ethereum : null;
    this.solana = typeof window !== 'undefined' ? (window.phantom || window.solana) : null;
  }
  isAvailable() {
    const evm = !!(this.ethereum && this.ethereum.isPhantom);
    const sol = !!this.solana;
    return evm || sol;
  }
  async connect() {
    if (this.ethereum && this.ethereum.isPhantom) {
      try {
        const accounts = await this.ethereum.request({ method: 'eth_requestAccounts' });
        const chainHex = await this.ethereum.request({ method: 'eth_chainId' });
        const chainId = parseInt(chainHex, 16);
        this._attachEvm();
        return { accounts, chainId };
      } catch (e) {
        const msg = this._friendlyError(e);
        throw new Error(msg);
      }
    }
    throw new Error('Phantom is currently in Solana-only mode. Enable Phantom EVM or use WalletConnect to connect an EVM wallet.');
  }
  async disconnect() {}
  async request(args) {
    if (!this.ethereum || !this.ethereum.isPhantom) throw new Error('Phantom EVM not available');
    return this.ethereum.request(args);
  }
  async getAccounts() {
    if (!this.ethereum || !this.ethereum.isPhantom) return [];
    return await this.ethereum.request({ method: 'eth_accounts' }) || [];
  }
  async getChainId() {
    if (!this.ethereum || !this.ethereum.isPhantom) return 0;
    return parseInt(await this.ethereum.request({ method: 'eth_chainId' }), 16);
  }
  async switchChain(chainId) {
    const hex = '0x' + chainId.toString(16);
    try {
      await this.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
    } catch (e) {
      // propagate specific code for missing chain
      throw e;
    }
  }
  async addChain(cfg) {
    try {
      await this.ethereum.request({ method: 'wallet_addEthereumChain', params: [cfg] });
    } catch (e) {
      throw new Error(this._friendlyError(e));
    }
  }
  _attachEvm() {
    this._onAccountsChanged = (accs) => this._emit('accountsChanged', accs);
    this._onChainChanged = (hex) => this._emit('chainChanged', parseInt(hex, 16));
    this._onDisconnect = (e) => this._emit('disconnect', e);
    this.ethereum.on('accountsChanged', this._onAccountsChanged);
    this.ethereum.on('chainChanged', this._onChainChanged);
    this.ethereum.on('disconnect', this._onDisconnect);
  }
  _friendlyError(e) {
    if (!e) return 'Unknown Phantom error';
    if (e.code === 4001) return 'User rejected wallet request';
    if (e.code === 4902) return 'Required network not added to Phantom. Attempting automatic configuration...';
    const m = typeof e.message === 'string' ? e.message : '';
    if (m.includes('Solana')) return 'Phantom Solana account cannot be used for Ethereum. Switch to an EVM-compatible account.';
    return m || 'Wallet operation failed';
  }
}

if (typeof window !== 'undefined') window.AigenPhantomProvider = PhantomProvider;
if (typeof module !== 'undefined' && module.exports) module.exports = PhantomProvider;
