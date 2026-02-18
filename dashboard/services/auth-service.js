/**
 * AIGEN Authentication Service
 * 
 * Provides secure session management with:
 * - Key derivation and validation
 * - Session timeout handling
 * - Auto-logout on session expiry
 * - Secure storage (with warnings about localStorage limitations)
 */

class AuthService {
    constructor(options = {}) {
        this.sessionTimeout = options.sessionTimeout || 3600000; // 1 hour default
        this.storageKey = options.storageKey || 'aigenAuth';
        this.autoRefreshInterval = null;
        this.sessionStartTime = null;
        
        // Event handlers
        this._onSessionExpire = null;
        this._onSessionUpdate = null;
        
        // Check for existing session
        this._initializeSession();
    }

    /**
     * Set event handlers
     */
    setEventHandlers(handlers) {
        this._onSessionExpire = handlers.onSessionExpire || null;
        this._onSessionUpdate = handlers.onSessionUpdate || null;
    }

    /**
     * Initialize session from storage
     */
    _initializeSession() {
        const stored = this.getSession();
        if (stored) {
            this.sessionStartTime = stored.timestamp;
            this._checkSessionExpiry();
            this._startAutoRefresh();
        }
    }

    /**
     * Get session from storage
     */
    getSession() {
        try {
            const stored = localStorage.getItem(this.storageKey);
            if (!stored) return null;
            
            const session = JSON.parse(stored);
            
            // Validate required fields
            if (!session.publicKey || !session.timestamp) {
                this.clearSession();
                return null;
            }
            
            return session;
        } catch (error) {
            console.error('[Auth] Failed to parse session:', error);
            this.clearSession();
            return null;
        }
    }

    /**
     * Save session to storage
     */
    _saveSession(session) {
        try {
            localStorage.setItem(this.storageKey, JSON.stringify(session));
            this.sessionStartTime = session.timestamp;
            this._startAutoRefresh();
            
            if (this._onSessionUpdate) {
                this._onSessionUpdate(session);
            }
        } catch (error) {
            console.error('[Auth] Failed to save session:', error);
        }
    }

    /**
     * Clear session
     */
    clearSession() {
        localStorage.removeItem(this.storageKey);
        this.sessionStartTime = null;
        
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
            this.autoRefreshInterval = null;
        }
        
