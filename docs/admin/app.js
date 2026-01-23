class AdminDashboard {
    constructor() {
        this.rpcClient = null;
        this.walletManager = null;
        this.chartManager = null;
        this.settings = this.loadSettings();
        this.currentBlockPage = 1;
        this.blocksPerPage = 10;
        this.autoRefreshInterval = null;
        this.currentBlockHash = null;
        
        this.init();
    }

    async init() {
        this.showLoading(true);
        
        try {
            this.rpcClient = new AdminRPCClient(this.settings.rpcUrl, this.settings.wsUrl);
            this.walletManager = new WalletManager();
            this.chartManager = new ChartManager();
            
            this.setupEventListeners();
            this.applyTheme(this.settings.theme);
            
            await this.connect();
            this.showLoading(false);
            
            this.showNotification('Dashboard initialized successfully', 'success');
        } catch (error) {
            this.showLoading(false);
            this.showNotification(`Initialization failed: ${error.message}`, 'error');
        }
    }

    loadSettings() {
        const defaultSettings = {
            rpcUrl: 'http://localhost:9944',
            wsUrl: 'ws://localhost:9944',
            ceoAddress: '',
            theme: 'dark',
            refreshInterval: 30
        };
        
        const saved = localStorage.getItem('adminSettings');
        return saved ? { ...defaultSettings, ...JSON.parse(saved) } : defaultSettings;
    }

    saveSettings(settings) {
        this.settings = { ...this.settings, ...settings };
        localStorage.setItem('adminSettings', JSON.stringify(this.settings));
    }

    setupEventListeners() {
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const tabName = e.currentTarget.dataset.tab;
                this.switchTab(tabName);
            });
        });

        document.getElementById('walletButton').addEventListener('click', () => this.connectWallet());
        document.getElementById('settingsButton').addEventListener('click', () => this.openSettings());
        
        document.querySelectorAll('.modal-close').forEach(btn => {
            btn.addEventListener('click', () => this.closeModals());
        });
        
        document.querySelectorAll('.modal-overlay').forEach(overlay => {
            overlay.addEventListener('click', () => this.closeModals());
        });

        document.getElementById('settingsForm').addEventListener('submit', (e) => {
            e.preventDefault();
            this.saveSettings({
                rpcUrl: document.getElementById('rpcUrl').value,
                wsUrl: document.getElementById('wsUrl').value,
                ceoAddress: document.getElementById('ceoAddress').value,
                ceoPrivateKey: document.getElementById('ceoPrivateKey').value,
                theme: document.getElementById('themeSelect').value
            });
            this.closeModals();
            this.showNotification('Settings saved', 'success');
        });

        document.getElementById('initModelBtn').addEventListener('click', () => this.showInitModelForm());
        document.getElementById('initModelForm').addEventListener('submit', (e) => this.submitInitModel(e));
        
        document.getElementById('searchButton').addEventListener('click', () => this.searchBlockOrTx());
        document.getElementById('searchInput').addEventListener('keypress', (e) => {
            if (e.key === 'Enter') this.searchBlockOrTx();
        });

        document.getElementById('prevBlocksBtn').addEventListener('click', () => this.loadBlocks(this.currentBlockPage - 1));
        document.getElementById('nextBlocksBtn').addEventListener('click', () => this.loadBlocks(this.currentBlockPage + 1));

        document.getElementById('refreshInterval').addEventListener('change', (e) => {
            const interval = parseInt(e.target.value);
            this.startAutoRefresh(interval);
        });

        document.getElementById('refreshBtn').addEventListener('click', () => {
            this.loadHealthData();
            this.loadMetricsData();
        });

        document.getElementById('voteForm').addEventListener('submit', (e) => this.submitVote(e));
        document.getElementById('approveSipBtn').addEventListener('click', () => this.approveSIP());
        document.getElementById('vetoSipBtn').addEventListener('click', () => this.vetoSIP());
        document.getElementById('shutdownForm').addEventListener('submit', (e) => this.submitShutdown(e));

        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') this.closeModals();
        });

        if (window.ethereum) {
            window.ethereum.on('accountsChanged', (accounts) => {
                if (accounts.length === 0) {
                    this.showNotification('Wallet disconnected', 'warning');
                } else {
                    this.verifyCeoAccess(accounts[0]);
                }
            });
            
            window.ethereum.on('chainChanged', () => {
                window.location.reload();
            });
        }
    }

    async connect() {
        try {
            await this.rpcClient.connect();
            this.updateConnectionStatus('connected');
            
            const walletAddress = await this.walletManager.getAddress();
            if (walletAddress) {
                this.verifyCeoAccess(walletAddress);
            }
            
            this.loadBlocks(1);
        } catch (error) {
            this.updateConnectionStatus('disconnected');
            throw error;
        }
    }

    async connectWallet() {
        try {
            const address = await this.walletManager.connect();
            this.verifyCeoAccess(address);
            this.updateWalletButton(address);
            this.showNotification('Wallet connected successfully', 'success');
        } catch (error) {
            this.showNotification(`Wallet connection failed: ${error.message}`, 'error');
        }
    }

    verifyCeoAccess(address) {
        if (!this.rpcClient.verifyCeoWallet(address)) {
            this.showNotification('Warning: Connected wallet is not the CEO address', 'warning');
        } else {
            this.showNotification('CEO wallet verified', 'success');
        }
    }

    updateWalletButton(address) {
        const btn = document.getElementById('walletButton');
        btn.innerHTML = `
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M20 12V8H6a2 2 0 0 1-2-2c0-1.1.9-2 2-2h12v4"/>
                <path d="M4 6v12c0 1.1.9 2 2 2h14v-4"/>
                <path d="M18 12a2 2 0 0 0 0 4h4v-4h-4z"/>
            </svg>
            <span>${this.formatHash(address)}</span>
        `;
    }

    switchTab(tabName) {
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.tab === tabName);
        });
        
        document.querySelectorAll('.tab-content').forEach(content => {
            content.classList.toggle('active', content.id === `${tabName}Tab`);
        });
        
        if (tabName === 'explorer') {
            this.loadBlocks(this.currentBlockPage);
        } else if (tabName === 'models') {
            this.loadModels();
            this.loadModelProposals();
        } else if (tabName === 'health') {
            this.loadHealthData();
            this.loadMetricsData();
            this.startAutoRefresh(parseInt(document.getElementById('refreshInterval').value));
        } else if (tabName === 'governance') {
            this.loadGovernanceProposals();
        }
    }

    async loadBlocks(page) {
        try {
            this.currentBlockPage = page;
            const blocks = await this.rpcClient.getLatestBlocks(this.blocksPerPage);
            this.renderBlockTable(blocks);
            
            document.getElementById('prevBlocksBtn').disabled = page <= 1;
            document.getElementById('blocksPageInfo').textContent = `Page ${page}`;
        } catch (error) {
            this.showNotification(`Failed to load blocks: ${error.message}`, 'error');
        }
    }

    async loadTransactions(blockHash) {
        try {
            this.currentBlockHash = blockHash;
            const block = await this.rpcClient.getBlock(blockHash);
            this.renderTransactionTable(block.transactions);
        } catch (error) {
            this.showNotification(`Failed to load transactions: ${error.message}`, 'error');
        }
    }

    async searchBlockOrTx() {
        const query = document.getElementById('searchInput').value.trim();
        if (!query) return;
        
        try {
            this.showLoading(true);
            
            const block = await this.rpcClient.getBlock(query);
            if (block) {
                this.switchTab('explorer');
                this.renderTransactionTable(block.transactions);
                this.showNotification('Block found', 'success');
            } else {
                const tx = await this.rpcClient.getTransaction(query);
                if (tx) {
                    this.switchTab('explorer');
                    this.showNotification('Transaction found', 'success');
                } else {
                    this.showNotification('Block or transaction not found', 'error');
                }
            }
            
            this.showLoading(false);
        } catch (error) {
            this.showLoading(false);
            this.showNotification(`Search failed: ${error.message}`, 'error');
        }
    }

    renderBlockTable(blocks) {
        const tbody = document.getElementById('blocksTableBody');
        
        if (!blocks || blocks.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">No blocks found</td></tr>';
            return;
        }
        
        tbody.innerHTML = blocks.map(block => `
            <tr>
                <td>${block.header.block_height}</td>
                <td><code class="hash">${this.formatHash(block.block_hash)}</code></td>
                <td>${this.formatTimestamp(block.header.timestamp)}</td>
                <td>${block.transactions.length}</td>
                <td>
                    <button class="btn btn-sm btn-secondary" onclick="dashboard.loadTransactions('${block.block_hash}')">
                        View
                    </button>
                </td>
            </tr>
        `).join('');
    }

    renderTransactionTable(transactions) {
        const tbody = document.getElementById('transactionsTableBody');
        
        if (!transactions || transactions.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">No transactions</td></tr>';
            return;
        }
        
        tbody.innerHTML = transactions.map(tx => `
            <tr>
                <td><code class="hash">${this.formatHash(tx.tx_hash)}</code></td>
                <td><code class="hash">${this.formatHash(tx.sender)}</code></td>
                <td><code class="hash">${this.formatHash(tx.receiver)}</code></td>
                <td>${this.formatAmount(tx.amount)}</td>
                <td>${this.formatTimestamp(tx.timestamp)}</td>
            </tr>
        `).join('');
    }

    async loadModels() {
        try {
            const models = await this.rpcClient.listModels();
            this.renderModelTable(models);
        } catch (error) {
            this.showNotification(`Failed to load models: ${error.message}`, 'error');
        }
    }

    renderModelTable(models) {
        const tbody = document.getElementById('modelsTableBody');
        
        if (!models || models.length === 0) {
            tbody.innerHTML = '<tr><td colspan="8" class="loading-cell">No models registered</td></tr>';
            return;
        }
        
        tbody.innerHTML = models.map(model => `
            <tr>
                <td><code class="hash">${this.formatHash(model.model_id)}</code></td>
                <td>${model.name}</td>
                <td>${model.version}</td>
                <td>${this.formatBytes(model.total_size)}</td>
                <td>${model.shard_count}</td>
                <td>${model.is_core_model ? '✓' : '-'}</td>
                <td><span class="status-badge status-${this.getModelStatus(model)}">${this.getModelStatus(model)}</span></td>
                <td>
                    <button class="btn btn-sm btn-primary" onclick="dashboard.loadModelOnNode('${model.model_id}')" title="Load model requires signed transaction. Use CLI for complex transaction signing." disabled>
                        Load
                    </button>
                </td>
            </tr>
        `).join('');
    }

    getModelStatus(model) {
        if (model.isLoaded) return 'loaded';
        if (model.isAvailable) return 'available';
        return 'unavailable';
    }

    showInitModelForm() {
        document.getElementById('initModelModal').classList.add('active');
    }

    async submitInitModel(e) {
        e.preventDefault();
        
        try {
            this.showLoading(true);
            
            const request = {
                modelId: document.getElementById('modelId').value,
                name: document.getElementById('modelName').value,
                version: document.getElementById('modelVersion').value,
                totalSize: parseInt(document.getElementById('totalSize').value),
                shardCount: parseInt(document.getElementById('shardCount').value),
                verificationHashes: document.getElementById('verificationHashes').value.split(',').map(h => h.trim()),
                isCoreModel: document.getElementById('isCoreModel').checked,
                minimumTier: parseInt(document.getElementById('minimumTier').value),
                isExperimental: document.getElementById('isExperimental').checked
            };
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('initNewModel', {
                modelId: request.modelId
            });
            
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.initNewModel(request, signature);
            
            this.closeModals();
            this.loadModels();
            this.showNotification('Model initialized successfully', 'success');
        } catch (error) {
            this.showNotification(`Initialization failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async loadModelOnNode(modelId) {
        const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
        if (!ceoPrivateKey) {
            this.showNotification('Load model requires CEO private key. Please configure it in settings.', 'error');
            return;
        }

        try {
            this.showLoading(true);

            // Get user address from wallet or prompt
            const userAddress = await this.walletManager.getAddress();
            if (!userAddress) {
                throw new Error('No wallet connected');
            }

            // For now, loadModel requires a signed transaction
            // This is complex to implement in browser without full transaction signing
            // Show message that this feature needs additional implementation
            this.showNotification('Load model requires signed transaction. This feature is under development.', 'warning');
        } catch (error) {
            this.showNotification(`Failed to load model: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async loadModelProposals() {
        try {
            const proposals = await this.rpcClient.listModels();
            const upgradeProposals = proposals.filter(m => m.upgradeProposal);
            this.renderProposalsTable(upgradeProposals);
        } catch (error) {
            this.showNotification(`Failed to load proposals: ${error.message}`, 'error');
        }
    }

    renderProposalsTable(proposals) {
        const tbody = document.getElementById('proposalsTableBody');
        
        if (!proposals || proposals.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">No upgrade proposals</td></tr>';
            return;
        }
        
        tbody.innerHTML = proposals.map(model => `
            <tr>
                <td><code class="hash">${model.upgradeProposal.id}</code></td>
                <td><code class="hash">${this.formatHash(model.id)}</code></td>
                <td>${model.version} → ${model.upgradeProposal.targetVersion}</td>
                <td><span class="status-badge status-${model.upgradeProposal.status}">${model.upgradeProposal.status}</span></td>
                <td>
                    <button class="btn btn-sm btn-success" onclick="dashboard.approveUpgrade('${model.upgradeProposal.id}')">
                        Approve
                    </button>
                    <button class="btn btn-sm btn-danger" onclick="dashboard.rejectUpgrade('${model.upgradeProposal.id}')">
                        Reject
                    </button>
                </td>
            </tr>
        `).join('');
    }

    async approveUpgrade(proposalId) {
        try {
            this.showLoading(true);
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('approveModelUpgrade', {
                proposalId
            });
            
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.approveModelUpgrade(proposalId, signature, timestamp);
            
            this.loadModelProposals();
            this.showNotification('Upgrade approved', 'success');
        } catch (error) {
            this.showNotification(`Approval failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async rejectUpgrade(proposalId) {
        const reason = prompt('Enter rejection reason:');
        if (!reason) return;
        
        try {
            this.showLoading(true);
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('rejectUpgrade', {
                proposalId
            });
            
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.rejectUpgrade(proposalId, reason, signature, timestamp);
            
            this.loadModelProposals();
            this.showNotification('Upgrade rejected', 'success');
        } catch (error) {
            this.showNotification(`Rejection failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async loadHealthData() {
        try {
            const { message, timestamp } = this.rpcClient.formatAdminMessage('health');
            const signature = await this.rpcClient.signMessage(message);
            
            const health = await this.rpcClient.getHealth(signature, timestamp);
            this.renderHealthCards(health);
        } catch (error) {
            this.showNotification(`Failed to load health data: ${error.message}`, 'error');
        }
    }

    async loadMetricsData() {
        try {
            const { message, timestamp } = this.rpcClient.formatAdminMessage('metrics');
            const signature = await this.rpcClient.signMessage(message);
            
            const metrics = await this.rpcClient.getMetrics(signature, timestamp, true, true, true);
            this.chartManager.updateCharts(metrics);
        } catch (error) {
            this.showNotification(`Failed to load metrics: ${error.message}`, 'error');
        }
    }

    renderHealthCards(health) {
        const updateCard = (containerId, index, label, value, status) => {
            const container = document.getElementById(containerId);
            const cards = container.querySelectorAll('.health-card');
            if (cards[index]) {
                cards[index].querySelector('.status-text').textContent = value;
                const icon = cards[index].querySelector('.card-icon');
                icon.textContent = status === 'healthy' ? '🟢' : status === 'warning' ? '🟡' : '🔴';
            }
        };
        
        if (health.node_health) {
            updateCard('nodeHealthCards', 0, 'RPC Status', health.node_health.status, 'healthy');
            updateCard('nodeHealthCards', 1, 'Peer Count', health.node_health.peer_count, 'healthy');
            updateCard('nodeHealthCards', 2, 'Memory Usage', `${health.node_health.memory_usage_mb} MB`, 'healthy');
        }
        
        if (health.ai_health) {
            updateCard('aiHealthCards', 0, 'Inference Service', health.ai_health.core_model_loaded ? 'Active' : 'Inactive', health.ai_health.core_model_loaded ? 'healthy' : 'warning');
            updateCard('aiHealthCards', 1, 'Cache Hit Rate', `${(health.ai_health.cache_hit_rate * 100).toFixed(1)}%`, 'healthy');
            updateCard('aiHealthCards', 2, 'Memory Usage', `${health.ai_health.memory_usage_mb} MB`, 'healthy');
        }
        
        if (health.blockchain_health) {
            updateCard('blockchainHealthCards', 0, 'Chain Height', health.blockchain_health.chain_height, 'healthy');
            updateCard('blockchainHealthCards', 1, 'Pending Transactions', health.blockchain_health.pending_transactions, 'healthy');
            updateCard('blockchainHealthCards', 2, 'Shutdown Active', health.blockchain_health.shutdown_active ? 'Yes' : 'No', health.blockchain_health.shutdown_active ? 'error' : 'healthy');
        }
    }

    startAutoRefresh(interval) {
        this.stopAutoRefresh();
        
        if (interval === 0) return;
        
        this.autoRefreshInterval = setInterval(() => {
            this.loadHealthData();
            this.loadMetricsData();
        }, interval * 1000);
    }

    stopAutoRefresh() {
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
            this.autoRefreshInterval = null;
        }
    }

    async loadGovernanceProposals() {
        try {
            // Governance listing RPC not implemented on server
            // Show message in table
            this.renderGovernanceTable(null);
        } catch (error) {
            this.showNotification(`Failed to load proposals: ${error.message}`, 'error');
        }
    }

    renderGovernanceTable(proposals) {
        const tbody = document.getElementById('governanceTableBody');
        
        if (!proposals || proposals.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">Governance listing RPC not available. Use CLI for governance operations.</td></tr>';
            return;
        }
        
        tbody.innerHTML = proposals.map(proposal => `
            <tr>
                <td><code class="hash">${this.formatHash(proposal.id)}</code></td>
                <td>${proposal.type}</td>
                <td>${proposal.description}</td>
                <td><span class="status-badge status-${proposal.status}">${proposal.status}</span></td>
                <td>
                    <button class="btn btn-sm btn-primary" onclick="dashboard.voteOnProposal('${proposal.id}')">
                        Vote
                    </button>
                </td>
            </tr>
        `).join('');
    }

    async submitVote(e) {
        e.preventDefault();
        
        try {
            this.showLoading(true);
            
            const proposalId = document.getElementById('voteProposalId').value;
            const vote = document.getElementById('voteType').value;
            const comment = document.getElementById('voteComment').value;
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('submitGovVote', {
                proposalId,
                vote
            });
            
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.submitGovVote(proposalId, vote, comment, signature, timestamp);
            
            this.showNotification('Vote submitted successfully', 'success');
            document.getElementById('voteForm').reset();
        } catch (error) {
            this.showNotification(`Vote submission failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async approveSIP() {
        const proposalId = document.getElementById('sipProposalId').value;
        if (!proposalId) {
            this.showNotification('Please enter a SIP proposal ID', 'error');
            return;
        }
        
        try {
            this.showLoading(true);
            
            const { message } = this.rpcClient.formatAdminMessage('approveSIP', { proposalId });
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.approveSIP(proposalId, signature);
            
            this.showNotification('SIP approved', 'success');
        } catch (error) {
            this.showNotification(`Approval failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async vetoSIP() {
        const proposalId = document.getElementById('sipProposalId').value;
        if (!proposalId) {
            this.showNotification('Please enter a SIP proposal ID', 'error');
            return;
        }
        
        if (!confirm('Are you sure you want to veto this SIP?')) return;
        
        try {
            this.showLoading(true);
            
            const { message } = this.rpcClient.formatAdminMessage('vetoSIP', { proposalId });
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.vetoSIP(proposalId, signature);
            
            this.showNotification('SIP vetoed', 'success');
        } catch (error) {
            this.showNotification(`Veto failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async submitShutdown(e) {
        e.preventDefault();
        
        if (!confirm('Are you sure you want to trigger an emergency shutdown? This action cannot be undone.')) {
            return;
        }
        
        try {
            this.showLoading(true);
            
            const reason = document.getElementById('shutdownReason').value;
            const nonce = parseInt(document.getElementById('shutdownNonce').value);
            const timestamp = Math.floor(Date.now() / 1000);
            
            const { message } = this.rpcClient.formatAdminMessage('shutdown', {
                nonce,
                reason
            });
            
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.submitShutdown(timestamp, reason, nonce, signature);
            
            this.showNotification('Shutdown command submitted', 'success');
            document.getElementById('shutdownForm').reset();
        } catch (error) {
            this.showNotification(`Shutdown failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    openSettings() {
        document.getElementById('rpcUrl').value = this.settings.rpcUrl;
        document.getElementById('wsUrl').value = this.settings.wsUrl;
        document.getElementById('ceoAddress').value = this.settings.ceoAddress || '';
        document.getElementById('ceoPrivateKey').value = this.settings.ceoPrivateKey || '';
        document.getElementById('themeSelect').value = this.settings.theme;
        document.getElementById('settingsModal').classList.add('active');
    }

    closeModals() {
        document.querySelectorAll('.modal').forEach(modal => {
            modal.classList.remove('active');
        });
    }

    applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
    }

    showNotification(message, type = 'info') {
        const container = document.getElementById('notificationContainer');
        const notification = document.createElement('div');
        notification.className = `notification notification-${type}`;
        notification.textContent = message;
        
        container.appendChild(notification);
        
        setTimeout(() => {
            notification.classList.add('show');
        }, 10);
        
        setTimeout(() => {
            notification.classList.remove('show');
            setTimeout(() => notification.remove(), 300);
        }, 3000);
    }

    showLoading(show) {
        const overlay = document.getElementById('loadingOverlay');
        overlay.style.display = show ? 'flex' : 'none';
    }

    updateConnectionStatus(status) {
        const statusEl = document.getElementById('connectionStatus');
        const dot = statusEl.querySelector('.status-dot');
        const text = statusEl.querySelector('.status-text');
        
        statusEl.className = `connection-status status-${status}`;
        text.textContent = status === 'connected' ? 'Connected' : 'Disconnected';
    }

    formatHash(hash) {
        if (!hash) return '-';
        return hash.length > 16 ? `${hash.substring(0, 8)}...${hash.substring(hash.length - 8)}` : hash;
    }

    formatTimestamp(timestamp) {
        if (!timestamp) return '-';
        return new Date(timestamp * 1000).toLocaleString();
    }

    formatBytes(bytes) {
        if (!bytes) return '-';
        const units = ['B', 'KB', 'MB', 'GB', 'TB'];
        let i = 0;
        while (bytes >= 1024 && i < units.length - 1) {
            bytes /= 1024;
            i++;
        }
        return `${bytes.toFixed(2)} ${units[i]}`;
    }

    formatAmount(amount) {
        if (!amount) return '0';
        return (amount / 1e18).toFixed(6);
    }
}

let dashboard;
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new AdminDashboard();
});
