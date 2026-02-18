/**
 * AIGEN API Service - Enhanced RPC Client
 * 
 * Provides type-safe API calls to the AIGEN blockchain node with:
 * - Comprehensive error handling
 * - Retry logic with exponential backoff
 * - Request timeout handling
 * - Loading/success/error state management
 * - Request/response logging for debugging
 */

class ApiService {
    constructor(options = {}) {
        this.rpcUrl = options.rpcUrl || 'http://127.0.0.1:9933';
        this.wsUrl = options.wsUrl || 'ws://127.0.0.1:9933';
        this.timeout = options.timeout || 30000;
        this.maxRetries = options.maxRetries || 3;
        this.retryDelay = options.retryDelay || 1000;
        this.requestId = 0;
        
        // Event handlers for state updates
        this._onLoading = null;
        this._onSuccess = null;
        this._onError = null;
        
        // Logging enabled by default
        this.logging = options.logging !== false;
    }

    /**
     * Set event handlers for state management
     */
    setEventHandlers(handlers) {
        this._onLoading = handlers.onLoading || null;
        this._onSuccess = handlers.onSuccess || null;
        this._onError = handlers.onError || null;
    }

    /**
     * Emit loading state
     */
    _emitLoading(method, params) {
        if (this._onLoading) {
            this._onLoading({ method, params, timestamp: Date.now() });
        }
    }

    /**
     * Emit success state
     */
    _emitSuccess(method, result, duration) {
        if (this._onSuccess) {
            this._onSuccess({ method, result, duration, timestamp: Date.now() });
        }
    }

    /**
     * Emit error state
     */
    _emitError(method, error, params) {
        if (this._onError) {
            this._onError({ method, error: error.message, params, timestamp: Date.now() });
        }
    }

    /**
     * Log request/response for debugging
     */
    _log(level, message, data) {
        if (this.logging) {
            const prefix = `[API:${level.toUpperCase()}]`;
            if (data) {
                console[level](prefix, message, data);
            } else {
                console[level](prefix, message);
            }
        }
    }

    /**
     * Sleep utility for retry delays
     */
    _sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    /**
     * Make RPC call with retry logic and timeout
     */
    async _callRpc(method, params = [], options = {}) {
        const maxRetries = options.maxRetries ?? this.maxRetries;
        const timeout = options.timeout ?? this.timeout;
        const retryDelay = options.retryDelay ?? this.retryDelay;
        
        this._emitLoading(method, params);
        this._log('info', `Calling RPC method: ${method}`, { params });
        
        let lastError;
        const startTime = Date.now();
        
        for (let attempt = 1; attempt <= maxRetries; attempt++) {
            try {
                if (attempt > 1) {
                    this._log('warn', `Retry attempt ${attempt}/${maxRetries} for ${method}`);
                    await this._sleep(retryDelay * Math.pow(2, attempt - 1));
                }
                
                const result = await this._executeRequest(method, params, timeout);
                const duration = Date.now() - startTime;
                
                this._log('info', `RPC call successful: ${method}`, { duration: `${duration}ms` });
                this._emitSuccess(method, result, duration);
                
                return result;
            } catch (error) {
                lastError = error;
                this._log('error', `RPC call failed: ${method}`, { 
                    attempt, 
                    error: error.message 
                });
                
                // Don't retry on validation errors or auth errors
                if (this._isNonRetryableError(error)) {
                    this._emitError(method, error, params);
                    throw error;
                }
            }
        }
        
        this._emitError(method, lastError, params);
        throw lastError;
    }

