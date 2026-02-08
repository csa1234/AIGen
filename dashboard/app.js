// Copyright (c) 2025-present Cesar Saguier Antebi
//
// This file is part of AIGEN Blockchain.
//
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     `https://github.com/yourusername/aigen/blob/main/LICENSE`
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

class AdminDashboard {
    constructor() {
        this.rpcClient = null;
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
            this.chartManager = new ChartManager();
            
            this.setupEventListeners();
            this.applyTheme(this.settings.theme);
            this.checkCeoKeyStatus();
            
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

    checkCeoKeyStatus() {
        const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
        const statusEl = document.querySelector('#headerKeyStatus');
        const keyButton = document.getElementById('configureKeysButton');
        const keyStatusText = document.getElementById('keyStatusText');
        const keyStatusMessage = document.getElementById('keyStatusMessage');
        
        if (!ceoPrivateKey) {
            statusEl.classList.add('warning');
            statusEl.classList.remove('success');
            keyStatusMessage.textContent = '⚠️ CEO key not configured. Click Configure Keys to set up.';
            keyButton.classList.remove('btn-success');
            keyButton.classList.add('btn-warning');
            keyStatusText.textContent = 'Configure Keys';
        } else {
            statusEl.classList.remove('warning');
            statusEl.classList.add('success');
            
            // Derive and display public key
            try {
                const publicKey = this.derivePublicKey(ceoPrivateKey);
                const shortKey = this.formatHash(publicKey);
                keyStatusMessage.textContent = `✓ CEO key configured: ${shortKey}`;
                keyButton.classList.remove('btn-warning');
                keyButton.classList.add('btn-success');
                keyStatusText.textContent = 'Keys Configured';
            } catch (error) {
                keyStatusMessage.textContent = '⚠️ Invalid CEO key format';
                statusEl.classList.add('warning');
            }
        }
    }

    derivePublicKey(privateKeyHex) {
        if (typeof nacl === 'undefined') {
            throw new Error('tweetnacl not loaded');
        }
        
        const privateKeyBytes = this.hexToBytes(privateKeyHex);
        if (privateKeyBytes.length !== 64) {
            throw new Error('Invalid private key length');
        }
        
        const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));
        return this.bytesToHex(keyPair.publicKey);
    }

    hexToBytes(hex) {
        hex = hex.replace(/^0x/, '');
        const bytes = new Uint8Array(hex.length / 2);
        for (let i = 0; i < bytes.length; i++) {
            bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
        }
        return bytes;
    }

    bytesToHex(bytes) {
        return Array.from(bytes)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
    }

    setupEventListeners() {
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const tabName = e.currentTarget.dataset.tab;
                this.switchTab(tabName);
            });
        });

        document.getElementById('configureKeysButton').addEventListener('click', () => this.openSettings());
        
        document.querySelectorAll('.modal-close').forEach(btn => {
            btn.addEventListener('click', () => this.closeModals());
        });
        
        document.querySelectorAll('.modal-overlay').forEach(overlay => {
            overlay.addEventListener('click', () => this.closeModals());
        });

        document.getElementById('settingsForm').addEventListener('submit', (e) => {
            e.preventDefault();
            const ceoPrivateKey = document.getElementById('ceoPrivateKey').value.trim();
            if (ceoPrivateKey) {
                localStorage.setItem('ceoPrivateKey', ceoPrivateKey);
            } else {
                localStorage.removeItem('ceoPrivateKey');
                this.updateDerivedKey('');
            }
            this.saveSettings({
                rpcUrl: document.getElementById('rpcUrl').value,
                wsUrl: document.getElementById('wsUrl').value,
                theme: document.getElementById('themeSelect').value
            });
            this.closeModals();
            this.showNotification('Settings saved', 'success');
            this.checkCeoKeyStatus();
        });

        document.getElementById('generateKeypairBtn').addEventListener('click', () => this.generateNewKeypair());
        document.getElementById('toggleKeyVisibility').addEventListener('click', () => this.toggleKeyVisibility());
        document.getElementById('copyPublicKeyBtn').addEventListener('click', () => this.copyPublicKey());
        document.getElementById('ceoPrivateKey').addEventListener('input', (e) => this.updateDerivedKey(e.target.value));

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

    }

    async connect() {
        try {
            this.updateConnectionStatus('connecting');
            // Pass maxRetries and status callback
            await this.rpcClient.connect(3, (status, msg) => {
                // Determine state based on status string or explicit status
                if (status === 'connecting' || status.includes('Connecting') || status.includes('Checking')) {
                    this.updateConnectionStatus('connecting', msg || status);
                } else if (status === 'disconnected') {
                    this.updateConnectionStatus('disconnected', msg);
                } else if (status === 'error') {
                     this.updateConnectionStatus('error', msg);
                }
            });
            this.updateConnectionStatus('connected');
            
            this.loadBlocks(1);
            if (document.querySelector('#healthTab').classList.contains('active')) {
                this.initializeMetricsFlow();
            }
        } catch (error) {
            this.updateConnectionStatus('error', error.message);
            throw error;
        }
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
            this.initializeMetricsFlow();
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
        const exec = async () => {
            const { message, timestamp } = this.rpcClient.formatAdminMessage('health');
            const signature = await this.rpcClient.signMessage(message);
            const health = await this.rpcClient.getHealth(signature, timestamp);
            this.renderHealthCards(health);
        };
        await this.withRetries(exec, 3);
    }

    async loadMetricsData() {
        const exec = async () => {
            const { message, timestamp } = this.rpcClient.formatAdminMessage('metrics');
            const signature = await this.rpcClient.signMessage(message);
            const metrics = await this.rpcClient.getMetrics(signature, timestamp, true, true, true);
            this.chartManager.updateCharts(metrics);
        };
        await this.withRetries(exec, 3);
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
        document.getElementById('ceoPrivateKey').value = localStorage.getItem('ceoPrivateKey') || '';
        this.updateDerivedKey(document.getElementById('ceoPrivateKey').value);
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

    updateConnectionStatus(status, message = null) {
        const statusEl = document.getElementById('connectionStatus');
        const dot = statusEl.querySelector('.status-dot');
        const text = statusEl.querySelector('.status-text');
        
        statusEl.className = `connection-status status-${status}`;
        
        if (message) {
            text.textContent = message;
            return;
        }

        switch (status) {
            case 'connected':
                text.textContent = 'Connected';
                break;
            case 'connecting':
                text.textContent = 'Connecting to node...';
                break;
            case 'disconnected':
                text.textContent = 'Disconnected - Check if node is running';
                break;
            case 'error':
                text.textContent = message || 'Connection Error';
                break;
            default:
                text.textContent = status;
        }
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

    async initializeMetricsFlow() {
        // Require CEO private key for signed admin calls
        const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
        if (!ceoPrivateKey) {
            this.renderHealthCards({}); // keep placeholders
            this.showNotification('Configure CEO private key in Settings to fetch Health & Metrics', 'warning');
            return;
        }
        // Ensure charts are initialized
        this.chartManager.initInferenceTimeChart('inferenceTimeChart');
        this.chartManager.initCacheHitRateChart('cacheHitRateChart');
        this.chartManager.initNetworkBandwidthChart('networkBandwidthChart');
        this.chartManager.initBlockchainGrowthChart('blockchainGrowthChart');
        // Perform initial fetch
        await this.loadHealthData();
        await this.loadMetricsData();
    }

    async withRetries(fn, attempts = 3) {
        let delay = 1000;
        for (let i = 0; i < attempts; i++) {
            try { return await fn(); }
            catch (err) {
                if (i === attempts - 1) {
                    this.showNotification(err.message || 'Operation failed', 'error');
                    throw err;
                }
                await new Promise(r => setTimeout(r, delay));
                delay *= 2;
            }
        }
    }

    generateNewKeypair() {
        if (typeof nacl === 'undefined') {
            this.showNotification('tweetnacl library not loaded', 'error');
            return;
        }
        
        if (!confirm('Generate a new keypair? This will replace any existing key in the form (not saved until you click Save Settings).')) {
            return;
        }
        
        try {
            // Generate new Ed25519 keypair
            const keyPair = nacl.sign.keyPair();
            
            // Combine seed (32 bytes) + public key (32 bytes) = 64 bytes
            const privateKey = new Uint8Array(64);
            privateKey.set(keyPair.secretKey.slice(0, 32), 0); // seed
            privateKey.set(keyPair.publicKey, 32); // public key
            
            const privateKeyHex = this.bytesToHex(privateKey);
            const publicKeyHex = this.bytesToHex(keyPair.publicKey);
            
            // Update form
            document.getElementById('ceoPrivateKey').value = privateKeyHex;
            this.updateDerivedKey(privateKeyHex);
            
            this.showNotification('New keypair generated! Save settings to persist.', 'success');
            
            // Show download prompt
            this.promptKeypairDownload(privateKeyHex, publicKeyHex);
        } catch (error) {
            this.showNotification(`Key generation failed: ${error.message}`, 'error');
        }
    }

    promptKeypairDownload(privateKeyHex, publicKeyHex) {
        const data = {
            privateKey: privateKeyHex,
            publicKey: publicKeyHex,
            address: publicKeyHex,
            warning: 'KEEP THIS PRIVATE KEY SECURE! Anyone with this key has full CEO authority.',
            generated: new Date().toISOString()
        };
        
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `aigen-ceo-keypair-${Date.now()}.json`;
        a.click();
        URL.revokeObjectURL(url);
        
        this.showNotification('Keypair downloaded. Store it securely!', 'warning');
    }

    updateDerivedKey(privateKeyHex) {
        const derivedKeyInfo = document.getElementById('derivedKeyInfo');
        const derivedPublicKey = document.getElementById('derivedPublicKey');
        
        if (!privateKeyHex || privateKeyHex.trim() === '') {
            derivedKeyInfo.style.display = 'none';
            return;
        }
        
        try {
            const publicKey = this.derivePublicKey(privateKeyHex);
            derivedPublicKey.textContent = publicKey;
            derivedKeyInfo.style.display = 'block';
        } catch (error) {
            derivedKeyInfo.style.display = 'none';
        }
    }

    toggleKeyVisibility() {
        const input = document.getElementById('ceoPrivateKey');
        const button = document.getElementById('toggleKeyVisibility');
        
        if (input.type === 'password') {
            input.type = 'text';
            button.title = 'Hide key';
        } else {
            input.type = 'password';
            button.title = 'Show key';
        }
    }

    copyPublicKey() {
        const publicKey = document.getElementById('derivedPublicKey').textContent;
        
        if (publicKey === '-') {
            this.showNotification('No public key to copy', 'error');
            return;
        }
        
        navigator.clipboard.writeText(publicKey).then(() => {
            this.showNotification('Public key copied to clipboard', 'success');
        }).catch(() => {
            this.showNotification('Failed to copy to clipboard', 'error');
        });
    }
}

let dashboard;
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new AdminDashboard();
});
