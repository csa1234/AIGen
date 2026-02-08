// Global Web3 context store for admin dashboard
class Web3Context {
  constructor() {
    this.state = {
      initialized: false,
      connected: false,
      address: null,
      accounts: [],
      chainId: null,
      network: null,
      providerName: null,
      error: null
    };
    this._subs = new Set();
  }
  subscribe(fn) { this._subs.add(fn); return () => this._subs.delete(fn); }
  _emit() { for (const fn of this._subs) { try { fn(this.state); } catch {} } }
  set(partial) { this.state = { ...this.state, ...partial }; this._emit(); }
}

if (!window.__web3Context) window.__web3Context = new Web3Context();
if (typeof module !== 'undefined' && module.exports) module.exports = window.__web3Context;
