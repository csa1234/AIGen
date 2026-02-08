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

class AIGENRPCClient {
    constructor(rpcUrl, wsUrl, onStatusUpdate = null) {
        this.rpcUrl = rpcUrl;
        this.wsUrl = wsUrl;
        this.ws = null;
        this.messageQueue = [];
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        this.reconnectDelay = 500;
        this.isConnected = false;
        this.subscriptions = new Map();
        this.requestId = 0;
        this.pendingRequests = new Map();
        this.onStatusUpdate = onStatusUpdate;
    }

    async connect() {
        try {
            if (this.onStatusUpdate) this.onStatusUpdate('connecting');
            await this.checkRPCConnection();
            await this.connectWebSocket();
            this.isConnected = true;
            this.reconnectAttempts = 0;
            if (this.onStatusUpdate) this.onStatusUpdate('connected');
            return true;
        } catch (error) {
            console.error('Connection failed:', error);
            this.isConnected = false;
            if (this.onStatusUpdate) this.onStatusUpdate('disconnected');
            throw error;
        }
    }

    async checkRPCConnection() {
        try {
            const response = await fetch(this.rpcUrl, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    id: ++this.requestId,
                    method: 'health',
                    params: []
                })
            });

            if (!response.ok) {
                throw new Error(`RPC connection failed: ${response.status}`);
            }

            const data = await response.json();
            if (data.error) {
                throw new Error(`RPC error: ${data.error.message}`);
            }
        } catch (error) {
            throw new Error(`Failed to connect to RPC: ${error.message}`);
        }
    }

    async connectWebSocket() {
        return new Promise((resolve, reject) => {
            try {
                this.ws = new WebSocket(this.wsUrl);
                const timeout = setTimeout(() => reject(new Error('WebSocket connection timeout')), 4000);
                
                this.ws.onopen = () => {
                    console.log('WebSocket connected');
                    clearTimeout(timeout);
                    this.setupWebSocketHandlers();
                    resolve();
                };

                this.ws.onerror = (error) => {
                    console.error('WebSocket error:', error);
                    clearTimeout(timeout);
                    reject(new Error('WebSocket connection failed'));
                };

                this.ws.onclose = () => {
                    this.isConnected = false;
                    this.handleDisconnection();
                };
            } catch (error) {
                reject(error);
            }
        });
    }

    setupWebSocketHandlers() {
        this.ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                this.handleWebSocketMessage(data);
            } catch (error) {
                console.error('Failed to parse WebSocket message:', error);
            }
        };

        this.ws.onclose = () => {
            this.isConnected = false;
            this.handleDisconnection();
        };
    }

    handleWebSocketMessage(data) {
        if (data && typeof data.id !== 'undefined' && this.pendingRequests.has(data.id)) {
            const { resolve, reject } = this.pendingRequests.get(data.id);
            this.pendingRequests.delete(data.id);
            if (data.error) reject(new Error(data.error.message || 'RPC error'));
            else resolve(data.result);
            return;
        }

        if (data && data.params && data.params.subscription) {
            const callback = this.subscriptions.get(data.params.subscription);
            if (callback) callback(data.params.result);
        }
    }

    handleDisconnection() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const exp = Math.min(30000, this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1));
            const jitter = Math.floor(Math.random() * 250);
            const waitMs = exp + jitter;
            console.log(`Reconnecting... Attempt ${this.reconnectAttempts} in ${waitMs}ms`);
            if (this.onStatusUpdate) this.onStatusUpdate('connecting');
            setTimeout(async () => {
                try {
                    await this.checkRPCConnection();
                    await this.connectWebSocket();
                    this.isConnected = true;
                    this.reconnectAttempts = 0;
                    if (this.onStatusUpdate) this.onStatusUpdate('connected');
                } catch (error) {
                    console.error('Reconnection failed:', error);
                    this.isConnected = false;
                    if (this.onStatusUpdate) this.onStatusUpdate('disconnected');
                    this.handleDisconnection();
                }
            }, waitMs);
        } else {
            if (this.onStatusUpdate) this.onStatusUpdate('disconnected');
        }
    }

    async callWs(method, params = []) {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            throw new Error('WebSocket not connected');
        }
        const id = ++this.requestId;
        const payload = { jsonrpc: '2.0', id, method, params };
        return new Promise((resolve, reject) => {
            this.pendingRequests.set(id, { resolve, reject });
            try {
                this.ws.send(JSON.stringify(payload));
            } catch (e) {
                this.pendingRequests.delete(id);
                reject(e);
            }
        });
    }

    async callHttp(method, params = []) {
        const payload = {
            jsonrpc: '2.0',
            id: ++this.requestId,
            method,
            params
        };
        const response = await fetch(this.rpcUrl, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });
        const data = await response.json();
        if (data.error) {
            throw new Error(data.error.message || 'RPC error');
        }
        return data.result;
    }

    async listModels(userAddress = null) {
        const res = await this.callHttp('listModels', [userAddress]);
        if (Array.isArray(res)) return res;
        if (res && Array.isArray(res.models)) return res.models;
        return [];
    }

    async chatCompletion(messages, options = {}) {
        const request = {
            messages,
            model_id: options.modelId || 'mistral-7b',
            stream: Boolean(options.stream),
            max_tokens: options.maxTokens || 2048,
            temperature: options.temperature || 0.7,
            user_address: options.userAddress || null
        };
        if (options.transaction) request.transaction = options.transaction;

        try {
            return await this.callHttp('chatCompletion', [request]);
        } catch (error) {
            throw new Error(`Chat completion failed: ${error.message}`);
        }
    }

    async subscribeChatCompletion(messages, options = {}, onChunk, onComplete, onError) {
        if (!this.isConnected) {
            throw new Error('WebSocket not connected');
        }
        const request = {
            messages,
            model_id: options.modelId || 'mistral-7b',
            stream: true,
            max_tokens: options.maxTokens || 2048,
            temperature: options.temperature || 0.7,
            user_address: options.userAddress || null
        };
        if (options.transaction) request.transaction = options.transaction;

        const subscriptionId = await this.callWs('subscribeChatCompletion', [request]);

        this.subscriptions.set(subscriptionId, (result) => {
            if (result.error) {
                if (onError) {
                    onError(new Error(`${result.error.message} (code: ${result.error.code})`));
                }
                return;
            }

            if (result.finish_reason) {
                if (onComplete) {
                    onComplete();
                }
                this.subscriptions.delete(subscriptionId);
            } else if (result.delta && onChunk) {
                onChunk(result);
            }
        });

        return subscriptionId;
    }

    async checkQuota(userAddress) {
        try {
            if (!userAddress) {
                throw new Error('user_address is required');
            }
            return await this.callHttp('checkQuota', [{ user_address: userAddress }]);
        } catch (error) {
            throw new Error(`Failed to check quota: ${error.message}`);
        }
    }

    async getTierInfo() {
        try {
            return await this.callHttp('getTierInfo', []);
        } catch (error) {
            throw new Error(`Failed to get tier info: ${error.message}`);
        }
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        this.isConnected = false;
        this.subscriptions.clear();
    }
}

