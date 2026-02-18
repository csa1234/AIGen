/**
 * AIGEN State Manager
 * 
 * Provides centralized application state management with:
 * - Centralized state object
 * - Event-driven updates
 * - Loading/error/success states for all async operations
 * - Subscription management for real-time updates
 */

class StateManager {
    constructor(options = {}) {
        this.storageKey = options.storageKey || 'aigenState';
        this.persist = options.persist !== false;
        
        // Initial state
        this._state = {
            // Connection state
            connected: false,
            connectionError: null,
            
            // Blockchain data
            chainInfo: null,
            latestBlock: null,
            pendingTransactions: [],
            
            // AI Models
            models: [],
            modelLoading: false,
            modelError: null,
            
            // Health & Metrics
            health: null,
            metrics: null,
            metricsLoading: false,
            
            // Governance
            proposals: [],
            governanceLoading: false,
            
            // VRAM Pool
            vramPool: null,
            vramLoading: false,
            
            // Distributed Inference
            orchestratorState: null,
            tasks: [],
            inferenceLoading: false,
            
            // Rewards
            rewards: [],
            leaderboard: [],
            rewardsLoading: false,
            
            // UI State
            currentTab: 'explorer',
            theme: 'dark',
            refreshInterval: 30,
            
            // User state
            authenticated: false,
            publicKey: null,
            
            // Timestamps
            lastUpdate: null,
            lastError: null
        };
        
        // Event listeners
        this._listeners = new Map();
        
        // Loading states for async operations
        this._loadingStates = new Map();
        
        // Load persisted state if available
        this._loadState();
        
        // Setup default listeners
        this._setupDefaultListeners();
    }

    /**
     * Setup default event listeners
     */
    _setupDefaultListeners() {
        // Log state changes for debugging
        this.subscribe('*', (event, state) => {
            if (event.type !== 'update') {
                console.log(`[State] ${event.type}:`, event.data);
            }
        });
    }

    /**
     * Load state from storage
     */
    _loadState() {
        if (!this.persist) return;
        
        try {
            const stored = localStorage.getItem(this.storageKey);
            if (stored) {
                const persisted = JSON.parse(stored);
                // Merge with initial state, preserving function/objects
                this._state = { ...this._state, ...persisted };
            }
        } catch (error) {
            console.error('[State] Failed to load state:', error);
        }
    }

    /**
     * Save state to storage
     */
    _saveState() {
        if (!this.persist) return;
        
        try {
            // Don't persist large data or sensitive data
            const toPersist = {
                theme: this._state.theme,
                refreshInterval: this._state.refreshInterval,
                currentTab: this._state.currentTab,
                authenticated: this._state.authenticated,
                publicKey: this._state.publicKey
            };
            localStorage.setItem(this.storageKey, JSON.stringify(toPersist));
        } catch (error) {
            console.error('[State] Failed to save state:', error);
        }
    }

    /**
     * Get current state
     */
    getState() {
        return { ...this._state };
    }

    /**
     * Get specific state value
     */
    get(key) {
        return this._state[key];
    }

    /**
     * Update state
     */
    set(key, value) {
        const oldValue = this._state[key];
        this._state[key] = value;
        
        this._emit({
            type: 'update',
            key,
            oldValue,
            newValue: value
        });
        
        // Check and clear loading state if value is set
        if (value !== null && value !== undefined) {
            this._clearLoading(key);
        }
        
        // Persist relevant state
        if (['theme', 'refreshInterval', 'currentTab', 'authenticated', 'publicKey'].includes(key)) {
            this._saveState();
        }
        
        this._state.lastUpdate = Date.now();
    }

    /**
     * Set multiple state values at once
     */
    setMultiple(updates) {
        const changes = [];
        
        for (const [key, value] of Object.entries(updates)) {
            const oldValue = this._state[key];
            this._state[key] = value;
            changes.push({ key, oldValue, newValue: value });
        }
        
        this._emit({
            type: 'bulkUpdate',
            changes
        });
        
        this._state.lastUpdate = Date.now();
        this._saveState();
    }

    /**
     * Set loading state for an operation
     */
    setLoading(key, loading = true) {
        if (loading) {
            this._loadingStates.set(key, true);
            this._emit({
                type: 'loading',
                key,
                loading: true
            });
        } else {
            this._clearLoading(key);
        }
    }

