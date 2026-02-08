# AIGEN Demo MVP Guide (Local, Windows)

This guide starts a working local demo with:
- AIGEN node (RPC on `127.0.0.1:9944`)
- Admin Dashboard (served on `http://127.0.0.1:8081`)
- Playground (served on `http://127.0.0.1:8082`)

## Prerequisites

- Windows PowerShell
- Rust toolchain installed (`rustup` + stable)
- Python 3 installed (for `python -m http.server`)

## 1) Start The Node

Open **Terminal #1** (PowerShell):

```powershell
cd D:\Code\AIGEN
cargo build --release -p node
cargo run --release --bin node -- start
```

Expected output includes:
- `loaded config: C:\Users\<you>\AppData\Roaming\aigen\config.toml`
- `rpc=127.0.0.1:9944` (or equivalent)

### If you see “os error 10048” (port already in use)

That means another process is already using the RPC or P2P port.

1) Find the process holding the RPC port:
```powershell
netstat -ano | findstr :9944
```

2) Kill the PID shown in the last column:
```powershell
taskkill /PID <PID> /F
```

Then re-run:
```powershell
cargo run --release --bin node -- start
```

### Note: “RPC server binding to localhost only”

That warning is expected for a local demo and is fine for the MVP. It only matters if you want other machines to reach your node.

## 2) Start The Admin Dashboard

Open **Terminal #2**:

```powershell
cd D:\Code\AIGEN\dashboard
python -m http.server 8081
```

Open:
- `http://127.0.0.1:8081`

### Dashboard Settings

In the dashboard, open **Settings** (gear icon) and confirm:
- RPC URL: `http://127.0.0.1:9944`
- WebSocket URL: `ws://127.0.0.1:9944`

If the UI looks stale after code changes, do a hard refresh:
- `Ctrl + Shift + R`

## 3) Blockchain Explorer (Quick Check)

Go to **Blockchain Explorer** tab:
- You should see at least height `0` (genesis block)
- Click **View** on a block row to load its transactions (genesis usually has none)

## 4) AI Models: Initialize From model.yml (Demo-Friendly)

Go to **AI Models** tab → **Initialize New Model**.

### 4.1 Load a model.yml and auto-fill

Use the **Model YAML (optional)** file picker and select:
- `D:\Code\AIGEN\model\Mistral\model.yml`

Click:
- **Auto-fill From YAML**

This fills the form fields from the YAML.

### 4.2 Use the local demo CEO key (required for initNewModel)

Model initialization is a CEO/admin operation and requires a CEO signature.

For the local demo, open **Settings** and click:
- **Use Dev CEO Key (Local Demo)**

Then try **Initialize Model** again.

### About verification hashes (MVP)

For the MVP demo UI, the form will use a placeholder hash when none is provided.
For a production-grade flow you’d compute real shard hashes from the model shards, but that’s not required just to demonstrate the UI flow end-to-end.

## 5) Health & Metrics (What to Expect)

Go to **Health & Metrics** tab:
- RPC Status should show `healthy`
- Peer Count being `0` is expected if you’re running only one node (it counts connected peers, not “self”)
- Chain Height should show `0` initially (until you produce more blocks)

## 6) Start The Playground (For Demo)

Open **Terminal #3**:

```powershell
cd D:\Code\AIGEN\docs\playground
python -m http.server 8082
```

Open:
- `http://127.0.0.1:8082`

In Playground settings, confirm:
- RPC URL: `http://127.0.0.1:9944`
- WebSocket URL: `ws://127.0.0.1:9944`

## 7) Demo Tips / Common Fixes

### “Failed to fetch” in dashboard/playground
- Ensure the node is running in Terminal #1
- Ensure you are serving dashboard/playground over HTTP (not `file://`)
- Ensure URLs use `127.0.0.1` and the correct port

### Browser caching issues
- Hard refresh: `Ctrl + Shift + R`
- Or DevTools → Network → “Disable cache” (while DevTools is open)

### Node config location
- Node config is typically loaded from:
  - `C:\Users\<you>\AppData\Roaming\aigen\config.toml`
- Editing `D:\Code\AIGEN\node\config.toml` won’t affect runs unless you pass `--config`.

