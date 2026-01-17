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
