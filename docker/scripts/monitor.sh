#!/usr/bin/env bash
set -euo pipefail

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd jq
require_cmd watch

query() {
  local name="$1"
  local url="$2"

  local health
  local chain

  health=$(curl -fsS -X POST "${url}" -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"health","params":[],"id":1}' || echo "")
  chain=$(curl -fsS -X POST "${url}" -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"getChainInfo","params":[],"id":1}' || echo "")

  local status
  local peer_count
  local height
  local latest

  status=$(echo "${health}" | jq -r '.result.status // "error"' 2>/dev/null || echo "error")
  peer_count=$(echo "${health}" | jq -r '.result.peer_count // 0' 2>/dev/null || echo 0)
  height=$(echo "${chain}" | jq -r '.result.height // 0' 2>/dev/null || echo 0)
  latest=$(echo "${chain}" | jq -r '.result.latest_block_hash // ""' 2>/dev/null || echo "")

  printf "%-14s %-10s %-10s %-12s %s\n" "${name}" "${status}" "${height}" "${peer_count}" "${latest}"
}

render() {
  printf "%-14s %-10s %-10s %-12s %s\n" "node_id" "status" "height" "peer_count" "latest_block_hash"
  query "bootstrap" "http://localhost:9944"
  query "full-node-1" "http://localhost:9945"
  query "full-node-2" "http://localhost:9946"
  query "miner" "http://localhost:9947"
  query "validator" "http://localhost:9948"
}

export -f query
export -f render

watch -n 5 bash -lc render
