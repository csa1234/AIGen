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

import axios from "axios";

// Simple JSON-RPC client for AIGEN.
// Usage:
//   node basic-client.js

const RPC_URL = process.env.AIGEN_RPC_URL || "http://localhost:9944";

async function rpc(method, params = [], id = 1) {
  const payload = { jsonrpc: "2.0", id, method, params };
  const { data } = await axios.post(RPC_URL, payload, {
    headers: { "content-type": "application/json" }
  });

  if (data && data.error) {
    const err = new Error(data.error.message || "RPC error");
    err.code = data.error.code;
    err.data = data.error.data;
    throw err;
  }

  return data.result;
}

async function main() {
  console.log("RPC:", RPC_URL);

  const health = await rpc("health");
  console.log("health():", health);

  const chainInfo = await rpc("getChainInfo");
  console.log("getChainInfo():", chainInfo);
}

main().catch((e) => {
  console.error("Error:", e);
  process.exitCode = 1;
});
