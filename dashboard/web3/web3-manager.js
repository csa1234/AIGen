// Web3Manager: detect, connect, and manage wallet providers
(function () {
  const ctx = window.__web3Context;
  const networks = {
    1: { name: 'Ethereum', hex: '0x1' },
    137: { name: 'Polygon', hex: '0x89' },
    56: { name: 'BNB Chain', hex: '0x38' },
    42161: { name: 'Arbitrum', hex: '0xa4b1' },
    10: { name: 'Optimism', hex: '0xa' },
    11155111: { name: 'Sepolia', hex: '0xaa36a7' }
  };

  function detectEnvRequiredChain() {
    const cfg = window.__aigenConfig || {};
    if (cfg.web3 && typeof cfg.web3.requiredChainId === 'number') return cfg.web3.requiredChainId;
    const override = localStorage.getItem('web3RequiredChainId');
    if (override) return parseInt(override, 10);
    return null; // no enforcement by default; use config to set
  }

  class Web3Manager {
    constructor(options = {}) {
      this.options = options;
      this.provider = null;
      const cfg = window.__aigenConfig || {};
      const enableEvm = cfg.web3?.enableEvm !== false;
      this.requiredChainId = detectEnvRequiredChain();
      // Build dynamic addChainConfigs: merge defaults with env-provided chains and optional AIGEN EVM chain
      this.options.addChainConfigs = {
        ...(this.options.addChainConfigs || {}),
        ...(cfg.web3?.addChains || {}),
      };
      if (cfg.web3?.aigenEvmChain && cfg.web3.aigenEvmChain.chainId) {
        const a = cfg.web3.aigenEvmChain;
        const key = typeof a.chainId === 'number' ? a.chainId : parseInt(a.chainId, 16);
        this.options.addChainConfigs[key] = {
          chainId: typeof a.chainId === 'string' ? a.chainId : ('0x' + key.toString(16)),
          chainName: a.chainName || 'AIGEN',
          rpcUrls: a.rpcUrls || (cfg.rpc?.http ? [cfg.rpc.http] : []),
          nativeCurrency: a.nativeCurrency || { name: 'AIGEN', symbol: 'AGN', decimals: 18 },
          blockExplorerUrls: a.blockExplorerUrls || []
        };
        // If no explicit requiredChainId set elsewhere, prefer the AIGEN EVM chain id
        if (this.requiredChainId === null) this.requiredChainId = key;
      }
      this.providers = enableEvm ? [
        new (window.AigenMetaMaskProvider || class {} )(),
        new (window.AigenCoinbaseProvider || class {} )(),
        new (window.AigenTrustProvider || class {} )(),
        new (window.AigenBraveProvider || class {} )(),
        new (window.AigenPhantomProvider || class {} )(),
        new (window.AigenWalletConnectProvider || class {} )(options.walletConnect || {})
      ] : [];
      // Establish connection preference priority
      const pref = localStorage.getItem('preferredWallet');
      this._preferredWallet = pref && typeof pref === 'string' ? pref : null;
      ctx.set({ initialized: true, chainId: null });
    }

    detectAvailable() {
      return this.providers.filter(p => typeof p.isAvailable === 'function' && p.isAvailable());
    }

    getNetworkName(chainId) {
      return networks[chainId]?.name || `Chain ${chainId}`;
    }

    async connect(preferred = null) {
      const list = this.detectAvailable();
      let target = null;
      const preference = preferred || this._preferredWallet;
      if (preference) target = list.find(p => p.name === preference);
      if (!target) target = list[0];
      if (!target) throw new Error('No wallet providers available (EVM disabled)');
      const { accounts, chainId } = await target.connect();
      this.provider = target;
      this._attachProviderEvents();
      ctx.set({
        connected: true,
        accounts,
        address: accounts[0] || null,
        chainId,
        network: this.getNetworkName(chainId),
        providerName: target.name,
        error: null
      });
      // Ensure network match only if configured
      if (this.requiredChainId !== null) {
        try {
          await this.ensureNetwork(this.requiredChainId);
        } catch (err) {
          const msg = this._friendlyError(err);
          ctx.set({ error: msg });
        }
      }
      return ctx.state;
    }

    async disconnect() {
      if (!this.provider) return;
      try { await this.provider.disconnect(); } catch {}
      this._detachProviderEvents();
      this.provider = null;
      ctx.set({ connected: false, address: null, accounts: [], chainId: null, network: null, providerName: null });
    }

    async ensureNetwork(requiredChainId) {
      if (!this.provider || !ctx.state.connected) return;
      if (ctx.state.chainId === requiredChainId) return;
      try {
        await this.provider.switchChain(requiredChainId);
      } catch (err) {
        if (err && err.code === 4902 && this.options.addChainConfigs?.[requiredChainId]) {
          await this.provider.addChain(this.options.addChainConfigs[requiredChainId]);
          await this.provider.switchChain(requiredChainId);
        } else {
          // Some wallets do not implement add/switch methods; attempt generic request fallback
          try {
            const hex = networks[requiredChainId]?.hex || ('0x' + requiredChainId.toString(16));
            await this.provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
          } catch (e2) {
            if (this.options.addChainConfigs?.[requiredChainId]) {
              await this.provider.request({ method: 'wallet_addEthereumChain', params: [this.options.addChainConfigs[requiredChainId]] });
              await this.provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hex }] });
            } else {
              throw err;
            }
          }
        }
      }
    }

    _attachProviderEvents() {
      if (!this.provider) return;
      this._acc = (accs) => ctx.set({ accounts: accs, address: accs[0] || null });
      this._chain = (id) => ctx.set({ chainId: id, network: this.getNetworkName(id) });
      this._disc = () => this.disconnect();
      this.provider.on('accountsChanged', this._acc);
      this.provider.on('chainChanged', this._chain);
      this.provider.on('disconnect', this._disc);
    }
    _detachProviderEvents() {
      if (!this.provider) return;
      try {
        this.provider.off('accountsChanged', this._acc);
        this.provider.off('chainChanged', this._chain);
        this.provider.off('disconnect', this._disc);
      } catch {}
    }
    _friendlyError(e) {
      if (!e) return 'Unknown wallet error';
      if (e.code === 4001) return 'User rejected wallet request';
      if (e.code === 4902) return 'Required network not configured; attempting automatic setup';
      const m = typeof e.message === 'string' ? e.message : '';
      return m || 'Wallet operation failed';
    }
  }

  window.__web3Manager = new Web3Manager({
    walletConnect: {
      rpc: {
        1: (window.__aigenConfig?.web3?.walletConnectRpc?.[1]) || 'https://cloudflare-eth.com',
        137: (window.__aigenConfig?.web3?.walletConnectRpc?.[137]) || 'https://polygon-rpc.com',
        56: (window.__aigenConfig?.web3?.walletConnectRpc?.[56]) || 'https://bsc-dataseed.binance.org'
      }
    },
    addChainConfigs: {
      137: { chainId: '0x89', chainName: 'Polygon', rpcUrls: [(window.__aigenConfig?.chains?.polygonRpc || 'https://polygon-rpc.com')], nativeCurrency: { name: 'MATIC', symbol: 'MATIC', decimals: 18 }, blockExplorerUrls: ['https://polygonscan.com'] },
      56: { chainId: '0x38', chainName: 'BNB Chain', rpcUrls: [(window.__aigenConfig?.chains?.bscRpc || 'https://bsc-dataseed.binance.org')], nativeCurrency: { name: 'BNB', symbol: 'BNB', decimals: 18 }, blockExplorerUrls: ['https://bscscan.com'] },
      11155111: { chainId: '0xaa36a7', chainName: 'Sepolia', rpcUrls: [(window.__aigenConfig?.chains?.sepoliaRpc || 'https://rpc.sepolia.org')], nativeCurrency: { name: 'ETH', symbol: 'ETH', decimals: 18 }, blockExplorerUrls: ['https://sepolia.etherscan.io'] },
      31337: { chainId: '0x7a69', chainName: 'Hardhat Local', rpcUrls: [(window.__aigenConfig?.chains?.hardhatRpc || 'http://localhost:8545')], nativeCurrency: { name: 'ETH', symbol: 'ETH', decimals: 18 }, blockExplorerUrls: [] }
    }
  });
})(); 