    /**
     * Execute the actual HTTP request
     */
    async _executeRequest(method, params, timeout) {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), timeout);
        
        try {
            const body = {
                jsonrpc: '2.0',
                id: ++this.requestId,
                method,
                params
            };
            
            this._log('debug', `HTTP Request`, { url: this.rpcUrl, method, body });
            
            const response = await fetch(this.rpcUrl, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify(body),
                signal: controller.signal
            });
            
            clearTimeout(timeoutId);
            
            if (!response.ok) {
                throw new Error(`HTTP error: ${response.status} ${response.statusText}`);
            }
            
            const data = await response.json();
            this._log('debug', `HTTP Response`, { method, data });
            
            if (data.error) {
                const msg = typeof data.error.message === 'string' 
                    ? data.error.message 
                    : 'Unknown RPC error';
                const code = data.error.code;
                throw new RpcError(msg, code, method, data.error);
            }
            
            return data.result;
        } catch (error) {
            clearTimeout(timeoutId);
            
            if (error.name === 'AbortError') {
                throw new Error(`Request timeout after ${timeout}ms`);
            }
            
            throw error;
        }
    }

    /**
     * Check if error should not trigger retry
     */
    _isNonRetryableError(error) {
        if (error instanceof RpcError) {
            // Validation errors (-32600), method not found (-32601), etc.
            if (error.code >= -32699 && error.code <= -32600) {
                return true;
            }
            // Auth errors
            if (error.message.includes('invalid signature') ||
                error.message.includes('unauthorized') ||
                error.message.includes('forbidden')) {
                return true;
            }
        }
        return false;
    }

    /**
     * Update RPC endpoint
     */
    setEndpoint(rpcUrl, wsUrl) {
        this.rpcUrl = rpcUrl;
        this.wsUrl = wsUrl;
        this._log('info', 'RPC endpoints updated', { rpcUrl, wsUrl });
    }

    // ==================== Public RPC Methods ====================

    /**
     * Get block by height or hash
     */
    async getBlock(blockHeight = null, blockHash = null) {
        return this._callRpc('getBlock', [blockHeight, blockHash]);
    }

    /**
     * Get chain information
     */
    async getChainInfo() {
        return this._callRpc('getChainInfo', []);
    }

    /**
     * Get node information
     */
    async getNodeInfo() {
        return this._callRpc('getNodeInfo', []);
    }

    /**
     * Health check
     */
    async health() {
        return this._callRpc('health', []);
    }

    /**
     * Get balance for an address
     */
    async getBalance(address) {
        if (!address) {
            throw new Error('Address is required');
        }
        return this._callRpc('getBalance', [address]);
    }

    /**
     * Submit a transaction
     */
    async submitTransaction(transaction) {
        if (!transaction) {
            throw new Error('Transaction is required');
        }
        return this._callRpc('submitTransaction', [transaction]);
    }

    /**
     * Get DCS (Distributed Compute System) state
     */
    async getDcsState() {
        return this._callRpc('getDcsState', []);
    }

    /**
     * Submit inference task
     */
    async submitInferenceTask(task) {
        if (!task) {
            throw new Error('Task is required');
        }
        return this._callRpc('submitInferenceTask', [task]);
    }

    /**
     * Get task status
     */
    async getTaskStatus(taskId) {
        if (!taskId) {
            throw new Error('Task ID is required');
        }
        return this._callRpc('getTaskStatus', [taskId]);
    }

    // ==================== Orchestrator Methods ====================

    /**
     * Get orchestrator state
     */
    async getOrchestratorState() {
        return this._callRpc('getOrchestratorState', []);
    }

    /**
     * Get block assignments
     */
    async getBlockAssignments(blockHeight) {
        return this._callRpc('getBlockAssignments', [blockHeight]);
    }

    /**
     * Submit pipeline inference
     */
    async submitPipelineInference(inference) {
        if (!inference) {
            throw new Error('Inference data is required');
        }
        return this._callRpc('submitPipelineInference', [inference]);
    }

    // ==================== Rewards Methods ====================

    /**
     * Get node earnings
     */
    async reward_getNodeEarnings(nodeId) {
        if (!nodeId) {
            throw new Error('Node ID is required');
        }
        return this._callRpc('reward_getNodeEarnings', [nodeId]);
    }

    /**
     * Get task reward
     */
    async reward_getTaskReward(taskId) {
        if (!taskId) {
            throw new Error('Task ID is required');
        }
        return this._callRpc('reward_getTaskReward', [taskId]);
    }

    /**
     * List pending rewards
     */
    async reward_listPendingRewards(limit = 50) {
        return this._callRpc('reward_listPendingRewards', [limit]);
    }

    /**
     * Get reward leaderboard
     */
    async getRewardLeaderboard(limit = 100) {
        return this._callRpc('getRewardLeaderboard', [limit]);
    }

    // ==================== Staking/Governance Methods ====================

    /**
     * List stakes
     */
    async staking_listStakes() {
        return this._callRpc('staking_listStakes', []);
    }

    /**
     * Submit stake transaction
     */
    async staking_submitStakeTx(stakeData) {
        if (!stakeData) {
            throw new Error('Stake data is required');
        }
        return this._callRpc('staking_submitStakeTx', [stakeData]);
    }

    /**
     * List governance proposals
     */
    async governance_listProposals() {
        return this._callRpc('governance_listProposals', []);
    }

    /**
     * Submit governance vote
     */
    async governance_submitVote(voteData) {
        if (!voteData) {
            throw new Error('Vote data is required');
        }
        return this._callRpc('governance_submitVote', [voteData]);
    }

    // ==================== Admin Methods (CEO-signed) ====================

    /**
     * Submit shutdown (requires CEO signature)
     */
    async submitShutdown(signature, timestamp) {
        const ts = timestamp || Math.floor(Date.now() / 1000);
        return this._callRpc('submitShutdown', [{ signature, timestamp: ts }]);
    }

    /**
     * Approve SIP (requires CEO signature)
     */
    async approveSIP(proposalId, signature) {
        return this._callRpc('approveSIP', [{ proposal_id: proposalId, signature }]);
    }

    /**
     * Initialize new model (requires CEO signature)
     */
    async initNewModel(request, signature, timestamp) {
        const ts = timestamp || Math.floor(Date.now() / 1000);
        return this._callRpc('initNewModel', [{ ...request, signature, timestamp: ts }]);
    }

    /**
     * Approve model upgrade (requires CEO signature)
     */
    async approveModelUpgrade(proposalId, signature, timestamp) {
        const ts = timestamp || Math.floor(Date.now() / 1000);
        return this._callRpc('approveModelUpgrade', [{ proposal_id: proposalId, signature, timestamp: ts }]);
    }

    // ==================== Model Methods ====================

    /**
     * List AI models
     */
    async listModels(userAddress = null) {
        return this._callRpc('listModels', [userAddress]);
    }

    /**
     * Get model info
     */
    async getModelInfo(modelId) {
        if (!modelId) {
            throw new Error('Model ID is required');
        }
        return this._callRpc('getModelInfo', [{ model_id: modelId }]);
    }

    /**
     * Load model
     */
    async loadModel(modelId, userAddress, transaction) {
        return this._callRpc('loadModel', [{
            model_id: modelId,
            user_address: userAddress,
            transaction
        }]);
    }

    /**
     * Get pending transactions
     */
    async getPendingTransactions(limit = 50) {
        return this._callRpc('getPendingTransactions', [limit]);
    }

    /**
     * Get metrics (requires signature)
     */
    async getMetrics(signature, timestamp, includeAi = true, includeNetwork = true, includeBlockchain = true) {
        const ts = timestamp || Math.floor(Date.now() / 1000);
        return this._callRpc('getMetrics', [{
            signature,
            timestamp: ts,
            includeAi,
            includeNetwork,
            includeBlockchain
        }]);
    }
}

/**
 * Custom RPC Error class with code and method information
 */
class RpcError extends Error {
    constructor(message, code, method, rawError = null) {
        super(message);
        this.name = 'RpcError';
        this.code = code;
        this.method = method;
        this.rawError = rawError;
    }
}

// Export for use in browser and Node.js
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { ApiService, RpcError };
}