class AIGENPlayground {
    constructor() {
        this.client = null;
        this.chatHistory = [];
        this.currentModel = 'mistral-7b';
        this.currentTier = 'free';
        this.isStreaming = true;
        this.settings = null;
        this.currentStreamingMessage = null;
        
        this.initializeElements();
        this.setupEventListeners();
        this.settings = this.loadSettings();
        this.connect();
    }

    initializeElements() {
        this.elements = {
            connectionStatus: document.getElementById('connectionStatus'),
            connectionText: document.getElementById('connectionText'),
            quotaValue: document.getElementById('quotaValue'),
            chatMessages: document.getElementById('chatMessages'),
            chatInput: document.getElementById('chatInput'),
            sendBtn: document.getElementById('sendBtn'),
            streamingToggle: document.getElementById('streamingToggle'),
            tokenCount: document.getElementById('tokenCount'),
            modelSelector: document.getElementById('modelSelector'),
            tierSelector: document.getElementById('tierSelector'),
            rpcUrl: document.getElementById('rpcUrl'),
            wsUrl: document.getElementById('wsUrl'),
            settingsBtn: document.getElementById('settingsBtn'),
            settingsModal: document.getElementById('settingsModal'),
            modalClose: document.getElementById('modalClose'),
            saveSettingsBtn: document.getElementById('saveSettingsBtn'),
            resetSettingsBtn: document.getElementById('resetSettingsBtn'),
            newChatBtn: document.getElementById('newChatBtn'),
            exportBtn: document.getElementById('exportBtn'),
            loadingOverlay: document.getElementById('loadingOverlay')
        };
    }

