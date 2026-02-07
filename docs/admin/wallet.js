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

class WalletManager {
    constructor() {
        this.wallet = null;
        this.address = null;
        this.chainId = null;
        this.listeners = new Map();
        this.ctx = window.__web3Context;
        this.mgr = window.__web3Manager;
        if (this.ctx) {
            this.ctx.subscribe((st) => {
                this.address = st.address;
                this.chainId = st.chainId;
            });
        }
    }

    detectWallet() {
        if (typeof window !== 'undefined' && window.ethereum) {
            return {
                name: this.getWalletName(),
                installed: true
            };
        }
        
        return {
            name: null,
            installed: false
        };
    }

    getWalletName() {
        if (!window.ethereum) return null;
        
        if (window.ethereum.isMetaMask) return 'MetaMask';
        if (window.ethereum.isCoinbaseWallet) return 'Coinbase Wallet';
        if (window.ethereum.isTrust) return 'Trust Wallet';
        if (window.ethereum.isBraveWallet) return 'Brave Wallet';
        
        return 'Unknown Wallet';
    }

    async connect() {
        try {
            const st = await this.mgr.connect();
            this.wallet = this.mgr.provider;
            this.address = st.address;
            this.chainId = st.chainId;
            return this.address;
        } catch (error) {
            throw new Error(`Wallet connection failed: ${error.message}`);
        }
    }

    async disconnect() {
        await this.mgr.disconnect();
        this.address = null;
        this.chainId = null;
        this.wallet = null;
        this.listeners.clear();
    }

    async getAddress() {
        return this.address || null;
    }

    async getChainId() {
        return this.chainId || null;
    }

    async signMessage(message) {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        if (!this.address) {
            throw new Error('No wallet connected');
        }

        try {
            // Convert message to UTF-8 bytes
            const encoder = new TextEncoder();
            const messageBytes = encoder.encode(message);

            // Sign using ed25519 (requires CEO private key)
            // Note: This requires the CEO private key to be available
            // In a real implementation, this would be done server-side or using a hardware wallet
            const signature = await this.signEd25519(messageBytes);

            return signature;
        } catch (error) {
            if (error.code === 4001) {
                throw new Error('User rejected the signature request');
            }
            throw new Error(`Signing failed: ${error.message}`);
        }
    }

    async signEd25519(messageBytes) {
        // Check if tweetnacl is available
        if (typeof nacl === 'undefined') {
            throw new Error('tweetnacl library not loaded. Please include tweetnacl-js for ed25519 signing.');
        }

        // Get CEO private key from localStorage or prompt user
        const ceoPrivateKey = localStorage.getItem('ceoPrivateKey');
        if (!ceoPrivateKey) {
            throw new Error('CEO private key not available. Please configure CEO private key in settings.');
        }

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

    async signTypedData(domain, types, value) {
        if (!this.mgr || !this.mgr.provider) throw new Error('No Web3 wallet detected');
        if (!this.address) throw new Error('No wallet connected');
        const provider = this.mgr.provider;
        try {
            const signature = await provider.request({
                method: 'eth_signTypedData_v4',
                params: [this.address, JSON.stringify({ domain, types, value })]
            });
            return signature;
        } catch (error) {
            throw new Error(`Typed data signing failed: ${error.message}`);
        }
    }

    onAccountsChanged(callback) {
        if (!this.mgr || !this.mgr.provider) return;
        const handler = (accounts) => {
            this.address = accounts[0] || null;
            callback(accounts);
        };
        this.mgr.provider.on('accountsChanged', handler);
        this.listeners.set('accountsChanged', handler);
    }

    onChainChanged(callback) {
        if (!this.mgr || !this.mgr.provider) return;
        const handler = (chainId) => {
            const id = typeof chainId === 'string' ? parseInt(chainId, 16) : chainId;
            this.chainId = id;
            callback(this.chainId);
        };
        this.mgr.provider.on('chainChanged', handler);
        this.listeners.set('chainChanged', handler);
    }

    onDisconnect(callback) {
        if (!this.mgr || !this.mgr.provider) return;
        const handler = () => {
            this.address = null;
            this.chainId = null;
            callback();
        };
        this.mgr.provider.on('disconnect', handler);
        this.listeners.set('disconnect', handler);
    }

    setupEventListeners() {}

    verifyCeoAddress(address) {
        const storedCeoAddress = localStorage.getItem('ceoAddress');
        if (!storedCeoAddress) {
            console.warn('CEO address not configured in settings');
            return false;
        }

        return address.toLowerCase() === storedCeoAddress.toLowerCase();
    }

    isWalletConnected() {
        return this.address !== null && this.wallet !== null;
    }

    getWalletInfo() {
        return {
            address: this.address,
            chainId: this.chainId,
            walletName: this.getWalletName(),
            connected: this.isWalletConnected()
        };
    }

    async switchChain(chainId) {
        await this.mgr.ensureNetwork(chainId);
    }

    async addChain(chainConfig) {
        if (!this.mgr || !this.mgr.provider) throw new Error('No Web3 wallet detected');
        await this.mgr.provider.addChain(chainConfig);
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = WalletManager;
}
