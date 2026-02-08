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

    async connect(maxRetries = 3, onStatusUpdate = null) {
        const isFileProtocol = typeof window !== 'undefined' && window.location && window.location.protocol.startsWith('file');
        let lastError;
        
        // Store callback for reconnects
        this.onStatusUpdate = onStatusUpdate;
        
        for (let attempt = 1; attempt <= maxRetries; attempt++) {
            if (attempt > 1 && onStatusUpdate) {
                onStatusUpdate(`Connecting... (attempt ${attempt}/${maxRetries})`);
            }

            let wsOk = false;
            try {
                await new Promise((resolve, reject) => {
                    this.ws = new WebSocket(this.wsUrl);
                    const timeout = setTimeout(() => reject(new Error('WebSocket connection timeout')), 3000);
                    this.ws.onopen = () => {
                        clearTimeout(timeout);
                        this.reconnectAttempts = 0;
                        wsOk = true;
                        resolve();
                    };
                    this.ws.onmessage = (event) => {
                        const response = JSON.parse(event.data);
                        const { id, result, error } = response;
                        if (this.pendingRequests.has(id)) {
                            const { resolve, reject } = this.pendingRequests.get(id);
                            this.pendingRequests.delete(id);
                            if (error) reject(new Error(error.message));
                            else resolve(result);
                        }
                    };
                    this.ws.onerror = (error) => {
                        clearTimeout(timeout);
                        if (this.onStatusUpdate) this.onStatusUpdate('error', 'WebSocket error');
                        reject(error);
                    };
                    this.ws.onclose = () => {
                        if (this.onStatusUpdate) this.onStatusUpdate('disconnected');
                        this.handleReconnect();
                    };
                });
            } catch (_) {}
            
            try {
                if (onStatusUpdate) onStatusUpdate('Checking health...');
                const health = await this.callHttp('health', []);
                if (health && typeof health.status === 'string' && wsOk) return;
            } catch (httpErr) {
                lastError = httpErr;
            }

            // If we're here, either WS failed or Health check failed
            if (attempt < maxRetries) {
                const backoff = 1000 * Math.pow(2, attempt - 1);
                await new Promise(r => setTimeout(r, backoff));
                continue;
            }
        }

        let msg = `RPC connection failed: ${lastError ? lastError.message : 'WebSocket or Health check failed'}`;
        if (lastError && lastError.message.includes("Failed to fetch")) {
            msg = `Node not running on ${this.rpcUrl} or CORS blocked`;
        }
        
        if (isFileProtocol) {
            msg += ' — Serve the dashboard over http:// or https:// instead of file://';
        } else {
            msg += '. Check if node is running: cargo run --release';
        }
        throw new Error(msg);
    }

    async testHealth() {
        return this.callHttp('health', []);
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
            const msg = `Reconnecting... Attempt ${this.reconnectAttempts}`;
            console.log(msg);
            if (this.onStatusUpdate) this.onStatusUpdate('connecting', msg);
            
            setTimeout(() => {
                this.connect().catch((err) => {
                    console.error(err);
                    // If this was the last attempt, mark as disconnected/error
                    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
                         if (this.onStatusUpdate) this.onStatusUpdate('disconnected', 'Failed - Check if node is running');
                    }
                });
            }, this.reconnectDelay);
        } else {
            if (this.onStatusUpdate) this.onStatusUpdate('disconnected', 'Failed - Check if node is running');
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

    async signMessage(message) {
        // Use ed25519 signing with CEO private key from localStorage
        const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
        if (!ceoPrivateKey) {
            throw new Error('CEO private key not configured. Please add it in settings.');
        }

        // Check if tweetnacl is available
        if (typeof nacl === 'undefined') {
            throw new Error('tweetnacl library not loaded. Please include tweetnacl-js for ed25519 signing.');
        }

        // Convert message to UTF-8 bytes
        const encoder = new TextEncoder();
        const messageBytes = encoder.encode(message);

        // Decode hex private key to bytes
        const privateKeyBytes = this.hexToBytes(ceoPrivateKey);
        if (privateKeyBytes.length !== 64) {
            throw new Error('CEO private key must be 64 bytes (seed + public key)');
        }

        // Create key pair from private key seed (first 32 bytes)
        const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));

        // Sign the message
        const signature = nacl.sign.detached(messageBytes, keyPair.secretKey);

        // Return 64-byte signature as hex string
        return this.bytesToHex(signature);
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

    async getBlock(blockHeight = null, blockHash = null) {
        if (blockHeight === null && blockHash === null) {
            throw new Error('Must provide either blockHeight or blockHash');
        }
        return this.callHttp('getBlock', [{ block_height: blockHeight, block_hash: blockHash }]);
    }

    async getTransaction(txHash) {
        // Note: There's no direct getTransaction RPC, need to get block and search
        // For now, return error - this needs to be implemented differently
        throw new Error('getTransaction not available via RPC. Use getBlock and search transactions.');
    }

    async getChainInfo() {
        return this.callHttp('getChainInfo', []);
    }

    async getLatestBlocks(count = 10) {
        // Use getChainInfo to get latest height, then fetch blocks
        const chainInfo = await this.getChainInfo();
        const blocks = [];
        for (let i = 0; i < count && chainInfo.height >= i; i++) {
            const block = await this.getBlock(chainInfo.height - i);
            if (block) blocks.push(block);
        }
        return blocks;
    }

    async getPendingTransactions(limit = 50) {
        return this.callHttp('getPendingTransactions', [limit]);
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

    async listModels(userAddress = null) {
        return this.callHttp('listModels', [userAddress]);
    }

    async getModelInfo(modelId) {
        return this.callHttp('getModelInfo', [{ model_id: modelId }]);
    }

    async initNewModel(request, signature) {
        return this.callHttp('initNewModel', [{ ...request, signature }]);
    }

    async loadModel(modelId, userAddress, transaction) {
        return this.callHttp('loadModel', [{
            model_id: modelId,
            user_address: userAddress,
            transaction: transaction
        }]);
    }

    async approveModelUpgrade(proposalId, signature, timestamp) {
        return this.callHttp('approveModelUpgrade', [{
            proposal_id: proposalId,
            signature: signature,
            timestamp: timestamp
        }]);
    }

    async rejectUpgrade(proposalId, reason, signature, timestamp) {
        return this.callHttp('rejectUpgrade', [{
            proposal_id: proposalId,
            reason: reason,
            signature: signature,
            timestamp: timestamp
        }]);
    }

    async submitGovVote(proposalId, vote, comment, signature, timestamp) {
        return this.callHttp('submitGovVote', [{
            proposal_id: proposalId,
            vote: vote,
            comment: comment,
            signature: signature,
            timestamp: timestamp
        }]);
    }

    async approveSIP(proposalId, signature) {
        return this.callHttp('approveSIP', [{
            proposal_id: proposalId,
            signature: signature
        }]);
    }

    async vetoSIP(proposalId, signature) {
        return this.callHttp('vetoSIP', [{
            proposal_id: proposalId,
            signature: signature
        }]);
    }

    async getSIPStatus(proposalId) {
        return this.callHttp('getSIPStatus', [proposalId]);
    }

    async submitShutdown(timestamp, reason, nonce, signature) {
        return this.callHttp('submitShutdown', [{
            timestamp: timestamp,
            reason: reason,
            nonce: nonce,
            signature: signature
        }]);
    }

    formatAdminMessage(action, params = {}) {
        const networkMagic = 0x41494745;
        const timestamp = Math.floor(Date.now() / 1000);

        // Exact message formats as defined in node/src/rpc/ceo.rs
        switch (action) {
            case 'shutdown':
                return {
                    message: `shutdown:${networkMagic}:${timestamp}:${params.nonce}:${params.reason}`,
                    timestamp
                };
            case 'health':
                return {
                    message: `admin_health:${networkMagic}:${timestamp}`,
                    timestamp
                };
            case 'metrics':
                return {
                    message: `get_metrics:${networkMagic}:${timestamp}`,
                    timestamp
                };
            case 'initNewModel':
                const hashesStr = params.verification_hashes.join(':');
                return {
                    message: `init_model:${networkMagic}:${timestamp}:${params.model_id}:${params.version}:${params.totalSize}:${params.shardCount}:${params.verification_hashes.length}:${params.isCoreModel}:${params.minimumTier || ''}:${params.isExperimental}:${params.name}:${hashesStr}`,
                    timestamp
                };
            case 'approveModelUpgrade':
                return {
                    message: `approve_upgrade:${networkMagic}:${timestamp}:${params.proposalId}`,
                    timestamp
                };
            case 'rejectUpgrade':
                return {
                    message: `reject_upgrade:${networkMagic}:${timestamp}:${params.proposalId}:${params.reason}`,
                    timestamp
                };
            case 'submitGovVote':
                return {
                    message: `gov_vote:${networkMagic}:${timestamp}:${params.proposalId}:${params.vote}`,
                    timestamp
                };
            case 'approveSIP':
                return {
                    message: `approve_sip:${networkMagic}:${params.proposalId}`,
                    timestamp
                };
            case 'vetoSIP':
                return {
                    message: `veto_sip:${networkMagic}:${params.proposalId}`,
                    timestamp
                };
            default:
                throw new Error(`Unknown admin action: ${action}`);
        }
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