        if (this._onSessionUpdate) {
            this._onSessionUpdate(null);
        }
    }

    /**
     * Check if session is valid
     */
    isAuthenticated() {
        const session = this.getSession();
        if (!session) return false;
        
        // Check if session has expired
        if (this._isSessionExpired()) {
            this.clearSession();
            return false;
        }
        
        return true;
    }

    /**
     * Check if session has expired
     */
    _isSessionExpired() {
        if (!this.sessionStartTime) return true;
        
        const now = Date.now();
        const elapsed = now - this.sessionStartTime;
        return elapsed > this.sessionTimeout;
    }

    /**
     * Check session expiry and emit event if expired
     */
    _checkSessionExpiry() {
        if (this._isSessionExpired()) {
            this.clearSession();
            if (this._onSessionExpire) {
                this._onSessionExpire();
            }
        }
    }

    /**
     * Start auto-refresh interval
     */
    _startAutoRefresh() {
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
        }
        
        // Check every minute
        this.autoRefreshInterval = setInterval(() => {
            this._checkSessionExpiry();
        }, 60000);
    }

    /**
     * Refresh session timestamp
     */
    refreshSession() {
        const session = this.getSession();
        if (session) {
            session.timestamp = Date.now();
            this._saveSession(session);
        }
    }

    /**
     * Generate a new keypair
     */
    generateKeyPair() {
        if (typeof nacl === 'undefined') {
            throw new Error('TweetNaCl library not loaded');
        }
        
        const keyPair = nacl.sign.keyPair();
        return {
            publicKey: this._bytesToHex(keyPair.publicKey),
            privateKey: this._bytesToHex(keyPair.secretKey)
        };
    }

    /**
     * Derive public key from private key
     */
    derivePublicKey(privateKeyHex) {
        try {
            const privateKeyBytes = this._hexToBytes(privateKeyHex);
            if (privateKeyBytes.length !== 64) {
                throw new Error('Invalid private key length');
            }
            
            const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));
            return this._bytesToHex(keyPair.publicKey);
        } catch (error) {
            console.error('[Auth] Failed to derive public key:', error);
            return null;
        }
    }

    /**
     * Import CEO private key
     */
    importCeoKey(privateKeyHex) {
        if (!privateKeyHex) {
            throw new Error('Private key is required');
        }
        
        // Validate the key format
        const privateKeyBytes = this._hexToBytes(privateKeyHex);
        if (privateKeyBytes.length !== 64) {
            throw new Error('Invalid CEO private key: must be 64 bytes (32-byte seed + 32-byte public)');
        }
        
        // Derive public key
        const publicKey = this.derivePublicKey(privateKeyHex);
        if (!publicKey) {
            throw new Error('Failed to derive public key from private key');
        }
        
        // Store the key
        const session = {
            publicKey: publicKey,
            privateKey: privateKeyHex,
            timestamp: Date.now(),
            type: 'ceo'
        };
        
        this._saveSession(session);
        
        // Also store in the legacy location for backward compatibility
        localStorage.setItem('ceoPrivateKey', privateKeyHex);
        
        return { publicKey, privateKey: privateKeyHex };
    }

    /**
     * Get CEO private key
     */
    getCeoPrivateKey() {
        const session = this.getSession();
        if (!session || session.type !== 'ceo') {
            // Fallback to legacy storage
            return localStorage.getItem('ceoPrivateKey');
        }
        return session.privateKey;
    }

    /**
     * Get CEO public key
     */
    getCeoPublicKey() {
        const session = this.getSession();
        if (!session || session.type !== 'ceo') {
            // Try to derive from stored private key
            const privateKey = localStorage.getItem('ceoPrivateKey');
            if (privateKey) {
                return this.derivePublicKey(privateKey);
            }
            return null;
        }
        return session.publicKey;
    }

    /**
     * Sign a message with CEO key
     */
    async signMessage(message) {
        const privateKeyHex = this.getCeoPrivateKey();
        
        if (!privateKeyHex) {
            throw new Error('CEO private key not configured. Please import your CEO key first.');
        }
        
        if (typeof nacl === 'undefined') {
            throw new Error('TweetNaCl library not loaded. Please include tweetnacl-js for Ed25519 signing.');
        }
        
        // Convert message to UTF-8 bytes
        const encoder = new TextEncoder();
        const messageBytes = encoder.encode(message);
        
        // Decode hex private key to bytes
        const privateKeyBytes = this._hexToBytes(privateKeyHex);
        
        // Create key pair from private key seed (first 32 bytes)
        const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));
        
        // Sign the message
        const signature = nacl.sign.detached(messageBytes, keyPair.secretKey);
        
        // Return 64-byte signature as hex string
        return this._bytesToHex(signature);
    }

    /**
     * Sign data with timestamp for RPC calls
     */
    async signRpcRequest(data) {
        const timestamp = Math.floor(Date.now() / 1000);
        const message = JSON.stringify(data) + ':' + timestamp;
        const signature = await this.signMessage(message);
        
        return {
            signature,
            timestamp,
            data
        };
    }

    /**
     * Validate CEO key against expected public key
     */
    async validateCeoKey(expectedPublicKeyHex) {
        const currentPublicKey = this.getCeoPublicKey();
        
        if (!currentPublicKey) {
            return { valid: false, reason: 'No CEO key configured' };
        }
        
        if (currentPublicKey.toLowerCase() !== expectedPublicKeyHex.toLowerCase()) {
            return { 
                valid: false, 
                reason: `CEO key mismatch. Expected: ${expectedPublicKeyHex}, Current: ${currentPublicKey}` 
            };
        }
        
        return { valid: true };
    }

    /**
     * Get session info
     */
    getSessionInfo() {
        const session = this.getSession();
        if (!session) {
            return {
                authenticated: false,
                sessionStartTime: null,
                remainingTime: 0
            };
        }
        
        const remainingTime = Math.max(0, this.sessionTimeout - (Date.now() - session.timestamp));
        
        return {
            authenticated: true,
            sessionStartTime: session.timestamp,
            remainingTime,
            publicKey: session.publicKey,
            type: session.type
        };
    }

    /**
     * Logout
     */
    logout() {
        this.clearSession();
        // Also clear legacy key storage
        localStorage.removeItem('ceoPrivateKey');
    }

    // Utility methods
    _hexToBytes(hex) {
        hex = hex.replace(/^0x/, '');
        const bytes = new Uint8Array(hex.length / 2);
        for (let i = 0; i < bytes.length; i++) {
            bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
        }
        return bytes;
    }

    _bytesToHex(bytes) {
        return Array.from(bytes)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
    }
}

// Export for use in browser and Node.js
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { AuthService };
}
