// Copyright (c) 2025-present Cesar Saguier Antebi
//
// Business Source License 1.1
//
// Base EIP-1193-like provider interface used by concrete wallet connectors.
// All providers must implement:
// - name: string
// - isAvailable(): boolean
// - connect(): Promise<{ accounts: string[], chainId: number }>
// - disconnect(): Promise<void>
// - request({ method, params }): Promise<any>
// - on(event, handler): void
// - off(event, handler): void
// - getAccounts(): Promise<string[]>
// - getChainId(): Promise<number>
// - switchChain(chainId: number): Promise<void>
// - addChain(chainConfig: object): Promise<void>

class BaseProvider {
  constructor() {
    this.name = 'BaseProvider';
    this._listeners = new Map();
  }

  isAvailable() { return false; }
  async connect() { throw new Error('connect not implemented'); }
  async disconnect() {}
  async request() { throw new Error('request not implemented'); }
  on(event, handler) {
    if (!this._listeners.has(event)) this._listeners.set(event, new Set());
    this._listeners.get(event).add(handler);
  }
  off(event, handler) {
    if (this._listeners.has(event)) this._listeners.get(event).delete(handler);
  }
  _emit(event, payload) {
    const set = this._listeners.get(event);
    if (!set) return;
    for (const fn of set) {
      try { fn(payload); } catch {}
    }
  }
  async getAccounts() { return []; }
  async getChainId() { return 0; }
  async switchChain(_chainId) { throw new Error('switchChain not implemented'); }
  async addChain(_cfg) { throw new Error('addChain not implemented'); }
}

if (typeof window !== 'undefined') {
  window.AigenBaseProvider = BaseProvider;
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = BaseProvider;
}
