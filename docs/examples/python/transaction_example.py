# Copyright (c) 2025-present Cesar Saguier Antebi
#
# This file is part of AIGEN Blockchain.
#
# Licensed under the Business Source License 1.1 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     `https://github.com/yourusername/aigen/blob/main/LICENSE`
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import os
import base64
import hashlib
import json
import requests

# Transaction submission example.
# NOTE: The exact transaction schema + signature rules depend on the node implementation.
# This file demonstrates the JSON-RPC request shape and a typical workflow.

RPC_URL = os.environ.get("AIGEN_RPC_URL", "http://localhost:9944")


def rpc(method, params=None, request_id=1):
    if params is None:
        params = []

    payload = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    r = requests.post(RPC_URL, json=payload, headers={"content-type": "application/json"}, timeout=30)
    r.raise_for_status()
    data = r.json()

    if "error" in data and data["error"]:
        raise RuntimeError(data["error"].get("message", "RPC error"))

    return data.get("result")


def main():
    sender = "0xSENDER_ADDRESS"
    sender_public_key = "0xSENDER_PUBLIC_KEY_32_BYTES"
    receiver = "0xRECEIVER_ADDRESS"

    amount = 1
    timestamp = int(__import__("time").time())
    nonce = 1
    priority = False
    chain_id = 1

    fee_base = 1
    fee_priority = 0
    fee_burn = 0

    payload_hex = None
    payload_b64 = None
    if payload_hex is not None:
        payload_bytes = bytes.fromhex(payload_hex.replace("0x", ""))
        payload_b64 = base64.b64encode(payload_bytes).decode("ascii")

    # This object matches blockchain_core::hash_transaction
    hash_payload = {
        "sender": sender,
        "receiver": receiver,
        "amount": amount,
        "timestamp": timestamp,
        "nonce": nonce,
        "priority": priority,
        "chain_id": chain_id,
        "fee": {
            "base_fee": fee_base,
            "priority_fee": fee_priority,
            "burn_amount": fee_burn,
        },
        "payload": payload_b64,
    }

    hash_bytes = json.dumps(hash_payload, separators=(",", ":")).encode("utf-8")
    tx_hash = "0x" + hashlib.sha256(hash_bytes).hexdigest()

    # TODO: signature must be an Ed25519 signature over the raw tx_hash bytes.
    signature = "0xSIGNATURE_64_BYTES"

    tx = {
        "sender": sender,
        "sender_public_key": sender_public_key,
        "receiver": receiver,
        "amount": amount,
        "signature": signature,
        "timestamp": timestamp,
        "nonce": nonce,
        "priority": priority,
        "tx_hash": tx_hash,
        "fee_base": fee_base,
        "fee_priority": fee_priority,
        "fee_burn": fee_burn,
        "chain_id": chain_id,
        "payload": payload_hex,
        "ceo_signature": None,
    }

    print("Submitting tx to:", RPC_URL)
    print("tx:", tx)

    res = rpc("submitTransaction", [tx])
    print("submitTransaction result:", res)

    pending = rpc("getPendingTransactions", [50])
    print("getPendingTransactions(50):", pending)


if __name__ == "__main__":
    main()
