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
import requests

# Simple JSON-RPC client for AIGEN.
# Usage:
#   python basic_client.py

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
    print("RPC:", RPC_URL)
    print("health():", rpc("health"))
    print("getChainInfo():", rpc("getChainInfo"))


if __name__ == "__main__":
    main()
