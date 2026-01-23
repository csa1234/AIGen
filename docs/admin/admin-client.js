class AdminRPCClient {
    constructor(rpcUrl, wsUrl) {
        this.rpcUrl = rpcUrl;
        this.wsUrl = wsUrl;
        this.ws = null;
        this.requestId = 0;
        this.pendingRequests = new Map();
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        this.reconnectDelay = 3000;
    }

    connect() {
        return new Promise((resolve, reject) => {
            this.ws = new WebSocket(this.wsUrl);
            
            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.reconnectAttempts = 0;
                resolve();
            };
            
            this.ws.onmessage = (event) => {
                const response = JSON.parse(event.data);
                const { id, result, error } = response;
                
                if (this.pendingRequests.has(id)) {
                    const { resolve, reject } = this.pendingRequests.get(id);
                    this.pendingRequests.delete(id);
                    
                    if (error) {
                        reject(new Error(error.message));
                    } else {
                        resolve(result);
                    }
                }
            };
            
            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                reject(error);
            };
            
            this.ws.onclose = () => {
                console.log('WebSocket disconnected');
                this.handleReconnect();
            };
        });
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }

    handleReconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            console.log(`Reconnecting... Attempt ${this.reconnectAttempts}`);
            setTimeout(() => {
                this.connect().catch(console.error);
            }, this.reconnectDelay);
        }
    }

    async call(method, params = []) {
        return new Promise((resolve, reject) => {
            const id = ++this.requestId;
            const message = {
                jsonrpc: '2.0',
                id,
                method,
                params
            };
            
            this.pendingRequests.set(id, { resolve, reject });
            
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(JSON.stringify(message));
            } else {
                reject(new Error('WebSocket not connected'));
            }
        });
    }

    async callHttp(method, params = []) {
        try {
            const response = await fetch(this.rpcUrl, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    jsonrpc: '2.0',
                    id: ++this.requestId,
                    method,
                    params
                })
            });
            
            const data = await response.json();
            
            if (data.error) {
                throw new Error(data.error.message);
            }
            
            return data.result;
        } catch (error) {
            throw new Error(`RPC call failed: ${error.message}`);
        }
    }

    async connectWallet() {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected. Please install MetaMask or WalletConnect.');
        }
        
        try {
            const accounts = await window.ethereum.request({ method: 'eth_requestAccounts' });
            return accounts[0];
        } catch (error) {
            throw new Error(`Wallet connection failed: ${error.message}`);
        }
    }

    async signMessage(message) {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }
        
        try {
            const signature = await window.ethereum.request({
                method: 'personal_sign',
                params: [message, await this.getWalletAddress()]
            });
            return signature;
        } catch (error) {
            throw new Error(`Signing failed: ${error.message}`);
        }
    }

    async getWalletAddress() {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }
        
        const accounts = await window.ethereum.request({ method: 'eth_accounts' });
        if (accounts.length === 0) {
            throw new Error('No wallet connected');
        }
        
        return accounts[0];
    }

    verifyCeoWallet(address) {
        const storedCeoAddress = localStorage.getItem('ceoAddress');
        if (!storedCeoAddress) {
            console.warn('CEO address not configured');
            return false;
        }
        
        return address.toLowerCase() === storedCeoAddress.toLowerCase();
    }

    async getBlock(blockHash) {
        return this.callHttp('chain_getBlock', [blockHash]);
    }

    async getTransaction(txHash) {
        return this.callHttp('chain_getTransaction', [txHash]);
    }

    async getChainInfo() {
        return this.callHttp('chain_getInfo', []);
    }

    async getLatestBlocks(count = 10) {
        return this.callHttp('chain_getLatestBlocks', [count]);
    }

    async getPendingTransactions() {
        return this.callHttp('chain_getPendingTransactions', []);
    }

    async getHealth(signature, timestamp) {
        return this.callHttp('getHealth', [{ signature, timestamp }]);
    }

    async getMetrics(signature, timestamp, includeAi = true, includeNetwork = true, includeBlockchain = true) {
        return this.callHttp('getMetrics', [{
            signature,
            timestamp,
            includeAi,
            includeNetwork,
            includeBlockchain
        }]);
    }

    async listModels() {
        return this.callHttp('model_listModels', []);
    }

    async getModelInfo(modelId) {
        return this.callHttp('model_getInfo', [modelId]);
    }

    async initNewModel(request, signature) {
        return this.callHttp('initNewModel', [{ ...request, signature }]);
    }

    async loadModel(modelId) {
        return this.callHttp('model_load', [modelId]);
    }

    async approveModelUpgrade(proposalId, signature, timestamp) {
        return this.callHttp('approveModelUpgrade', [{ proposalId, signature, timestamp }]);
    }

    async rejectUpgrade(proposalId, reason, signature, timestamp) {
        return this.callHttp('rejectUpgrade', [{ proposalId, reason, signature, timestamp }]);
    }

    async submitGovVote(proposalId, vote, comment, signature, timestamp) {
        return this.callHttp('submitGovVote', [{
            proposalId,
            vote,
            comment,
            signature,
            timestamp
        }]);
    }

    async approveSIP(proposalId, signature) {
        return this.callHttp('approveSIP', [{ proposalId, signature }]);
    }

    async vetoSIP(proposalId, signature) {
        return this.callHttp('vetoSIP', [{ proposalId, signature }]);
    }

    async getSIPStatus(proposalId) {
        return this.callHttp('getSIPStatus', [proposalId]);
    }

    async submitShutdown(timestamp, reason, nonce, signature) {
        return this.callHttp('submitShutdown', [{
            timestamp,
            reason,
            nonce,
            signature
        }]);
    }

    formatAdminMessage(action, params = {}) {
        const networkMagic = 0x41494745;
        const timestamp = Math.floor(Date.now() / 1000);
        
        let message = `admin_${action}:${networkMagic}:${timestamp}`;
        
        Object.entries(params).forEach(([key, value]) => {
            message += `:${key}=${value}`;
        });
        
        return { message, timestamp };
    }

    parseRpcError(error) {
        const errorMap = {
            'CEO signature verification failed': 'Invalid CEO signature. Please ensure you are using the correct CEO wallet.',
            'Unauthorized': 'Unauthorized. CEO access required.',
            'Model not found': 'The specified model could not be found.',
            'Proposal not found': 'The specified proposal could not be found.',
            'Invalid parameters': 'Invalid request parameters. Please check your input.',
            'Network error': 'Network connection failed. Please check your connection.',
            'Timeout': 'Request timed out. Please try again.'
        };
        
        for (const [key, value] of Object.entries(errorMap)) {
            if (error.message.includes(key)) {
                return value;
            }
        }
        
        return error.message || 'An unknown error occurred.';
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = AdminRPCClient;
}
