// Wallet UI: connect button, selection modal, connected badge, disconnect
(function () {
  const ctx = window.__web3Context;
  const mgr = window.__web3Manager;

  function truncate(addr) {
    if (!addr) return '-';
    return addr.length > 10 ? `${addr.slice(0, 6)}...${addr.slice(-4)}` : addr;
  }

  function buildModal() {
    const container = document.createElement('div');
    container.id = 'walletSelectModal';
    container.className = 'modal';
    container.innerHTML = `
      <div class="modal-overlay"></div>
      <div class="modal-content">
        <div class="modal-header">
          <h2>Select Wallet</h2>
          <button class="modal-close">&times;</button>
        </div>
        <div class="modal-body">
          <div class="wallet-grid">
            <button class="wallet-item" data-wallet="MetaMask">MetaMask</button>
            <button class="wallet-item" data-wallet="Coinbase Wallet">Coinbase Wallet</button>
            <button class="wallet-item" data-wallet="Trust Wallet">Trust Wallet</button>
            <button class="wallet-item" data-wallet="Brave Wallet">Brave Wallet</button>
            <button class="wallet-item" data-wallet="Phantom">Phantom</button>
            <button class="wallet-item" data-wallet="WalletConnect">WalletConnect</button>
          </div>
          <div class="wallet-help">
            <p>If your wallet is not detected, try WalletConnect.</p>
            <p id="walletDetectionInfo" class="wallet-detection-info"></p>
          </div>
        </div>
      </div>`;
    document.body.appendChild(container);
    container.querySelector('.modal-close').addEventListener('click', () => hideModal());
    container.querySelector('.modal-overlay').addEventListener('click', () => hideModal());
    const available = mgr.detectAvailable().map(p => p.name);
    container.querySelectorAll('.wallet-item').forEach(b => {
      const name = b.dataset.wallet;
      if (!available.includes(name) && name !== 'WalletConnect') {
        b.disabled = true;
        b.classList.add('disabled');
        b.title = `${name} not detected`;
      }
      b.addEventListener('click', async () => {
        const name = b.dataset.wallet;
        try {
          setWalletButtonLoading(true);
          await mgr.connect(name);
          // auto network alignment for local dev (Sepolia by default)
          await mgr.ensureNetwork(mgr.requiredChainId);
          hideModal();
          try { localStorage.setItem('preferredWallet', name); } catch {}
        } catch (e) {
          const msg = e && e.message ? e.message : 'Connection failed';
          showToast(msg, 'error');
        } finally {
          setWalletButtonLoading(false);
        }
      });
    });
    const info = document.getElementById('walletDetectionInfo');
    if (available.length === 0) {
      info.textContent = 'No injected wallets detected. Use WalletConnect or enable EVM support in Settings.';
    } else {
      info.textContent = `Detected: ${available.join(', ')}`;
    }
  }

  function showModal() {
    document.getElementById('walletSelectModal').classList.add('active');
  }
  function hideModal() {
    document.getElementById('walletSelectModal').classList.remove('active');
  }

  function showToast(message, type = 'info') {
    const container = document.getElementById('notificationContainer');
    const el = document.createElement('div');
    el.className = `notification notification-${type}`;
    el.textContent = message;
    container.appendChild(el);
    setTimeout(() => el.classList.add('show'), 10);
    setTimeout(() => { el.classList.remove('show'); setTimeout(() => el.remove(), 300); }, 3000);
  }

  function setWalletButtonLoading(loading) {
    const btn = document.getElementById('walletButton');
    if (!btn) return;
    btn.disabled = loading;
    btn.classList.toggle('loading', loading);
  }

  function updateWalletButton() {
    const btn = document.getElementById('walletButton');
    if (!btn) return;
    if (ctx.state.connected) {
      btn.innerHTML = `
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M20 12V8H6a2 2 0 0 1-2-2c0-1.1.9-2 2-2h12v4"/>
          <path d="M4 6v12c0 1.1.9 2 2 2h14v-4"/>
          <path d="M18 12a2 2 0 0 0 0 4h4v-4h-4z"/>
        </svg>
        <span>${truncate(ctx.state.address)}</span>
      `;
      btn.onclick = async () => {
        try { await mgr.disconnect(); } catch (e) {}
      };
      btn.title = `Connected via ${ctx.state.providerName}. Click to disconnect.`;
    } else {
      btn.innerHTML = `
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M20 12V8H6a2 2 0 0 1-2-2c0-1.1.9-2 2-2h12v4"/>
          <path d="M4 6v12c0 1.1.9 2 2 2h14v-4"/>
          <path d="M18 12a2 2 0 0 0 0 4h4v-4h-4z"/>
        </svg>
        <span>Connect Wallet</span>
      `;
      btn.onclick = () => showModal();
      btn.title = 'Connect a Web3 wallet';
    }
  }

  document.addEventListener('DOMContentLoaded', () => {
    buildModal();
    updateWalletButton();
    ctx.subscribe(() => updateWalletButton());
  });
})(); 
