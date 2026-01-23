export class AIGENClient {
    constructor(rpcUrl: string, wsUrl?: string, walletAddress?: string);
    
    connect(): Promise<boolean>;
    disconnect(): void;
    
    chatCompletion(messages: ChatMessage[], options?: ChatCompletionOptions): Promise<ChatCompletionResponse>;
    streamChatCompletion(
        messages: ChatMessage[], 
        options: ChatCompletionOptions, 
        onChunk: (delta: ChatCompletionDelta) => void,
        onComplete?: (result: ChatCompletionResult) => void,
        onError?: (error: AIGENError) => void
    ): Promise<string>;
    
    checkQuota(): Promise<QuotaInfo>;
    getTierInfo(): Promise<TierInfo>;
    subscribeTier(tier: string, durationMonths?: number, transaction?: Transaction): Promise<any>;
    
    listModels(): Promise<ModelInfo[]>;
    getModelInfo(modelId: string): Promise<ModelInfo>;
    loadModel(modelId: string, transaction?: Transaction): Promise<any>;
    
    submitBatchRequest(priority: string, modelId: string, inputData: any, transaction?: Transaction): Promise<BatchJob>;
    getBatchStatus(jobId: string): Promise<BatchJob>;
    listUserJobs(status?: string): Promise<BatchJob[]>;
    
    signTransaction(tx: Transaction, privateKey: Buffer): string;
    deriveAddress(publicKey: Buffer): string;
    createPaymentTransaction(amount: number, payload?: any): Transaction;
}

export interface ChatMessage {
    role: 'user' | 'assistant' | 'system';
    content: string;
}

export interface ChatCompletionOptions {
    modelId?: string;
    maxTokens?: number;
    temperature?: number;
    stream?: boolean;
    transaction?: Transaction;
}

export interface ChatCompletionResponse {
    id: string;
    object: string;
    created: number;
    model: string;
    choices: ChatCompletionChoice[];
    usage: UsageInfo;
}

export interface ChatCompletionChoice {
    index: number;
    message: ChatMessage;
    finish_reason: string | null;
}

export interface ChatCompletionDelta {
    content?: string;
    role?: string;
}

export interface ChatCompletionResult {
    content: string;
    finish_reason: string;
}

export interface QuotaInfo {
    used: number;
    limit: number;
    reset_date: string;
}

export interface TierInfo {
    current_tier: string;
    available_tiers: string[];
    tier_limits: Record<string, number>;
}

export interface ModelInfo {
    id: string;
    name: string;
    description: string;
    tier_required: string;
    max_tokens: number;
    context_length: number;
}

export interface BatchJob {
    id: string;
    status: string;
    priority: string;
    model_id: string;
    created_at: string;
    completed_at?: string;
    result?: any;
    error?: string;
}

export interface Transaction {
    type: string;
    amount: number;
    timestamp: number;
    nonce: number;
    payload?: any;
}

export class AIGENError extends Error {
    code: string | null;
    details: any;
    
    constructor(message: string, code?: string, details?: any);
}

export class QuotaExceededError extends AIGENError {
    constructor(message?: string, details?: any);
}

export class InsufficientTierError extends AIGENError {
    constructor(message?: string, details?: any);
}

export class PaymentRequiredError extends AIGENError {
    constructor(message?: string, details?: any);
}