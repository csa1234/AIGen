#!/usr/bin/env bash
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
