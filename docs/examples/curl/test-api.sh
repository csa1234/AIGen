#!/usr/bin/env sh
# Copyright (c) 2025-present Cesar Saguier Antebi
#
# This file is part of AIGEN Blockchain.
#
# Licensed under the Business Source License 1.1 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at the root of this repository.
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
set -eu

# AIGEN JSON-RPC smoke test script.
# Usage:
#   ./test-api.sh
#   RPC=http://localhost:9944 ./test-api.sh

RPC="${RPC:-http://localhost:9944}"

post() {
  method="$1"
  params="$2"

  curl -s -X POST "$RPC" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}"
  echo ""
}

echo "RPC: $RPC"

echo "== health =="
post "health" "[]"

echo "== getChainInfo =="
post "getChainInfo" "[]"

echo "== getPendingTransactions(limit=10) =="
post "getPendingTransactions" "[10]"

echo "== submitTransaction (schema smoke test) =="

# This demonstrates the *accepted JSON shape*. The node will reject this call unless:
# - sender_public_key is a real 32-byte hex public key
# - sender matches the address derived from sender_public_key
# - signature is a valid Ed25519 signature over tx_hash bytes

SENDER="${SENDER:-0xSENDER_ADDRESS}"
SENDER_PUBLIC_KEY="${SENDER_PUBLIC_KEY:-0xSENDER_PUBLIC_KEY_32_BYTES}"
RECEIVER="${RECEIVER:-0xRECEIVER_ADDRESS}"
AMOUNT="${AMOUNT:-1}"
TIMESTAMP="${TIMESTAMP:-1730000000}"
NONCE="${NONCE:-1}"
PRIORITY="${PRIORITY:-false}"
CHAIN_ID="${CHAIN_ID:-1}"
FEE_BASE="${FEE_BASE:-1}"
FEE_PRIORITY="${FEE_PRIORITY:-0}"
FEE_BURN="${FEE_BURN:-0}"

# tx_hash derivation matches blockchain_core::hash_transaction:
# SHA-256 over UTF-8 JSON bytes of a structure that includes fee and chain_id.
TX_HASH=$(python - <<'PY'
import hashlib, json, os
payload = {
  "sender": os.environ["SENDER"],
  "receiver": os.environ["RECEIVER"],
  "amount": int(os.environ["AMOUNT"]),
  "timestamp": int(os.environ["TIMESTAMP"]),
  "nonce": int(os.environ["NONCE"]),
  "priority": os.environ["PRIORITY"].lower() == "true",
  "chain_id": int(os.environ["CHAIN_ID"]),
  "fee": {
    "base_fee": int(os.environ["FEE_BASE"]),
    "priority_fee": int(os.environ["FEE_PRIORITY"]),
    "burn_amount": int(os.environ["FEE_BURN"]),
  },
  "payload": None,
}
b = json.dumps(payload, separators=(",", ":")).encode("utf-8")
print("0x" + hashlib.sha256(b).hexdigest())
PY
)

SIGNATURE="${SIGNATURE:-0xSIGNATURE_64_BYTES}"

post "submitTransaction" "[{\"sender\":\"${SENDER}\",\"sender_public_key\":\"${SENDER_PUBLIC_KEY}\",\"receiver\":\"${RECEIVER}\",\"amount\":${AMOUNT},\"signature\":\"${SIGNATURE}\",\"timestamp\":${TIMESTAMP},\"nonce\":${NONCE},\"priority\":${PRIORITY},\"tx_hash\":\"${TX_HASH}\",\"fee_base\":${FEE_BASE},\"fee_priority\":${FEE_PRIORITY},\"fee_burn\":${FEE_BURN},\"chain_id\":${CHAIN_ID},\"payload\":null,\"ceo_signature\":null}]"

# getBlock expects positional params (block_height, block_hash), e.g.:
# post "getBlock" "[0,null]"
# post "getBlock" "[null,\"0x...\"]"
