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

import { v4 as uuidv4 } from 'uuid';
import axios from 'axios';
import WebSocket from 'ws';
import { createHash } from 'crypto';
import nacl from 'tweetnacl';

class AIGENError extends Error {
    constructor(message, code = null, details = null) {
        super(message);
        this.name = 'AIGENError';
        this.code = code;
        this.details = details;
    }
}

class QuotaExceededError extends AIGENError {
    constructor(message = 'Monthly quota exceeded', details = null) {
        super(message, 2004, details);
        this.name = 'QuotaExceededError';
    }
}

class InsufficientTierError extends AIGENError {
    constructor(message = 'Current tier does not support this feature', details = null) {
        super(message, 2003, details);
        this.name = 'InsufficientTierError';
    }
}

class PaymentRequiredError extends AIGENError {
    constructor(message = 'Payment required for this operation', details = null) {
        super(message, 2007, details);
        this.name = 'PaymentRequiredError';
    }
}

class AIGENClient {
    constructor(rpcUrl, wsUrl = null, walletAddress = null) {
        this.rpcUrl = rpcUrl;
        this.wsUrl = wsUrl || rpcUrl.replace('http', 'ws');
        this.walletAddress = walletAddress;
        this.ws = null;
        this.subscriptions = new Map();
        this.requestId = 0;
        this.isConnected = false;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        this.reconnectDelay = 1000;
        this.messageQueue = [];
        
        this.retryConfig = {
            maxRetries: 3,
            baseDelay: 1000,
            maxDelay: 10000,
            backoffMultiplier: 2
        };
    }

    async connect() {
        try {
            await this.checkRPCConnection();
            if (this.wsUrl) {
                await this.connectWebSocket();
            }
            this.isConnected = true;
            this.reconnectAttempts = 0;
            return true;
        } catch (error) {
            this.isConnected = false;
            throw new AIGENError(`Connection failed: ${error.message}`, 'CONNECTION_FAILED');
        }
    }

    async checkRPCConnection() {
        try {
            await this.makeRPCCall('system_health', []);
        } catch (error) {
            throw new AIGENError(`RPC connection check failed: ${error.message}`, 'RPC_CONNECTION_FAILED');
        }
    }

    async connectWebSocket() {
        return new Promise((resolve, reject) => {
            try {
                this.ws = new WebSocket(this.wsUrl);
                
                this.ws.onopen = () => {
                    this.isConnected = true;
                    this.setupWebSocketHandlers();
                    this.processMessageQueue();
                    resolve();
                };

                this.ws.onerror = (error) => {
                    reject(new AIGENError('WebSocket connection failed', 'WS_CONNECTION_FAILED'));
                };

                this.ws.onclose = () => {
                    this.isConnected = false;
                    this.handleDisconnection();
                };
            } catch (error) {
                reject(new AIGENError(`WebSocket setup failed: ${error.message}`, 'WS_SETUP_FAILED'));
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
    }

    handleWebSocketMessage(data) {
        if (data.method === 'subscription' && data.params?.subscription) {
            const callback = this.subscriptions.get(data.params.subscription);
            if (callback) {
                try {
                    callback(data.params.result);
                } catch (error) {
                    console.error('Subscription callback error:', error);
                }
            }
        }
    }

    handleDisconnection() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);
            
            setTimeout(() => {
                this.connectWebSocket().catch(error => {
                    console.error('Reconnection failed:', error);
                });
            }, delay);
        }
    }

    async makeRPCCall(method, params, retries = 0) {
        const payload = {
            jsonrpc: '2.0',
            id: uuidv4(),
            method,
            params
        };

        try {
            const response = await axios.post(this.rpcUrl, payload, {
                headers: { 'Content-Type': 'application/json' }
            });
            const data = response.data;
            
            if (data.error) {
                this.handleRPCError(data.error);
            }

            return data.result;
        } catch (error) {
            if (retries < this.retryConfig.maxRetries && this.isRetryableError(error)) {
                const delay = this.calculateRetryDelay(retries);
                await this.sleep(delay);
                return this.makeRPCCall(method, params, retries + 1);
            }
            if (error.response) {
                throw new AIGENError(`HTTP ${error.response.status}: ${error.response.statusText}`, 'HTTP_ERROR');
            }
            throw error;
        }
    }

    handleRPCError(error) {
        switch (error.code) {
            case 2003:
                throw new InsufficientTierError(error.message, error.data);
            case 2004:
                throw new QuotaExceededError(error.message, error.data);
            case 2007:
                throw new PaymentRequiredError(error.message, error.data);
            default:
                throw new AIGENError(error.message, error.code, error.data);
        }
    }

    isRetryableError(error) {
        return error.code === 'HTTP_ERROR' || 
               error.code === 'RPC_CONNECTION_FAILED' ||
               error.message.includes('timeout') ||
               error.message.includes('network');
    }

    calculateRetryDelay(attempt) {
        const delay = this.retryConfig.baseDelay * Math.pow(this.retryConfig.backoffMultiplier, attempt);
        return Math.min(delay, this.retryConfig.maxDelay);
    }

    sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async chatCompletion(messages, options = {}) {
        const params = {
            messages,
            model_id: options.modelId || 'mistral-7b',
            max_tokens: options.maxTokens || 2048,
            temperature: options.temperature || 0.7,
            stream: false
        };

        if (options.transaction) {
            params.transaction = options.transaction;
        }

        if (this.walletAddress) {
            params.wallet_address = this.walletAddress;
        }

        try {
            const result = await this.makeRPCCall('chatCompletion', params);
            return result;
        } catch (error) {
            throw new AIGENError(`Chat completion failed: ${error.message}`, 'CHAT_COMPLETION_FAILED', error);
        }
    }

