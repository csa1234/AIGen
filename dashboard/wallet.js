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
        this.listeners = new Map();
    }

    async signMessage(message) {
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

    generateKeypair() {
        if (typeof nacl === 'undefined') {
            throw new Error('tweetnacl library not loaded');
        }
        
        const keyPair = nacl.sign.keyPair();
        
        // Combine seed (32 bytes) + public key (32 bytes) = 64 bytes
        const privateKey = new Uint8Array(64);
        privateKey.set(keyPair.secretKey.slice(0, 32), 0);
        privateKey.set(keyPair.publicKey, 32);
        
        return {
            privateKey: this.bytesToHex(privateKey),
            publicKey: this.bytesToHex(keyPair.publicKey),
            address: this.bytesToHex(keyPair.publicKey) // In AIGEN, address = public key
        };
    }

    derivePublicKey(privateKeyHex) {
        const privateKeyBytes = this.hexToBytes(privateKeyHex);
        if (privateKeyBytes.length !== 64) {
            throw new Error('Private key must be 64 bytes');
        }
        
        const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));
        return this.bytesToHex(keyPair.publicKey);
    }

    verifyKeypair(privateKeyHex) {
        try {
            const privateKeyBytes = this.hexToBytes(privateKeyHex);
            if (privateKeyBytes.length !== 64) return false;
            
            const keyPair = nacl.sign.keyPair.fromSeed(privateKeyBytes.slice(0, 32));
            const derivedPublicKey = this.bytesToHex(keyPair.publicKey);
            const storedPublicKey = this.bytesToHex(privateKeyBytes.slice(32));
            
            return derivedPublicKey === storedPublicKey;
        } catch {
            return false;
        }
    }
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = WalletManager;
}