    /**
     * Check if a key is loading
     */
    isLoading(key) {
        return this._loadingStates.get(key) === true;
    }

    /**
     * Clear loading state
     */
    _clearLoading(key) {
        if (this._loadingStates.has(key)) {
            this._loadingStates.delete(key);
            this._emit({
                type: 'loading',
                key,
                loading: false
            });
        }
    }

    /**
     * Set error state
     */
    setError(key, error) {
        this._state.lastError = {
            key,
            message: error.message || String(error),
            timestamp: Date.now()
        };
        
        this._emit({
            type: 'error',
            key,
            error: error.message || String(error)
        });
        
        this._clearLoading(key);
    }

    /**
     * Clear error state
     */
    clearError(key) {
        if (this._state.lastError && this._state.lastError.key === key) {
            this._state.lastError = null;
            
            this._emit({
                type: 'errorCleared',
                key
            });
        }
    }

    /**
     * Subscribe to state changes
     */
    subscribe(key, callback) {
        if (!this._listeners.has(key)) {
            this._listeners.set(key, new Set());
        }
        
        this._listeners.get(key).add(callback);
        
        // Return unsubscribe function
        return () => {
            this._listeners.get(key).delete(callback);
        };
    }

    /**
     * Unsubscribe from state changes
     */
    unsubscribe(key, callback) {
        if (this._listeners.has(key)) {
            this._listeners.get(key).delete(callback);
        }
    }

    /**
     * Emit event to listeners
     */
    _emit(event) {
        // Notify specific key listeners
        if (this._listeners.has(event.key)) {
            this._listeners.get(event.key).forEach(callback => {
                try {
                    callback(event, this._state);
                } catch (error) {
                    console.error('[State] Listener error:', error);
                }
            });
        }
        
        // Notify wildcard listeners
        if (this._listeners.has('*')) {
            this._listeners.get('*').forEach(callback => {
                try {
                    callback(event, this._state);
                } catch (error) {
                    console.error('[State] Wildcard listener error:', error);
                }
            });
        }
    }

    /**
     * Reset state to initial values
     */
    reset() {
        const oldState = { ...this._state };
        this._state = {
            connected: false,
            connectionError: null,
            chainInfo: null,
            latestBlock: null,
            pendingTransactions: [],
            models: [],
            modelLoading: false,
            modelError: null,
            health: null,
            metrics: null,
            metricsLoading: false,
            proposals: [],
            governanceLoading: false,
            vramPool: null,
            vramLoading: false,
            orchestratorState: null,
            tasks: [],
            inferenceLoading: false,
            rewards: [],
            leaderboard: [],
            rewardsLoading: false,
            currentTab: 'explorer',
            theme: 'dark',
            refreshInterval: 30,
            authenticated: false,
            publicKey: null,
            lastUpdate: null,
            lastError: null
        };
        
        this._emit({
            type: 'reset',
            oldState
        });
        
        this._saveState();
    }

    /**
     * Perform async operation with state management
     */
    async perform(key, asyncFn) {
        this.setLoading(key, true);
        this.clearError(key);
        
        try {
            const result = await asyncFn();
            this.set(key, result);
            return result;
        } catch (error) {
            this.setError(key, error);
            throw error;
        } finally {
            this.setLoading(key, false);
        }
    }

    /**
     * Get state summary for debugging
     */
    getSummary() {
        return {
            connected: this._state.connected,
            lastUpdate: this._state.lastUpdate,
            lastError: this._state.lastError,
            loadingCount: this._loadingStates.size,
            keys: Object.keys(this._state),
            theme: this._state.theme,
            authenticated: this._state.authenticated
        };
    }

    /**
     * Export state for debugging
     */
    export() {
        return JSON.stringify(this._state, null, 2);
    }

    /**
     * Import state from export
     */
    import(jsonString) {
        try {
            const imported = JSON.parse(jsonString);
            this._state = { ...this._state, ...imported };
            this._emit({ type: 'import' });
            this._saveState();
            return true;
        } catch (error) {
            console.error('[State] Import failed:', error);
            return false;
        }
    }
}

// Export for use in browser and Node.js
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { StateManager };
}