    setupEventListeners() {
        this.elements.sendBtn.addEventListener('click', () => this.sendMessage());
        this.elements.chatInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                this.sendMessage();
            }
        });
        this.elements.chatInput.addEventListener('input', () => this.updateTokenCount());
        
        this.elements.streamingToggle.addEventListener('change', (e) => {
            this.isStreaming = e.target.checked;
        });

        this.elements.modelSelector.addEventListener('change', (e) => {
            this.currentModel = e.target.value;
        });

        this.elements.tierSelector.addEventListener('change', (e) => {
            this.currentTier = e.target.value;
        });

        this.elements.settingsBtn.addEventListener('click', () => this.openSettings());
        this.elements.modalClose.addEventListener('click', () => this.closeSettings());
        this.elements.saveSettingsBtn.addEventListener('click', () => this.saveSettings());
        this.elements.resetSettingsBtn.addEventListener('click', () => this.resetSettings());
        this.elements.newChatBtn.addEventListener('click', () => this.newChat());
        this.elements.exportBtn.addEventListener('click', () => this.exportChat());

        document.querySelectorAll('.quick-action').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const message = e.target.dataset.message;
                this.elements.chatInput.value = message;
                this.updateTokenCount();
                this.sendMessage();
            });
        });

        document.getElementById('settingsRpcUrl').addEventListener('input', (e) => {
            this.elements.rpcUrl.value = e.target.value;
        });

        document.getElementById('settingsWsUrl').addEventListener('input', (e) => {
            this.elements.wsUrl.value = e.target.value;
        });

        document.getElementById('maxTokens').addEventListener('input', (e) => {
            document.getElementById('maxTokensValue').textContent = e.target.value;
        });

        document.getElementById('temperature').addEventListener('input', (e) => {
            document.getElementById('temperatureValue').textContent = e.target.value;
        });

        document.getElementById('themeToggle').addEventListener('change', (e) => {
            document.documentElement.setAttribute('data-theme', e.target.checked ? 'dark' : 'light');
        });

        window.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                this.closeSettings();
            }
        });
    }

    loadSettings() {
        const defaultSettings = {
            rpcUrl: 'http://127.0.0.1:9944',
            wsUrl: 'ws://127.0.0.1:9944',
            maxTokens: 2048,
            temperature: 0.7,
            walletAddress: '0x0000000000000000000000000000000000000002',
            theme: 'dark'
        };

        const saved = localStorage.getItem('aigenPlaygroundSettings');
        const parsed = saved ? JSON.parse(saved) : null;
        const settings = parsed ? { ...defaultSettings, ...parsed } : defaultSettings;
        if (settings.rpcUrl === 'http://localhost:9944') settings.rpcUrl = defaultSettings.rpcUrl;
        if (settings.wsUrl === 'ws://localhost:9944') settings.wsUrl = defaultSettings.wsUrl;
        if (!settings.walletAddress) settings.walletAddress = defaultSettings.walletAddress;

        document.getElementById('settingsRpcUrl').value = settings.rpcUrl;
        document.getElementById('settingsWsUrl').value = settings.wsUrl;
        document.getElementById('walletAddress').value = settings.walletAddress;
        document.getElementById('maxTokens').value = settings.maxTokens;
        document.getElementById('temperature').value = settings.temperature;
        document.getElementById('themeToggle').checked = settings.theme === 'dark';

        if (this.elements && this.elements.rpcUrl && this.elements.wsUrl) {
            this.elements.rpcUrl.value = settings.rpcUrl;
            this.elements.wsUrl.value = settings.wsUrl;
        }

        document.getElementById('maxTokensValue').textContent = settings.maxTokens;
        document.getElementById('temperatureValue').textContent = settings.temperature;
        document.documentElement.setAttribute('data-theme', settings.theme);

        localStorage.setItem('aigenPlaygroundSettings', JSON.stringify(settings));
        return settings;
    }

    saveSettings() {
        const settings = {
            rpcUrl: document.getElementById('settingsRpcUrl').value,
            wsUrl: document.getElementById('settingsWsUrl').value,
            maxTokens: parseInt(document.getElementById('maxTokens').value),
            temperature: parseFloat(document.getElementById('temperature').value),
            walletAddress: document.getElementById('walletAddress').value,
            theme: document.getElementById('themeToggle').checked ? 'dark' : 'light'
        };

        localStorage.setItem('aigenPlaygroundSettings', JSON.stringify(settings));
        this.settings = settings;
        this.closeSettings();
        
        this.showNotification('Settings saved successfully!', 'success');
    }

    resetSettings() {
        localStorage.removeItem('aigenPlaygroundSettings');
        this.loadSettings();
        this.showNotification('Settings reset to defaults', 'info');
    }

    openSettings() {
        this.elements.settingsModal.classList.add('show');
    }

    closeSettings() {
        this.elements.settingsModal.classList.remove('show');
    }

    async connect() {
        try {
            this.showLoading(true);
            
            this.client = new AIGENRPCClient(
                this.elements.rpcUrl.value,
                this.elements.wsUrl.value,
                (status) => this.updateConnectionStatus(status)
            );
            await this.client.connect();
            
            this.updateConnectionStatus('connected');
            await this.loadAvailableModels();
            await this.updateQuota();
            
            this.showNotification('Connected to AIGEN network!', 'success');
        } catch (error) {
            console.error('Connection failed:', error);
            this.updateConnectionStatus('disconnected');
            this.showNotification(`Connection failed: ${error.message}`, 'error');
        } finally {
            this.showLoading(false);
        }
    }

    async loadAvailableModels() {
        if (!this.client) return;
        const models = await this.client.listModels(null);
        if (!Array.isArray(models) || models.length === 0) return;

        this.elements.modelSelector.innerHTML = models
            .map(m => `<option value="${m.model_id}">${m.name} (${m.model_id})</option>`)
            .join('');

        const selected = models.find(m => m.model_id === this.currentModel) ? this.currentModel : models[0].model_id;
        this.currentModel = selected;
        this.elements.modelSelector.value = selected;
    }

    updateConnectionStatus(status) {
        const statusElement = this.elements.connectionStatus;
        const textElement = this.elements.connectionText;
        
        statusElement.className = `status-indicator ${status}`;
        
        switch (status) {
            case 'connected':
                textElement.textContent = 'Connected';
                this.elements.sendBtn.disabled = false;
                break;
            case 'disconnected':
                textElement.textContent = 'Disconnected';
                this.elements.sendBtn.disabled = true;
                break;
            default:
                textElement.textContent = 'Connecting...';
                this.elements.sendBtn.disabled = true;
        }
    }

    async updateQuota() {
        try {
            if (this.client) {
                const wallet = (this.settings && this.settings.walletAddress) ? this.settings.walletAddress.trim() : '';
                if (!wallet) {
                    this.elements.quotaValue.textContent = '--';
                    return;
                }
                const quota = await this.client.checkQuota(wallet);
                this.elements.quotaValue.textContent = `${quota.used}/${quota.limit}`;
            }
        } catch (error) {
            console.error('Failed to update quota:', error);
            this.elements.quotaValue.textContent = '--';
        }
    }

    updateTokenCount() {
        const text = this.elements.chatInput.value;
        const tokens = this.estimateTokens(text);
        this.elements.tokenCount.textContent = tokens;
    }

    estimateTokens(text) {
        return Math.ceil(text.length / 4);
    }

    async sendMessage() {
        const message = this.elements.chatInput.value.trim();
        if (!message || !this.client || !this.client.isConnected) {
            return;
        }

        this.elements.chatInput.value = '';
        this.updateTokenCount();
        this.addMessageToChat('user', message);
        this.elements.sendBtn.disabled = true;

        try {
            const messages = [
                ...this.chatHistory,
                { role: 'user', content: message }
            ];

            if (this.isStreaming) {
                await this.streamChatCompletion(messages);
            } else {
                await this.regularChatCompletion(messages);
            }

            this.chatHistory.push({ role: 'user', content: message });
            await this.updateQuota();
        } catch (error) {
            this.handleError(error);
        } finally {
            this.elements.sendBtn.disabled = false;
        }
    }

    async streamChatCompletion(messages) {
        let assistantMessage = '';
        const messageElement = this.addMessageToChat('assistant', '');
        
        await this.client.subscribeChatCompletion(
            messages,
            {
                modelId: this.currentModel,
                maxTokens: this.settings.maxTokens,
                temperature: this.settings.temperature,
                userAddress: this.settings.walletAddress
            },
            (chunk) => {
                if (chunk.delta && chunk.delta.content) {
                    assistantMessage += chunk.delta.content;
                    this.updateMessageContent(messageElement, assistantMessage);
                }
            },
            () => {
                this.chatHistory.push({ role: 'assistant', content: assistantMessage });
            },
            (error) => {
                this.handleError(error);
            }
        );
    }

    async regularChatCompletion(messages) {
        const response = await this.client.chatCompletion(
            messages,
            {
                modelId: this.currentModel,
                maxTokens: this.settings.maxTokens,
                temperature: this.settings.temperature,
                stream: false,
                userAddress: this.settings.walletAddress
            }
        );

        const assistantMessage = response.choices[0].message.content;
        this.addMessageToChat('assistant', assistantMessage);
        this.chatHistory.push({ role: 'assistant', content: assistantMessage });
    }

    addMessageToChat(role, content) {
        const messageDiv = document.createElement('div');
        messageDiv.className = `message ${role}`;
        
        const headerDiv = document.createElement('div');
        headerDiv.className = 'message-header';
        headerDiv.innerHTML = `
            <span>${role === 'user' ? 'You' : 'Assistant'}</span>
            <span>${new Date().toLocaleTimeString()}</span>
        `;
        
        const bubbleDiv = document.createElement('div');
        bubbleDiv.className = 'message-bubble';
        
        if (content) {
            bubbleDiv.innerHTML = this.renderMarkdown(content);
        }
        
        messageDiv.appendChild(headerDiv);
        messageDiv.appendChild(bubbleDiv);
        
        if (this.elements.chatMessages.querySelector('.welcome-message')) {
            this.elements.chatMessages.querySelector('.welcome-message').remove();
        }
        
        this.elements.chatMessages.appendChild(messageDiv);
        this.scrollToBottom();
        
        return bubbleDiv;
    }

    updateMessageContent(messageElement, content) {
        messageElement.innerHTML = this.renderMarkdown(content);
        this.scrollToBottom();
    }

    renderMarkdown(text) {
        return marked.parse(text);
    }

    scrollToBottom() {
        this.elements.chatMessages.scrollTop = this.elements.chatMessages.scrollHeight;
    }

    handleError(error) {
        console.error('Error:', error);
        
        let errorMessage = 'An error occurred. Please try again.';
        
        if (error.message.includes('Insufficient tier')) {
            errorMessage = 'Your current tier does not support this model. Please upgrade your subscription.';
        } else if (error.message.includes('Quota exceeded')) {
            errorMessage = 'You have exceeded your monthly quota. Please upgrade your tier or wait for the next billing cycle.';
        } else if (error.message.includes('Payment required')) {
            errorMessage = 'Payment required. Please add a transaction to continue.';
        }
        
        this.showNotification(errorMessage, 'error');
        this.addMessageToChat('assistant', `**Error:** ${errorMessage}`);
    }

    showNotification(message, type = 'info') {
        const notification = document.createElement('div');
        notification.className = `notification notification-${type}`;
        notification.textContent = message;
        
        notification.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            padding: 1rem 1.5rem;
            background: ${type === 'error' ? 'var(--accent-red)' : type === 'success' ? 'var(--accent-green)' : 'var(--accent-blue)'};
            color: white;
            border-radius: var(--border-radius);
            box-shadow: var(--shadow);
            z-index: 1001;
            animation: slideInRight 0.3s ease-out;
        `;
        
        document.body.appendChild(notification);
        
        setTimeout(() => {
            notification.style.animation = 'slideOutRight 0.3s ease-out';
            setTimeout(() => notification.remove(), 300);
        }, 3000);
    }

    showLoading(show) {
        this.elements.loadingOverlay.style.display = show ? 'flex' : 'none';
    }

    newChat() {
        this.chatHistory = [];
        this.elements.chatMessages.innerHTML = `
            <div class="welcome-message">
                <h2>Welcome to AIGEN AI Playground</h2>
                <p>Start chatting with AI models powered by the AIGEN blockchain. Select a model and tier, then type your message below.</p>
                <div class="quick-actions">
                    <button class="quick-action" data-message="Hello! How can you help me today?">Hello!</button>
                    <button class="quick-action" data-message="Explain quantum computing in simple terms">Explain quantum computing</button>
                    <button class="quick-action" data-message="Write a Python function to calculate fibonacci numbers">Write Python code</button>
                </div>
            </div>
        `;
        
        document.querySelectorAll('.quick-action').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const message = e.target.dataset.message;
                this.elements.chatInput.value = message;
                this.updateTokenCount();
                this.sendMessage();
            });
        });
    }

    exportChat() {
        const chatData = {
            timestamp: new Date().toISOString(),
            model: this.currentModel,
            tier: this.currentTier,
            messages: this.chatHistory
        };

        const jsonContent = JSON.stringify(chatData, null, 2);
        const markdownContent = this.generateMarkdownExport(chatData);

        const exportModal = document.createElement('div');
        exportModal.className = 'modal show';
        exportModal.innerHTML = `
            <div class="modal-content">
                <div class="modal-header">
                    <h2>Export Chat</h2>
                    <button class="modal-close">&times;</button>
                </div>
                <div class="modal-body">
                    <div style="margin-bottom: 1rem;">
                        <button id="exportJson" class="btn-primary" style="margin-right: 0.5rem;">Export as JSON</button>
                        <button id="exportMarkdown" class="btn-secondary">Export as Markdown</button>
                    </div>
                    <textarea readonly style="width: 100%; height: 300px; font-family: monospace;">${markdownContent}</textarea>
                </div>
            </div>
        `;

        document.body.appendChild(exportModal);

        exportModal.querySelector('.modal-close').addEventListener('click', () => {
            exportModal.remove();
        });

        exportModal.querySelector('#exportJson').addEventListener('click', () => {
            this.downloadFile(jsonContent, `chat-export-${Date.now()}.json`, 'application/json');
            exportModal.remove();
        });

        exportModal.querySelector('#exportMarkdown').addEventListener('click', () => {
            this.downloadFile(markdownContent, `chat-export-${Date.now()}.md`, 'text/markdown');
            exportModal.remove();
        });
    }

    generateMarkdownExport(chatData) {
        let markdown = `# AIGEN Chat Export\n\n`;
        markdown += `**Date:** ${new Date(chatData.timestamp).toLocaleString()}\n`;
        markdown += `**Model:** ${chatData.model}\n`;
        markdown += `**Tier:** ${chatData.tier}\n\n`;
        markdown += `---\n\n`;

        chatData.messages.forEach(msg => {
            markdown += `### ${msg.role === 'user' ? '👤 User' : '🤖 Assistant'}\n\n`;
            markdown += `${msg.content}\n\n`;
            markdown += `---\n\n`;
        });

        return markdown;
    }

    downloadFile(content, filename, mimeType) {
        const blob = new Blob([content], { type: mimeType });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    new AIGENPlayground();
});

const style = document.createElement('style');
style.textContent = `
@keyframes slideInRight {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
}

@keyframes slideOutRight {
    from { transform: translateX(0); opacity: 1; }
    to { transform: translateX(100%); opacity: 0; }
}
`;
document.head.appendChild(style);
