# AIGEN Deployment Guide

This guide provides a step-by-step walkthrough for deploying the AIGEN node, starting the admin dashboard, and using the playground.

## Prerequisites

- **Rust**: Ensure you have the latest stable Rust installed (`rustup update`).
- **Python 3**: For serving the dashboard (or Node.js).
- **Git**: For version control.

---

## Step 1: Start the AIGEN Node

The node is the core of the network. It handles the blockchain, RPC, and AI model verification.

1.  **Open a Terminal** (PowerShell or Command Prompt).
2.  **Navigate to the project root**:
    ```powershell
    cd D:\Code\AIGEN
    ```
3.  **Build and Run the Node**:
    We use the `--release` flag for performance.
    ```powershell
    cargo run --release --bin node -- start
    ```
4.  **Verify Startup**:
    Look for output similar to:
    ```
    node started; waiting for shutdown
    rpc=127.0.0.1:9944
    ```
    This indicates the node is running and listening for RPC connections on port 9944.

    > **Note on config location**: by default the node loads config from your user profile, typically:
    > `C:\Users\<you>\AppData\Roaming\aigen\config.toml`.
    > If you edit `D:\Code\AIGEN\node\config.toml`, it will NOT affect runs that load the user config unless you explicitly pass `--config`.

    > **Troubleshooting**: If you see "Address already in use", make sure no other node instances are running. Use Task Manager to kill `node.exe` if necessary.

5.  **(Optional) Bind RPC to 0.0.0.0** (only needed for access from other machines):
    ```powershell
    cargo run --release --bin node -- start --rpc-addr 0.0.0.0:9944
    ```

---

## Step 2: Start the Admin Dashboard

The dashboard is a web interface to interact with your node. It must be served over HTTP to avoid CORS issues.

1.  **Open a NEW Terminal**.
2.  **Navigate to the dashboard directory**:
    ```powershell
    cd D:\Code\AIGEN\dashboard
    ```
3.  **Start a Local HTTP Server**:
    Using Python (built-in):
    ```powershell
    python -m http.server 8000
    ```
    Or using Node.js (if installed):
    ```powershell
    npx http-server -p 8000
    ```
4.  **Access the Dashboard**:
    - Open your browser (Chrome/Edge/Firefox).
    - Go to: **[http://localhost:8000](http://localhost:8000)**
5.  **Configure Connection**:
    - If you see a connection error, click the **Settings (Gear Icon)** in the top right.
    - **RPC URL**: `http://127.0.0.1:9944`
    - **WebSocket URL**: `ws://127.0.0.1:9944`
    - Click **Save Settings**.
    - The status indicator should turn **Green (Connected)**.

---

## Step 3: Configure CEO Keys (Important)

To perform admin actions (like initializing models or voting), you need the CEO private key.

1.  **Generate a Keypair** (if you haven't already):
    In a terminal:
    ```powershell
    cd D:\Code\AIGEN
    cargo run --release --bin generate-ceo-keypair
    ```
    Copy the **Private Key** (64-byte hex).

2.  **Enter Key in Dashboard**:
    - Go to Dashboard Settings.
    - Paste the key into **CEO Private Key**.
    - Click **Save Settings**.

---

## Step 4: Using the Playground (Optional)

The playground allows you to test AI model inference directly.

1.  **Open a NEW Terminal**.
2.  **Navigate to the playground directory**:
    ```powershell
    cd D:\Code\AIGEN\docs\playground
    ```
3.  **Start HTTP Server**:
    ```powershell
    python -m http.server 8001
    ```
4.  **Access Playground**:
    - Go to: **[http://localhost:8001](http://localhost:8001)**
    - Ensure it is configured to connect to `http://127.0.0.1:9944`.

---

## Common Issues & Fixes

### "Node not running or CORS blocked"
- **Cause**: The dashboard cannot reach the node.
- **Fix**:
    1. Ensure the node terminal is running and shows "rpc=...".
    2. Ensure the dashboard is configured to use `127.0.0.1:9944` (not `localhost`).
    3. If you built the node before today, rebuild and restart it: the RPC server now sends proper browser CORS headers.
       ```powershell
       cargo build --release -p node
       cargo run --release --bin node -- start
       ```

### "Used HTTP Method is not allowed" (in browser)
- **Cause**: You are visiting the RPC port (`9944`) directly in the browser.
- **Meaning**: The node IS working! It just expects API calls (POST), not browser visits (GET).
- **Fix**: Go back to the dashboard (`http://localhost:8000`) to interact with it.

### "Failed to fetch"
- **Cause**: Network restriction or wrong port.
- **Fix**: Disable any VPNs or strict firewalls that might block local ports 9944 or 8000.
