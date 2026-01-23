class WalletManager {
    constructor() {
        this.wallet = null;
        this.address = null;
        this.chainId = null;
        this.listeners = new Map();
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
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected. Please install MetaMask or WalletConnect.');
        }

        try {
            const accounts = await window.ethereum.request({
                method: 'eth_requestAccounts'
            });

            if (accounts.length === 0) {
                throw new Error('No accounts found');
            }

            this.address = accounts[0];
            this.chainId = await window.ethereum.request({ method: 'eth_chainId' });
            this.wallet = window.ethereum;

            this.setupEventListeners();

            return this.address;
        } catch (error) {
            if (error.code === 4001) {
                throw new Error('User rejected the connection request');
            }
            throw new Error(`Wallet connection failed: ${error.message}`);
        }
    }

    async disconnect() {
        this.address = null;
        this.chainId = null;
        this.wallet = null;
        this.listeners.clear();
    }

    async getAddress() {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        try {
            const accounts = await window.ethereum.request({ method: 'eth_accounts' });
            
            if (accounts.length === 0) {
                return null;
            }

            return accounts[0];
        } catch (error) {
            throw new Error(`Failed to get address: ${error.message}`);
        }
    }

    async getChainId() {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        try {
            const chainId = await window.ethereum.request({ method: 'eth_chainId' });
            return parseInt(chainId, 16);
        } catch (error) {
            throw new Error(`Failed to get chain ID: ${error.message}`);
        }
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
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        if (!this.address) {
            throw new Error('No wallet connected');
        }

        try {
            const signature = await window.ethereum.request({
                method: 'eth_signTypedData_v4',
                params: [this.address, JSON.stringify({ domain, types, value })]
            });

            return signature;
        } catch (error) {
            if (error.code === 4001) {
                throw new Error('User rejected the signature request');
            }
            throw new Error(`Typed data signing failed: ${error.message}`);
        }
    }

    onAccountsChanged(callback) {
        if (!window.ethereum) return;

        const handler = (accounts) => {
            this.address = accounts[0] || null;
            callback(accounts);
        };

        window.ethereum.on('accountsChanged', handler);

        this.listeners.set('accountsChanged', handler);
    }

    onChainChanged(callback) {
        if (!window.ethereum) return;

        const handler = (chainId) => {
            this.chainId = parseInt(chainId, 16);
            callback(this.chainId);
        };

        window.ethereum.on('chainChanged', handler);

        this.listeners.set('chainChanged', handler);
    }

    onDisconnect(callback) {
        if (!window.ethereum) return;

        const handler = () => {
            this.address = null;
            this.chainId = null;
            callback();
        };

        window.ethereum.on('disconnect', handler);

        this.listeners.set('disconnect', handler);
    }

    setupEventListeners() {
        if (!window.ethereum) return;

        window.ethereum.on('accountsChanged', (accounts) => {
            this.address = accounts[0] || null;
        });

        window.ethereum.on('chainChanged', (chainId) => {
            this.chainId = parseInt(chainId, 16);
        });

        window.ethereum.on('disconnect', () => {
            this.address = null;
            this.chainId = null;
        });
    }

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
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        try {
            await window.ethereum.request({
                method: 'wallet_switchEthereumChain',
                params: [{ chainId: `0x${chainId.toString(16)}` }]
            });
        } catch (error) {
            if (error.code === 4902) {
                throw new Error('Chain not added to wallet. Please add it manually.');
            }
            throw new Error(`Failed to switch chain: ${error.message}`);
        }
    }

    async addChain(chainConfig) {
        if (!window.ethereum) {
            throw new Error('No Web3 wallet detected');
        }

        try {
            await window.ethereum.request({
                method: 'wallet_addEthereumChain',
                params: [chainConfig]
            });
        } catch (error) {
            throw new Error(`Failed to add chain: ${error.message}`);
        }
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = WalletManager;
}
