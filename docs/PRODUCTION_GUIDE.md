<!--
Copyright (c) 2025-present Cesar Saguier Antebi
This file is part of AIGEN Blockchain.
Licensed under the Business Source License 1.1
-->

# AIGEN Production Deployment Guide

## Overview
- AIGEN uses a custom JSON-RPC and Ed25519 signatures for admin operations; it is not EVM-native.
- EVM wallets can be optionally enabled for identity bridging via configuration; core functions do not depend on eth_*.
- This guide describes preparing nodes, configuring the Admin Dashboard, securing secrets, and validating the setup.

## Prerequisites
- OS: Linux (recommended), macOS, or Windows Server
- Rust toolchain and cargo installed
- Docker or container runtime (optional but recommended)
- Reverse proxy (nginx) or CDN for serving static dashboard

## Build and Node Setup
1. Build:
   ```bash
   cargo build --release
   ```
2. Initialize and start node:
   ```bash
   ./target/release/node init --node-id prod-node-1
   ./target/release/node start
   ```
3. Confirm RPC:
   ```bash
   curl -s http://127.0.0.1:9944 -H 'Content-Type: application/json' \
     -d '{"jsonrpc":"2.0","id":1,"method":"getChainInfo","params":[]}'
   ```

## Admin Dashboard Deployment
1. Host the static files in dashboard with nginx or any static server:
   - Example nginx snippet:
     ```
     server {
       listen 80;
       server_name admin.yourdomain.com;
       root /srv/aigen/dashboard;
       index index.html;
       location / {
         try_files $uri $uri/ /index.html;
       }
     }
     ```
2. Provide production configuration via one of:
   - config.json in dashboard (served alongside index.html)
   - window.__AIGEN_CONFIG injected by your server
3. Sample config.json:
   ```json
   {
     "rpc": { "http": "https://rpc.aigen.yourdomain.com", "ws": "wss://rpc.aigen.yourdomain.com" },
     "web3": {
       "enableEvm": false,
       "requiredChainId": null,
       "walletConnectRpc": { }
     },
     "chains": { }
   }
   ```
4. The dashboard loads this config automatically via [config.js](../dashboard/config.js).

## Environment Configurations
- Laptop (local):
  - Use [config.laptop.sample.json](../dashboard/config.laptop.sample.json) and save as `dashboard/config.json`.
  - Set RPC to `http://127.0.0.1:9944` and `ws://127.0.0.1:9944`.
- Cloud VPS (production):
  - Use [config.vps.sample.json](../dashboard/config.vps.sample.json) and save as `dashboard/config.json`.
  - Set RPC to your public HTTPS/WSS endpoints behind TLS and reverse proxy pointing to the node’s internal port 9944.

## Security and Secrets
- CEO private key used for admin signatures must be managed securely.
- Recommended:
  - Keep CEO key out of browser whenever possible; use hardware wallet or server-side signing proxy.
  - If using localStorage for CEO key (as supported), restrict access and use dedicated admin machine.
  - Do not store RPC credentials or secrets in index.html.

## Optional EVM Wallet Bridging
- Enable via `web3.enableEvm: true` in your config.
- Provide `requiredChainId` and `walletConnectRpc` mappings if enforcing an EVM chain.
- EVM wallets are used only for user identity when needed; admin RPC still relies on Ed25519 signatures.

## Observability and Health
- Health & Metrics require signed admin requests; configure CEO key in Settings.
- Use the dashboard Health & Metrics tab to monitor:
  - Node health (RPC status, peers, memory)
  - AI health (cache hit rate, memory)
  - Blockchain health (height, pending txs, shutdown status)

## End-to-End Validation
1. Node RPC:
   - getChainInfo returns valid result
2. Dashboard:
   - Loads with config
   - Health & Metrics show values after CEO key configured
3. Governance/Admin actions:
   - approveSIP, vetoSIP, shutdown require Ed25519 signatures; verify success via dashboard notifications and RPC results

## Deployment Checklist
- [ ] Nodes started, RPC reachable via HTTPS/WSS
- [ ] Dashboard served over HTTPS
- [ ] config.json or window.__AIGEN_CONFIG provided
- [ ] CEO key management policy documented
- [ ] EVM bridging disabled unless explicitly needed
- [ ] Health & Metrics validated

## Troubleshooting
- Phantom shows Solana-only account: switch to Phantom EVM or use WalletConnect if EVM bridging is enabled.
- Signed calls failing: ensure CEO key is present and formatted as 64-byte hex (seed + public key).
- RPC timeouts: check network, reverse proxy, and node logs.