    async streamChatCompletion(messages, options = {}, onChunk, onComplete, onError) {
        if (!this.isConnected || !this.ws) {
            throw new AIGENError('WebSocket not connected', 'WS_NOT_CONNECTED');
        }

        const subscriptionId = `sub_${uuidv4()}`;
        let accumulatedContent = '';
        
        this.subscriptions.set(subscriptionId, (result) => {
            try {
                if (result.error) {
                    this.handleRPCError(result.error);
                }

                if (result.finish_reason) {
                    this.subscriptions.delete(subscriptionId);
                    if (onComplete) {
                        onComplete({
                            content: accumulatedContent,
                            finish_reason: result.finish_reason
                        });
                    }
                } else if (result.delta) {
                    if (result.delta.content) {
                        accumulatedContent += result.delta.content;
                    }
                    if (onChunk) {
                        onChunk(result.delta);
                    }
                }
            } catch (error) {
                this.subscriptions.delete(subscriptionId);
                if (onError) {
                    onError(error);
                }
            }
        });

        const params = {
            messages,
            model_id: options.modelId || 'mistral-7b',
            max_tokens: options.maxTokens || 2048,
            temperature: options.temperature || 0.7,
            subscription: subscriptionId
        };

        if (options.transaction) {
            params.transaction = options.transaction;
        }

        if (this.walletAddress) {
            params.wallet_address = this.walletAddress;
        }

        try {
            const payload = {
                jsonrpc: '2.0',
                id: uuidv4(),
                method: 'subscribeChatCompletion',
                params
            };

            this.ws.send(JSON.stringify(payload));
            return subscriptionId;
        } catch (error) {
            this.subscriptions.delete(subscriptionId);
            throw new AIGENError(`Failed to subscribe: ${error.message}`, 'SUBSCRIPTION_FAILED', error);
        }
    }

    async checkQuota() {
        try {
            const result = await this.makeRPCCall('checkQuota', []);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to check quota: ${error.message}`, 'QUOTA_CHECK_FAILED', error);
        }
    }

    async getTierInfo() {
        try {
            const result = await this.makeRPCCall('getTierInfo', []);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to get tier info: ${error.message}`, 'TIER_INFO_FAILED', error);
        }
    }

    async subscribeTier(tier, durationMonths = 1, transaction = null) {
        const params = {
            tier,
            duration_months: durationMonths
        };

        if (transaction) {
            params.transaction = transaction;
        }

        try {
            const result = await this.makeRPCCall('subscribeTier', params);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to subscribe to tier: ${error.message}`, 'TIER_SUBSCRIPTION_FAILED', error);
        }
    }

    async listModels() {
        try {
            const result = await this.makeRPCCall('listModels', []);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to list models: ${error.message}`, 'MODEL_LIST_FAILED', error);
        }
    }

    async getModelInfo(modelId) {
        try {
            const result = await this.makeRPCCall('getModelInfo', [modelId]);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to get model info: ${error.message}`, 'MODEL_INFO_FAILED', error);
        }
    }

    async loadModel(modelId, transaction = null) {
        const params = { model_id: modelId };
        
        if (transaction) {
            params.transaction = transaction;
        }

        try {
            const result = await this.makeRPCCall('loadModel', params);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to load model: ${error.message}`, 'MODEL_LOAD_FAILED', error);
        }
    }

    async submitBatchRequest(priority, modelId, inputData, transaction = null) {
        const params = {
            priority,
            model_id: modelId,
            input_data: inputData
        };

        if (transaction) {
            params.transaction = transaction;
        }

        try {
            const result = await this.makeRPCCall('submitBatchRequest', params);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to submit batch request: ${error.message}`, 'BATCH_SUBMISSION_FAILED', error);
        }
    }

    async getBatchStatus(jobId) {
        try {
            const result = await this.makeRPCCall('getBatchStatus', [jobId]);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to get batch status: ${error.message}`, 'BATCH_STATUS_FAILED', error);
        }
    }

    async listUserJobs(status = null) {
        const params = status ? [status] : [];
        
        try {
            const result = await this.makeRPCCall('listUserJobs', params);
            return result;
        } catch (error) {
            throw new AIGENError(`Failed to list user jobs: ${error.message}`, 'JOB_LIST_FAILED', error);
        }
    }

    signTransaction(tx, privateKey) {
        try {
            const message = new TextEncoder().encode(JSON.stringify(tx));
            const signature = nacl.sign.detached(message, privateKey);
            return Buffer.from(signature).toString('hex');
        } catch (error) {
            throw new AIGENError(`Failed to sign transaction: ${error.message}`, 'SIGNING_FAILED', error);
        }
    }

    deriveAddress(publicKey) {
        try {
            const hash = createHash('sha256').update(publicKey).digest();
            return hash.toString('hex').slice(0, 32);
        } catch (error) {
            throw new AIGENError(`Failed to derive address: ${error.message}`, 'ADDRESS_DERIVATION_FAILED', error);
        }
    }

    createPaymentTransaction(amount, payload = null) {
        const tx = {
            type: 'payment',
            amount: amount,
            timestamp: Date.now(),
            nonce: Math.floor(Math.random() * 1000000)
        };

        if (payload) {
            tx.payload = payload;
        }

        return tx;
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        this.isConnected = false;
        this.subscriptions.clear();
        this.messageQueue = [];
    }

    processMessageQueue() {
        while (this.messageQueue.length > 0) {
            const message = this.messageQueue.shift();
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(JSON.stringify(message));
            }
        }
    }
}

export {
    AIGENClient,
    AIGENError,
    QuotaExceededError,
    InsufficientTierError,
    PaymentRequiredError
};
