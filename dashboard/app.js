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
        this.settings = null;
        this.currentBlockPage = 1;
        this.blocksPerPage = 10;
        this.autoRefreshInterval = null;
        this.currentBlockHash = null;
        
        this.init();
    }

    async init() {
        this.showLoading(true);
        
        try {
            this.settings = await this.loadSettingsAsync();
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

    async loadSettingsAsync() {
        const defaultSettings = {
            rpcUrl: 'http://127.0.0.1:9944',
            wsUrl: 'ws://127.0.0.1:9944',
            theme: 'dark',
            refreshInterval: 30
        };
        
        // Try to load from config.json via config.js global helper
        if (window.loadAigenConfig) {
            try {
                const extConfig = await window.loadAigenConfig();
                if (extConfig && extConfig.rpc) {
                    if (extConfig.rpc.http) defaultSettings.rpcUrl = extConfig.rpc.http;
                    if (extConfig.rpc.ws) defaultSettings.wsUrl = extConfig.rpc.ws;
                }
            } catch (e) {
                console.warn('Failed to load external config:', e);
            }
        }
        
        const saved = localStorage.getItem('adminSettings');
        let settings = saved ? { ...defaultSettings, ...JSON.parse(saved) } : defaultSettings;

        // Migration: Fix localhost -> 127.0.0.1 issue on Windows
        if (settings.rpcUrl === 'http://localhost:9944') {
            settings.rpcUrl = 'http://127.0.0.1:9944';
            settings.wsUrl = 'ws://127.0.0.1:9944';
            // Update storage with fixed value
            this.saveSettings(settings); 
        }

        return settings;
    }

    loadSettings() {
        // Fallback for synchronous access if needed, but init is now async
        // This is mainly kept if other methods call it, but they should use this.settings
        const defaultSettings = {
            rpcUrl: 'http://127.0.0.1:9944',
            wsUrl: 'ws://127.0.0.1:9944',
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
                const expected = this.rpcClient && this.rpcClient.ceoPublicKeyHex ? this.rpcClient.ceoPublicKeyHex : null;
                if (expected && publicKey.toLowerCase() !== expected.toLowerCase()) {
                    statusEl.classList.add('warning');
                    statusEl.classList.remove('success');
                    keyStatusMessage.textContent = `⚠️ CEO key mismatch for this node. Expected: ${this.formatHash(expected)}`;
                    keyButton.classList.remove('btn-success');
                    keyButton.classList.add('btn-warning');
                    keyStatusText.textContent = 'Fix CEO Key';
                } else {
                    keyStatusMessage.textContent = `✓ CEO key configured: ${shortKey}`;
                    keyButton.classList.remove('btn-warning');
                    keyButton.classList.add('btn-success');
                    keyStatusText.textContent = 'Keys Configured';
                }
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
        document.getElementById('useDevCeoKeyBtn').addEventListener('click', () => this.useDevCeoKey());
        document.getElementById('toggleKeyVisibility').addEventListener('click', () => this.toggleKeyVisibility());
        document.getElementById('copyPublicKeyBtn').addEventListener('click', () => this.copyPublicKey());
        document.getElementById('ceoPrivateKey').addEventListener('input', (e) => this.updateDerivedKey(e.target.value));

        document.getElementById('initModelBtn').addEventListener('click', () => this.showInitModelForm());
        document.getElementById('initModelForm').addEventListener('submit', (e) => this.submitInitModel(e));
        document.getElementById('autofillModelFromYamlBtn').addEventListener('click', () => this.autofillModelFromYaml());
        
        document.getElementById('searchButton').addEventListener('click', () => this.searchBlockOrTx());
        document.getElementById('searchInput').addEventListener('keypress', (e) => {
            if (e.key === 'Enter') this.searchBlockOrTx();
        });

        document.getElementById('prevBlocksBtn').addEventListener('click', () => this.loadBlocks(this.currentBlockPage - 1));
        document.getElementById('nextBlocksBtn').addEventListener('click', () => this.loadBlocks(this.currentBlockPage + 1));

        document.getElementById('blocksTableBody').addEventListener('click', (e) => {
            const btn = e.target.closest('button.view-block');
            if (!btn) return;
            const blockHash = btn.dataset.blockHash;
            if (!blockHash) return;
            this.loadTransactions(blockHash);
        });

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

        // Staking form
        document.getElementById('stakeForm')?.addEventListener('submit', (e) => this.submitStake(e));
        document.getElementById('unstakeBtn')?.addEventListener('click', () => this.submitUnstake());
        document.getElementById('claimStakeBtn')?.addEventListener('click', () => this.submitClaimStake());

        // CEO staker proposal controls
        document.getElementById('approveStakerProposalBtn')?.addEventListener('click', () => this.approveStakerProposal());
        document.getElementById('vetoStakerProposalBtn')?.addEventListener('click', () => this.vetoStakerProposal());

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
        } else if (tabName === 'vram') {
            this.loadVramPoolData();
            this.startAutoRefresh(30); // Auto-refresh every 30 seconds
        } else if (tabName === 'distributed-inference') {
            this.loadDistributedInferenceData();
            this.startDistributedInferenceAutoRefresh(parseInt(document.getElementById('distributedInferenceRefreshInterval').value));
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
            const block = await this.rpcClient.getBlock(null, blockHash);
            this.renderTransactionTable(block.transactions);
            const count = Array.isArray(block.transactions) ? block.transactions.length : 0;
            this.showNotification(`Loaded block transactions (${count})`, 'success');
            const txTable = document.getElementById('transactionsTableBody');
            if (txTable) txTable.scrollIntoView({ behavior: 'smooth', block: 'start' });
        } catch (error) {
            this.showNotification(`Failed to load transactions: ${error.message}`, 'error');
        }
    }

    async searchBlockOrTx() {
        const query = document.getElementById('searchInput').value.trim();
        if (!query) return;
        
        try {
            this.showLoading(true);

            const asNumber = Number(query);
            const isHeight = Number.isFinite(asNumber) && String(Math.floor(asNumber)) === query;
            const block = isHeight ? await this.rpcClient.getBlock(Math.floor(asNumber), null) : await this.rpcClient.getBlock(null, query);
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
                    <button class="btn btn-sm btn-secondary view-block" data-block-hash="${block.block_hash}">
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

            const rawHashes = document
                .getElementById('verificationHashes')
                .value
                .split(',')
                .map(h => h.trim())
                .filter(Boolean);

            const verificationHashes = rawHashes.length > 0 ? rawHashes : ['0x' + '00'.repeat(32)];

            const tierValue = parseInt(document.getElementById('minimumTier').value);
            const tierMap = { 1: 'free', 2: 'basic', 3: 'pro' };
            const minimumTier = tierMap[tierValue] || null;

            const request = {
                model_id: document.getElementById('modelId').value.trim(),
                name: document.getElementById('modelName').value.trim(),
                version: document.getElementById('modelVersion').value.trim(),
                total_size: Number.parseInt(document.getElementById('totalSize').value, 10) || 0,
                shard_count: verificationHashes.length,
                verification_hashes: verificationHashes,
                is_core_model: document.getElementById('isCoreModel').checked,
                minimum_tier: minimumTier,
                is_experimental: document.getElementById('isExperimental').checked
            };

            if (!request.model_id || !/^[A-Za-z0-9_-]+$/.test(request.model_id)) {
                throw new Error('Invalid Model ID. Use only letters, numbers, "-" and "_".');
            }

            const { message, timestamp } = this.rpcClient.formatAdminMessage('initNewModel', request);
            console.info('initNewModel', {
                rpcUrl: this.rpcClient.rpcUrl,
                networkMagic: this.rpcClient.networkMagic,
                timestamp,
                model_id: request.model_id,
                version: request.version,
                total_size: request.total_size,
                shard_count: request.shard_count,
                hashes_count: request.verification_hashes.length,
                is_core_model: request.is_core_model,
                minimum_tier: request.minimum_tier,
                is_experimental: request.is_experimental,
                message
            });
            const signature = await this.rpcClient.signMessage(message);

            await this.rpcClient.initNewModel(request, signature, timestamp);
            
            this.closeModals();
            this.loadModels();
            this.showNotification('Model initialized successfully', 'success');
        } catch (error) {
            this.showNotification(`Initialization failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async autofillModelFromYaml() {
        const fileInput = document.getElementById('modelYamlFile');
        const file = fileInput.files && fileInput.files[0];
        if (!file) {
            this.showNotification('Select a model.yml file first', 'warning');
            return;
        }

        const text = await file.text();
        const data = {};
        for (const line of text.split(/\r?\n/)) {
            const trimmed = line.trim();
            if (!trimmed || trimmed.startsWith('#')) continue;
            const idx = trimmed.indexOf(':');
            if (idx === -1) continue;
            const key = trimmed.slice(0, idx).trim();
            let value = trimmed.slice(idx + 1).trim();
            if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
                value = value.slice(1, -1);
            }
            if (/^\d+$/.test(value)) value = Number(value);
            if (value === 'true') value = true;
            if (value === 'false') value = false;
            data[key] = value;
        }

        const name = typeof data.name === 'string' ? data.name : '';
        const version = typeof data.version === 'string' ? data.version : '';
        const sizeBytes = typeof data.size_bytes === 'number' ? data.size_bytes : 0;

        const slug = (name || 'model')
            .toLowerCase()
            .replace(/\./g, '-')
            .replace(/[^a-z0-9_\- ]/g, '')
            .trim()
            .replace(/\s+/g, '-');

        document.getElementById('modelId').value = slug;
        document.getElementById('modelName').value = name || slug;
        document.getElementById('modelVersion').value = version || '1.0.0';
        if (sizeBytes) document.getElementById('totalSize').value = String(sizeBytes);

        if (!document.getElementById('verificationHashes').value.trim()) {
            document.getElementById('verificationHashes').value = '0x' + '00'.repeat(32);
        }
        document.getElementById('shardCount').value = '1';

        this.showNotification('Model fields auto-filled from YAML', 'success');
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
            this.renderProposalsTable([]);
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
            const health = await this.rpcClient.testHealth(); // public method: health
            const chainInfo = await this.rpcClient.getChainInfo();
            const pending = await this.rpcClient.getPendingTransactions(50);
            const combined = {
                node_health: {
                    status: health.status,
                    peer_count: health.peer_count,
                    sync_status: health.sync_status
                },
                ai_health: {
                    core_model_loaded: false,
                    cache_hit_rate: 0,
                    batch_processing: 'Idle'
                },
                blockchain_health: {
                    chain_height: chainInfo.height,
                    pending_transactions: Array.isArray(pending) ? pending.length : 0,
                    shutdown_active: health.shutdown_active
                }
            };
            this.renderHealthCards(combined);
        };
        await this.withRetries(exec, 3);
    }

    async loadMetricsData() {
        const exec = async () => {
            const chainInfo = await this.rpcClient.getChainInfo();
            const pending = await this.rpcClient.getPendingTransactions(50);
            const metrics = {
                blockchain_metrics: {
                    total_blocks: chainInfo.height + 1,
                    total_transactions: Array.isArray(pending) ? pending.length : 0
                }
            };
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
            updateCard('nodeHealthCards', 2, 'Block Sync', health.node_health.sync_status || '-', 'healthy');
        }
        
        if (health.ai_health) {
            updateCard('aiHealthCards', 0, 'Inference Service', health.ai_health.core_model_loaded ? 'Active' : 'Inactive', health.ai_health.core_model_loaded ? 'healthy' : 'warning');
            updateCard('aiHealthCards', 1, 'Cache Hit Rate', `${(health.ai_health.cache_hit_rate * 100).toFixed(1)}%`, 'healthy');
            updateCard('aiHealthCards', 2, 'Batch Processing', health.ai_health.batch_processing || '-', 'healthy');
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
            // Fetch proposals
            const proposalsData = await this.rpcClient.listProposals(null);
            this.renderGovernanceTable(proposalsData.proposals || []);
            
            // Fetch stakes
            await this.loadStakes();
        } catch (error) {
            this.showNotification(`Failed to load governance data: ${error.message}`, 'error');
            // Fallback to empty state
            this.renderGovernanceTable([]);
        }
    }

    async loadStakes() {
        try {
            const stakesData = await this.rpcClient.listStakes(null);
            const stakes = Array.isArray(stakesData?.stakes) ? stakesData.stakes : [];
            this.renderStakesTable(stakes);
            
            // Update overview cards
            const totalStaked = stakes.reduce((sum, s) => sum + s.staked_amount, 0);
            document.querySelector('#totalStakedCard .stat-value').textContent = 
                this.formatAmount(totalStaked);
            document.querySelector('#totalStakersCard .stat-value').textContent = 
                stakesData?.total_count || 0;
        } catch (error) {
            this.showNotification(`Failed to load stakes: ${error.message}`, 'error');
        }
    }

    async loadProposalVotes(proposalId) {
        try {
            const votesData = await this.rpcClient.listVotes(proposalId);
            this.renderVotesTable(votesData.votes || [], votesData.tally);
            
            // Show votes section
            document.getElementById('proposalVotesSection').style.display = 'block';
            document.getElementById('selectedProposalId').textContent = proposalId;
        } catch (error) {
            this.showNotification(`Failed to load votes: ${error.message}`, 'error');
        }
    }

    renderGovernanceTable(proposals) {
        const tbody = document.getElementById('governanceTableBody');
        
        if (!proposals || proposals.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">No proposals found</td></tr>';
            return;
        }
        
        tbody.innerHTML = proposals.map(proposal => `
            <tr>
                <td><code class="hash">${this.formatHash(proposal.proposal_id)}</code></td>
                <td>${proposal.model_id ? 'Model Upgrade' : 'General'}</td>
                <td>${proposal.description || `${proposal.current_version} → ${proposal.new_version}`}</td>
                <td><span class="status-badge status-${proposal.status.toLowerCase()}">${proposal.status}</span></td>
                <td>
                    <button class="btn btn-sm btn-primary" onclick="dashboard.loadProposalVotes('${proposal.proposal_id}')">
                        View Votes
                    </button>
                    ${proposal.status === 'Pending' ? `
                        <button class="btn btn-sm btn-success" onclick="dashboard.voteOnProposal('${proposal.proposal_id}', 'Approve')">
                            Vote Approve
                        </button>
                        <button class="btn btn-sm btn-danger" onclick="dashboard.voteOnProposal('${proposal.proposal_id}', 'Reject')">
                            Vote Reject
                        </button>
                    ` : ''}
                </td>
            </tr>
        `).join('');
    }

    renderStakesTable(stakes) {
        const tbody = document.getElementById('stakesTableBody');
        
        if (!stakes || stakes.length === 0) {
            tbody.innerHTML = '<tr><td colspan="6" class="loading-cell">No stakers found</td></tr>';
            return;
        }
        
        const totalStake = stakes.reduce((sum, s) => sum + s.staked_amount, 0);
        
        tbody.innerHTML = stakes.map(stake => {
            const votingPower = totalStake > 0 
                ? ((stake.staked_amount / totalStake) * 100).toFixed(2) 
                : '0.00';
            
            return `
                <tr>
                    <td><code class="hash">${this.formatHash(stake.staker_address)}</code></td>
                    <td>${this.formatAmount(stake.staked_amount)} AIGEN</td>
                    <td><span class="badge badge-${stake.role.toLowerCase()}">${stake.role}</span></td>
                    <td>${votingPower}%</td>
                    <td>${this.formatAmount(stake.total_rewards_claimed)} AIGEN</td>
                    <td>${this.formatTimestamp(stake.staked_since)}</td>
                </tr>
            `;
        }).join('');
    }

    renderVotesTable(votes, tally) {
        const tbody = document.getElementById('votesTableBody');
        
        if (!votes || votes.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="loading-cell">No votes yet</td></tr>';
            return;
        }
        
        // Update tally cards
        document.getElementById('approveWeight').textContent = this.formatAmount(tally.total_approve_weight);
        document.getElementById('approvePercentage').textContent = `${tally.approval_percentage.toFixed(1)}%`;
        document.getElementById('rejectWeight').textContent = this.formatAmount(tally.total_reject_weight);
        const rejectPct = tally.total_voting_power > 0 
            ? ((tally.total_reject_weight / tally.total_voting_power) * 100).toFixed(1) 
            : '0.0';
        document.getElementById('rejectPercentage').textContent = `${rejectPct}%`;
        document.getElementById('abstainWeight').textContent = this.formatAmount(tally.total_abstain_weight);
        const abstainPct = tally.total_voting_power > 0 
            ? ((tally.total_abstain_weight / tally.total_voting_power) * 100).toFixed(1) 
            : '0.0';
        document.getElementById('abstainPercentage').textContent = `${abstainPct}%`;
        
        tbody.innerHTML = votes.map(vote => `
            <tr>
                <td><code class="hash">${this.formatHash(vote.voter_address)}</code></td>
                <td><span class="badge badge-${vote.vote.toLowerCase()}">${vote.vote}</span></td>
                <td>${this.formatAmount(vote.stake_weight)} AIGEN</td>
                <td>${vote.comment || '-'}</td>
                <td>${this.formatTimestamp(vote.timestamp)}</td>
            </tr>
        `).join('');
    }

    async submitVote(e) {
        e.preventDefault();
        
        try {
            this.showLoading(true);
            
            const proposalId = document.getElementById('voteProposalId').value;
            const voteType = document.getElementById('voteType').value;
            const comment = document.getElementById('voteComment').value;
            
            // Capitalize first letter to match governance API (Approve/Reject/Abstain)
            const vote = voteType.charAt(0).toUpperCase() + voteType.slice(1);
            
            const voterAddress = this.rpcClient.derivePublicKeyFromStoredKey();
            const timestamp = Math.floor(Date.now() / 1000);
            const message = `vote:${proposalId}:${vote}:${timestamp}`;
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.submitVote(
                proposalId, 
                voterAddress, 
                vote, 
                comment, 
                signature,
                timestamp
            );
            
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

    async submitStake(e) {
        e.preventDefault();
        
        const amount = parseInt(document.getElementById('stakeAmount').value);
        const role = document.getElementById('stakeRole').value;
        
        // Validate minimum amounts
        const minAmounts = { 'Inference': 1000, 'Training': 5000, 'Both': 5000 };
        if (amount < minAmounts[role]) {
            this.showNotification(`Minimum stake for ${role}: ${minAmounts[role]} AIGEN`, 'error');
            return;
        }
        
        try {
            this.showLoading(true);
            
            const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
            if (!ceoPrivateKey) {
                throw new Error('CEO private key required for staking');
            }
            
            // Derive staker address from CEO key
            const stakerAddress = this.rpcClient.derivePublicKeyFromStoredKey();
            if (!stakerAddress) {
                throw new Error('Failed to derive staker address');
            }
            
            // Sign stake transaction
            const timestamp = Math.floor(Date.now() / 1000);
            const message = `stake:${this.rpcClient.networkMagic}:${timestamp}:${stakerAddress}:${amount}:${role}`;
            const signature = await this.rpcClient.signMessage(message);
            
            const result = await this.rpcClient.submitStakeTx(stakerAddress, amount, role, signature, timestamp);
            
            this.showNotification(result.message, 'success');
            document.getElementById('stakeForm').reset();
            await this.loadStakes();
        } catch (error) {
            this.showNotification(`Stake failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async submitUnstake() {
        const amount = prompt('Enter amount to unstake (AIGEN):');
        if (!amount || isNaN(amount) || parseInt(amount) <= 0) return;
        
        try {
            this.showLoading(true);
            
            const stakerAddress = this.rpcClient.derivePublicKeyFromStoredKey();
            const timestamp = Math.floor(Date.now() / 1000);
            const message = `unstake:${this.rpcClient.networkMagic}:${timestamp}:${stakerAddress}:${amount}`;
            const signature = await this.rpcClient.signMessage(message);
            
            const result = await this.rpcClient.submitUnstakeTx(stakerAddress, parseInt(amount), signature, timestamp);
            
            this.showNotification(result.message, 'success');
            await this.loadStakes();
        } catch (error) {
            this.showNotification(`Unstake failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async submitClaimStake() {
        if (!confirm('Claim all unstaked tokens that have passed the 7-day cooldown?')) return;
        
        try {
            this.showLoading(true);
            
            const stakerAddress = this.rpcClient.derivePublicKeyFromStoredKey();
            const timestamp = Math.floor(Date.now() / 1000);
            const message = `claim_stake:${this.rpcClient.networkMagic}:${timestamp}:${stakerAddress}`;
            const signature = await this.rpcClient.signMessage(message);
            
            const result = await this.rpcClient.submitClaimStakeTx(stakerAddress, signature, timestamp);
            
            this.showNotification(result.message, 'success');
            await this.loadStakes();
        } catch (error) {
            this.showNotification(`Claim failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async voteOnProposal(proposalId, voteType) {
        const comment = prompt(`Comment for your ${voteType} vote (optional):`);
        
        try {
            this.showLoading(true);
            
            const voterAddress = this.rpcClient.derivePublicKeyFromStoredKey();
            const voterPublicKey = voterAddress; // Same as address for Ed25519
            const timestamp = Math.floor(Date.now() / 1000);
            const message = `vote:${proposalId}:${voteType}:${timestamp}`;
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.submitVote(
                proposalId, 
                voterAddress, 
                voteType, 
                comment || '', 
                signature,
                timestamp
            );
            
            this.showNotification(`Vote submitted: ${voteType}`, 'success');
            await this.loadProposalVotes(proposalId);
            await this.loadGovernanceProposals();
        } catch (error) {
            this.showNotification(`Vote failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async approveStakerProposal() {
        const proposalId = document.getElementById('stakerProposalId').value;
        if (!proposalId) {
            this.showNotification('Please enter a proposal ID', 'error');
            return;
        }
        
        try {
            this.showLoading(true);
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('approveStakerProposal', { proposalId });
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.approveStakerProposal(proposalId, signature, timestamp);
            
            this.showNotification('Staker proposal approved', 'success');
            await this.loadGovernanceProposals();
        } catch (error) {
            this.showNotification(`Approval failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async vetoStakerProposal() {
        const proposalId = document.getElementById('stakerProposalId').value;
        if (!proposalId) {
            this.showNotification('Please enter a proposal ID', 'error');
            return;
        }
        
        const reason = prompt('Enter veto reason:');
        if (!reason) return;
        
        if (!confirm(`Veto proposal ${proposalId}? This cannot be undone.`)) return;
        
        try {
            this.showLoading(true);
            
            const { message, timestamp } = this.rpcClient.formatAdminMessage('vetoStakerProposal', { proposalId, reason });
            const signature = await this.rpcClient.signMessage(message);
            
            await this.rpcClient.vetoStakerProposal(proposalId, reason, signature, timestamp);
            
            this.showNotification('Staker proposal vetoed', 'success');
            await this.loadGovernanceProposals();
        } catch (error) {
            this.showNotification(`Veto failed: ${error.message}`, 'error');
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
        // Ensure charts are initialized
        this.chartManager.initAllCharts();
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

    useDevCeoKey() {
        try {
            if (typeof nacl === 'undefined') {
                throw new Error('tweetnacl library not loaded');
            }

            const seedHex = '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';
            const seedBytes = this.hexToBytes(seedHex);
            if (seedBytes.length !== 32) {
                throw new Error('Dev seed must be 32 bytes');
            }

            const keyPair = nacl.sign.keyPair.fromSeed(seedBytes);
            const privateKey = new Uint8Array(64);
            privateKey.set(seedBytes, 0);
            privateKey.set(keyPair.publicKey, 32);
            const privateKeyHex = this.bytesToHex(privateKey);

            const input = document.getElementById('ceoPrivateKey');
            input.value = privateKeyHex;
            localStorage.setItem('ceoPrivateKey', privateKeyHex);
            this.updateDerivedKey(privateKeyHex);
            this.checkCeoKeyStatus();
            this.showNotification('Dev CEO key loaded', 'success');
        } catch (e) {
            this.showNotification(`Failed to load dev key: ${e.message}`, 'error');
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

    // NEW: VRAM Pool Dashboard Methods
    async loadVramPoolData() {
        try {
            const [poolStats, nodeMetrics, rewardHistory] = await Promise.all([
                this.rpcClient.getVramPoolStats(),
                this.rpcClient.getNodeMetrics(),
                this.rpcClient.getRewardLeaderboard(100)
            ]);
            
            this.renderVramPoolStats(poolStats);
            this.renderVramNodesTable(nodeMetrics);
            this.renderRewardLeaderboard(rewardHistory);
            this.renderTopologyGraph(nodeMetrics);
        } catch (error) {
            console.error('Failed to load VRAM pool data:', error);
            this.showNotification('Failed to load VRAM pool data', 'error');
        }
    }

    renderVramPoolStats(data) {
        const vramPoolStats = document.getElementById('vramPoolStats');
        const activeNodesCount = document.getElementById('activeNodesCount');
        const fragmentDistribution = document.getElementById('fragmentDistribution');
        
        if (!data) {
            vramPoolStats.innerHTML = '<p>No data available</p>';
            return;
        }
        
        const totalVram = data.total_vram_gb || 0;
        const allocatedVram = data.allocated_vram_gb || 0;
        const freeVram = totalVram - allocatedVram;
        const utilization = totalVram > 0 ? ((allocatedVram / totalVram) * 100).toFixed(1) : 0;
        
        vramPoolStats.innerHTML = `
            <div class="stat-content">
                <div class="stat-row">
                    <span>Total VRAM:</span>
                    <span class="stat-value">${totalVram.toFixed(2)} GB</span>
                </div>
                <div class="stat-row">
                    <span>Allocated:</span>
                    <span class="stat-value">${allocatedVram.toFixed(2)} GB (${utilization}%)</span>
                </div>
                <div class="stat-row">
                    <span>Free:</span>
                    <span class="stat-value">${freeVram.toFixed(2)} GB</span>
                </div>
            </div>
        `;
        
        activeNodesCount.innerHTML = `
            <div class="stat-content">
                <div class="big-number">${data.active_nodes || 0}</div>
                <div class="stat-label">nodes online</div>
            </div>
        `;
        
        fragmentDistribution.innerHTML = `
            <div class="stat-content">
                <div class="stat-row">
                    <span>Total Fragments:</span>
                    <span class="stat-value">${data.total_fragments || 0}</span>
                </div>
                <div class="stat-row">
                    <span>Avg Replicas:</span>
                    <span class="stat-value">${(data.avg_replicas || 0).toFixed(1)}</span>
                </div>
            </div>
        `;
    }

    renderVramNodesTable(nodes) {
        const tbody = document.getElementById('vramNodesTableBody');
        
        if (!nodes || nodes.length === 0) {
            tbody.innerHTML = '<tr><td colspan="7" class="loading-cell">No nodes available</td></tr>';
            return;
        }
        
        tbody.innerHTML = nodes.map(node => `
            <tr>
                <td><code class="hash">${this.formatHash(node.node_id)}</code></td>
                <td>${(node.vram_total_gb || 0).toFixed(2)} GB</td>
                <td>${(node.vram_free_gb || 0).toFixed(2)} GB</td>
                <td>${node.fragments_hosted || 0}</td>
                <td>${this.formatAmount(node.stake || 0)} AIGEN</td>
                <td>${this.formatAmount(node.total_earned || 0)} AIGEN</td>
                <td><span class="status-badge status-${node.status || 'offline'}">${node.status || 'offline'}</span></td>
            </tr>
        `).join('');
    }

    renderRewardLeaderboard(rewards) {
        const tbody = document.getElementById('rewardLeaderboardBody');
        
        if (!rewards || rewards.length === 0) {
            tbody.innerHTML = '<tr><td colspan="6" class="loading-cell">No rewards data available</td></tr>';
            return;
        }
        
        tbody.innerHTML = rewards.map((entry, i) => `
            <tr>
                <td>${i + 1}</td>
                <td><code class="hash">${this.formatHash(entry.node_id)}</code></td>
                <td>${this.formatAmount(entry.total_earned || 0)} AIGEN</td>
                <td>${this.formatAmount(entry.compute_rewards || 0)} AIGEN</td>
                <td>${this.formatAmount(entry.storage_rewards || 0)} AIGEN</td>
                <td>${entry.tasks_completed || 0}</td>
            </tr>
        `).join('');
    }

    renderTopologyGraph(nodes) {
        const canvas = document.getElementById('topologyCanvas');
        if (!canvas || !nodes || nodes.length === 0) return;
        
        const ctx = canvas.getContext('2d');
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        
        // Simple force-directed-like visualization
        const centerX = canvas.width / 2;
        const centerY = canvas.height / 2;
        const radius = Math.min(centerX, centerY) * 0.7;
        
        nodes.forEach((node, i) => {
            const angle = (i / nodes.length) * 2 * Math.PI;
            const x = centerX + Math.cos(angle) * radius;
            const y = centerY + Math.sin(angle) * radius;
            
            // Draw node circle sized by VRAM
            const nodeRadius = 10 + (node.vram_total_gb || 0) / 10;
            
            // Color by load score (green = low, red = high)
            const loadScore = node.load_score || 0;
            const hue = 120 - (loadScore * 120); // 120 (green) to 0 (red)
            
            ctx.beginPath();
            ctx.arc(x, y, nodeRadius, 0, 2 * Math.PI);
            ctx.fillStyle = `hsl(${hue}, 70%, 50%)`;
            ctx.fill();
            ctx.strokeStyle = '#333';
            ctx.lineWidth = 2;
            ctx.stroke();
            
            // Draw label
            ctx.fillStyle = '#fff';
            ctx.font = '10px monospace';
            ctx.textAlign = 'center';
            ctx.fillText(node.node_id.substring(0, 8), x, y + nodeRadius + 12);
        });
    }

    // NEW: Distributed Inference Dashboard Methods
    async loadDistributedInferenceData() {
        try {
            const [stats, activePipelines, blockMetrics, compressionStats] = await Promise.all([
                this.rpcClient.getDistributedInferenceStats(),
                this.rpcClient.getActivePipelines(50),
                this.rpcClient.getBlockLatencyMetrics(),
                this.rpcClient.getCompressionStats()
            ]);
            
            this.renderDistributedInferenceStats(stats);
            this.renderActivePipelinesTable(activePipelines);
            this.renderBlockLatencyChart(blockMetrics);
            this.renderCompressionStatsCard(compressionStats);
        } catch (error) {
            console.error('Failed to load distributed inference data:', error);
            this.showNotification('Failed to load distributed inference data', 'error');
        }
    }

    renderDistributedInferenceStats(stats) {
        const activePipelinesCard = document.getElementById('activePipelinesCard');
        const pipelineLatencyCard = document.getElementById('pipelineLatencyCard');
        const throughputCard = document.getElementById('throughputCard');
        const compressionRatioCard = document.getElementById('compressionRatioCard');
        
        if (activePipelinesCard) {
            activePipelinesCard.innerHTML = `
                <h3>Active Pipelines</h3>
                <div class="stat-value">${stats?.active_pipelines || 0}</div>
                <div class="stat-label">${stats?.total_inferences_completed || 0} completed</div>
            `;
        }
        
        if (pipelineLatencyCard) {
            const latency = stats?.avg_pipeline_latency_ms || 0;
            pipelineLatencyCard.innerHTML = `
                <h3>Avg Pipeline Latency</h3>
                <div class="stat-value">${latency.toFixed(0)}</div>
                <div class="stat-unit">ms</div>
            `;
        }
        
        if (throughputCard) {
            const throughput = stats?.avg_throughput_tasks_per_sec || 0;
            throughputCard.innerHTML = `
                <h3>Throughput</h3>
                <div class="stat-value">${throughput.toFixed(2)}</div>
                <div class="stat-unit">tasks/sec</div>
            `;
        }
        
        if (compressionRatioCard) {
            const ratio = stats?.avg_compression_ratio || 1.0;
            compressionRatioCard.innerHTML = `
                <h3>Compression Ratio</h3>
                <div class="stat-value">${ratio.toFixed(2)}</div>
                <div class="stat-unit">x</div>
            `;
        }
    }

    renderActivePipelinesTable(pipelines) {
        const tbody = document.getElementById('activePipelinesTableBody');
        
        if (!pipelines || pipelines.length === 0) {
            tbody.innerHTML = '<tr><td colspan="7" class="loading-cell">No active pipelines</td></tr>';
            return;
        }
        
        tbody.innerHTML = pipelines.map(pipeline => {
            const routeStr = pipeline.pipeline_route ? pipeline.pipeline_route.join(' → ') : '-';
            const progress = pipeline.pipeline_route && pipeline.pipeline_route.length > 0
                ? Math.round(((pipeline.current_block + 1) / pipeline.pipeline_route.length) * 100)
                : 0;
            
            return `
                <tr>
                    <td><code class="hash">${this.formatHash(pipeline.inference_id)}</code></td>
                    <td>${pipeline.model_id || '-'}</td>
                    <td><span class="route-path">${routeStr}</span></td>
                    <td>${pipeline.current_block !== undefined ? pipeline.current_block : '-'}</td>
                    <td>
                        <div class="progress-bar">
                            <div class="progress-fill" style="width: ${progress}%"></div>
                        </div>
                        <span class="progress-text">${progress}%</span>
                    </td>
                    <td>${this.formatDuration(pipeline.elapsed_ms)}</td>
                    <td>${this.formatDuration(pipeline.estimated_remaining_ms)}</td>
                </tr>
            `;
        }).join('');
    }

    renderBlockLatencyChart(metrics) {
        const canvas = document.getElementById('blockLatencyChart');
        if (!canvas) return;
        
        // Use Chart.js if available, otherwise skip
        if (typeof Chart === 'undefined' || !metrics || metrics.length === 0) return;
        
        const ctx = canvas.getContext('2d');
        
        // Destroy existing chart if any
        if (canvas._chart) {
            canvas._chart.destroy();
        }
        
        const labels = metrics.map(m => `Block ${m.block_id}`);
        const computeData = metrics.map(m => m.avg_compute_time_ms || 0);
        const transferData = metrics.map(m => m.avg_transfer_time_ms || 0);
        
        canvas._chart = new Chart(ctx, {
            type: 'bar',
            data: {
                labels: labels,
                datasets: [
                    {
                        label: 'Compute Time (ms)',
                        data: computeData,
                        backgroundColor: 'rgba(54, 162, 235, 0.8)',
                        borderColor: 'rgba(54, 162, 235, 1)',
                        borderWidth: 1
                    },
                    {
                        label: 'Transfer Time (ms)',
                        data: transferData,
                        backgroundColor: 'rgba(255, 99, 132, 0.8)',
                        borderColor: 'rgba(255, 99, 132, 1)',
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    y: {
                        beginAtZero: true,
                        title: {
                            display: true,
                            text: 'Time (ms)'
                        }
                    }
                },
                plugins: {
                    title: {
                        display: true,
                        text: 'Per-Block Latency Breakdown'
                    }
                }
            }
        });
    }

    renderCompressionStatsCard(stats) {
        const card = document.getElementById('compressionStatsCard');
        if (!card) return;
        
        if (!stats) {
            card.innerHTML = '<div class="stat-content"><p>No compression data available</p></div>';
            return;
        }
        
        const ratio = stats.avg_compression_ratio || 1.0;
        const bytesSaved = stats.total_bytes_saved || 0;
        const tensorsCompressed = stats.total_tensors_compressed || 0;
        const quantizationStatus = stats.quantization_enabled ? 'Enabled' : 'Disabled';
        
        card.innerHTML = `
            <div class="stat-content">
                <div class="stat-row">
                    <span>Avg Compression:</span>
                    <span class="stat-value">${ratio.toFixed(2)}x</span>
                </div>
                <div class="stat-row">
                    <span>Bytes Saved:</span>
                    <span class="stat-value">${this.formatBytes(bytesSaved)}</span>
                </div>
                <div class="stat-row">
                    <span>Tensors Compressed:</span>
                    <span class="stat-value">${tensorsCompressed.toLocaleString()}</span>
                </div>
                <div class="stat-row">
                    <span>8-bit Quantization:</span>
                    <span class="stat-value">${quantizationStatus}</span>
                </div>
            </div>
        `;
    }

    formatDuration(ms) {
        if (!ms || ms === 0) return '-';
        if (ms < 1000) return `${ms}ms`;
        if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
        return `${(ms / 60000).toFixed(1)}m`;
    }

    startDistributedInferenceAutoRefresh(intervalSeconds) {
        // Store interval ID for cleanup
        if (this.distributedInferenceRefreshInterval) {
            clearInterval(this.distributedInferenceRefreshInterval);
        }
        
        if (intervalSeconds > 0) {
            this.distributedInferenceRefreshInterval = setInterval(() => {
                this.loadDistributedInferenceData();
            }, intervalSeconds * 1000);
        }
    }

    stopDistributedInferenceAutoRefresh() {
        if (this.distributedInferenceRefreshInterval) {
            clearInterval(this.distributedInferenceRefreshInterval);
            this.distributedInferenceRefreshInterval = null;
        }
    }
}

let dashboard;
document.addEventListener('DOMContentLoaded', () => {
    dashboard = new AdminDashboard();
    window.dashboard = dashboard;
});
