// Tests for Web3Manager detection and auto-configuration
(function () {
  const { it, expect, run } = window.__test;

  function mockEthereum(flags = {}) {
    const listeners = {};
    return {
      ...flags,
      request: async ({ method, params }) => {
        if (method === 'eth_requestAccounts') return ['0xabc'];
        if (method === 'eth_accounts') return ['0xabc'];
        if (method === 'eth_chainId') return '0xaa36a7'; // Sepolia
        if (method === 'wallet_switchEthereumChain') {
          const target = params?.[0]?.chainId;
          listeners['chainChanged']?.forEach(fn => fn(target));
          return null;
        }
        if (method === 'wallet_addEthereumChain') return null;
        throw new Error(`Unsupported method: ${method}`);
      },
      on: (evt, fn) => {
        listeners[evt] = listeners[evt] || new Set();
        listeners[evt].add(fn);
      },
      removeListener: (evt, fn) => { listeners[evt]?.delete(fn); }
    };
  }

  it('detects MetaMask and connects', async () => {
    window.ethereum = mockEthereum({ isMetaMask: true });
    const mgr = window.__web3Manager;
    const available = mgr.detectAvailable().map(p => p.name);
    expect(available.includes('MetaMask')).toBeTruthy();
    const st = await mgr.connect('MetaMask');
    expect(st.connected).toBeTruthy();
    expect(st.address).toBe('0xabc');
    await mgr.disconnect();
    window.ethereum = undefined;
  });

  it('falls back to WalletConnect availability when no injected wallet', async () => {
    window.ethereum = undefined;
    const mgr = window.__web3Manager;
    const available = mgr.detectAvailable().map(p => p.name);
    expect(available.includes('WalletConnect')).toBeTruthy();
  });

  it('ensureNetwork adds and switches when missing', async () => {
    window.ethereum = mockEthereum({ isMetaMask: true });
    const mgr = window.__web3Manager;
    await mgr.connect('MetaMask');
    const addCfg = {
      chainId: '0x7a69',
      chainName: 'Hardhat Local',
      rpcUrls: ['http://localhost:8545'],
      nativeCurrency: { name: 'ETH', symbol: 'ETH', decimals: 18 },
      blockExplorerUrls: []
    };
    mgr.options.addChainConfigs[31337] = addCfg;
    await mgr.ensureNetwork(31337);
    expect(window.__web3Context.state.chainId).toEqual(31337);
    await mgr.disconnect();
    window.ethereum = undefined;
  });

  run();
})(); 
