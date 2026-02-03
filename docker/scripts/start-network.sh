#!/usr/bin/env bash
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
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${ROOT_DIR}" 2>/dev/null || true

if command -v docker-compose >/dev/null 2>&1; then
  DC="docker-compose"
else
  DC="docker compose"
fi

echo "Starting AIGEN testnet..."
${DC} up -d

echo "Waiting for services to become healthy..."
SERVICES=(bootstrap-node full-node-1 full-node-2 miner-node validator-node)

for s in "${SERVICES[@]}"; do
  echo "- ${s}"
  # Best-effort wait loop
  for i in $(seq 1 60); do
    status=$(docker inspect --format='{{.State.Health.Status}}' "aigen-${s//-/-}" 2>/dev/null || docker inspect --format='{{.State.Health.Status}}' "aigen-${s}" 2>/dev/null || echo "")
    if [ "${status}" = "healthy" ]; then
      break
    fi
    sleep 2
  done

done

echo "RPC endpoints:"
echo "- bootstrap:  http://localhost:9944"
echo "- full-node-1: http://localhost:9945"
echo "- full-node-2: http://localhost:9946"
echo "- miner:      http://localhost:9947"
echo "- validator:  http://localhost:9948"

echo "Example:"
echo "curl -X POST http://localhost:9944 -H 'Content-Type: application/json' -d '{\"jsonrpc\":\"2.0\",\"method\":\"getChainInfo\",\"params\":[],\"id\":1}'"
