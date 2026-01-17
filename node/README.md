# AIGEN Node

## CLI

Binary name: `aigen-node`

### Initialize a new node

```
aigen-node init --node-id node1
```

- Creates the data directory
- Generates and saves a persistent libp2p keypair
- Writes a config file

Optional:

- `--config <PATH>`
- `--data-dir <PATH>`
- `--force`

### Start the node

```
aigen-node start
```

Optional overrides:

- `--config <PATH>`
- `--data-dir <PATH>`
- `--listen-addr <ADDR>`
- `--bootstrap-peers <PEERS>` (comma-separated multiaddrs)
- `--node-type <TYPE>` (miner, validator, full_node, light_client)
- `--validator-address <ADDR>`

### Generate a keypair

```
aigen-node keygen --output keypair.bin --show-peer-id
```

### Version

```
aigen-node version --verbose
```

## Configuration

Supported formats:

- TOML (`.toml`)
- JSON (`.json`)

Default config location:

- Unix: `~/.config/aigen/config.toml` (depends on platform `dirs` resolution)
- Windows: `%APPDATA%\aigen\config.toml`

Default data directory:

- Unix: `~/.local/share/aigen`
- Windows: `%APPDATA%\aigen`

### Precedence

Configuration is applied in this order (highest priority last):

- Defaults
- Config file
- Environment variables (`AIGEN_*`)
- CLI args

## Environment variables

- `AIGEN_NODE_ID`
- `AIGEN_DATA_DIR`
- `AIGEN_LISTEN_ADDR`
- `AIGEN_BOOTSTRAP_PEERS` (comma-separated)
- `AIGEN_MAX_PEERS`
- `AIGEN_ENABLE_MDNS`
- `AIGEN_NODE_TYPE`
- `AIGEN_VALIDATOR_ADDRESS`

## RPC API Documentation

The node can expose a JSON-RPC 2.0 server over HTTP and WebSocket.

Default address:

- `127.0.0.1:9944`

### Public methods

- `getChainInfo()`
- `getBlock(block_height?, block_hash?)`
- `getBalance(address)`
- `submitTransaction(transaction)`
- `getPendingTransactions(limit?)`

### CEO methods

- `submitShutdown(request)`
- `approveSIP(request)`
- `vetoSIP(request)`
- `getSIPStatus(proposal_id)`

### Example curl commands

```bash
# Get chain info
curl -X POST http://localhost:9944 -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getChainInfo","params":[],"id":1}'

# Get balance
curl -X POST http://localhost:9944 -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getBalance","params":["0x..."],"id":1}'
```

### CEO authentication

CEO endpoints require a signature provided in the request payload.

- `submitShutdown`
  - Message format: `shutdown:{network_magic}:{timestamp}:{nonce}:{reason}`
- `approveSIP`
  - Message format: `sip_approve:{proposal_id}`
- `vetoSIP`
  - Message format: `sip_veto:{proposal_id}`

The `signature` field must be hex-encoded (64 bytes), prefixed with `0x`.

### WebSocket subscriptions

Over WebSocket, the server supports:

- `subscribeNewBlocks`
- `subscribeNewTransactions`
- `subscribeShutdown`

Example using `wscat`:

```bash
wscat -c ws://localhost:9944

> {"jsonrpc":"2.0","id":1,"method":"subscribeNewBlocks","params":[]}
```

## Examples

- Initialize:

```
aigen-node init --node-id node-1
```

- Start from a config:

```
aigen-node start --config ./node/config.example.toml
```

- Start with env override:

```
set AIGEN_LISTEN_ADDR=/ip4/0.0.0.0/tcp/9100
set AIGEN_NODE_TYPE=full_node

aigen-node start --config ./node/config.example.toml
```

## Security notes

- Keep the keypair file private.
- On Unix, the keypair file is written with mode 0600.
- On Windows, the keypair inherits the containing directory ACLs.
