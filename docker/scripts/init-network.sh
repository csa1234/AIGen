#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DOCKER_DIR="${ROOT_DIR}/docker"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

require_cmd docker

if ! docker info >/dev/null 2>&1; then
  echo "Docker does not seem to be running." >&2
  exit 1
fi

echo "Building Docker image aigen-node:latest..."
docker build -t aigen-node:latest "${ROOT_DIR}"

mkdir -p "${DOCKER_DIR}/data/bootstrap" \
         "${DOCKER_DIR}/data/full-node-1" \
         "${DOCKER_DIR}/data/full-node-2" \
         "${DOCKER_DIR}/data/miner" \
         "${DOCKER_DIR}/data/validator"

keygen_node() {
  local name="$1"
  local dir="$2"
  local out="${dir}/node_keypair.bin"

  if [ -f "${out}" ]; then
    echo "Keypair already exists for ${name}: ${out}"
  else
    echo "Generating keypair for ${name}..."
    docker run --rm -v "${dir}:/data" aigen-node:latest keygen --output /data/node_keypair.bin
  fi

  docker run --rm -v "${dir}:/data" aigen-node:latest keygen --output /data/_tmp_keypair.bin --show-peer-id 2>&1 | true
}

# Generate keypairs
keygen_node "bootstrap-node" "${DOCKER_DIR}/data/bootstrap"
keygen_node "full-node-1" "${DOCKER_DIR}/data/full-node-1"
keygen_node "full-node-2" "${DOCKER_DIR}/data/full-node-2"
keygen_node "miner-node" "${DOCKER_DIR}/data/miner"
keygen_node "validator-node" "${DOCKER_DIR}/data/validator"

# Extract bootstrap peer id from existing keypair
BOOTSTRAP_PEER_ID=$(docker run --rm -v "${DOCKER_DIR}/data/bootstrap:/data" aigen-node:latest keygen --output /data/_tmp.bin --show-peer-id 2>&1 | sed -n 's/.*peer_id=\([^ ]*\).*/\1/p' | tail -n 1)

if [ -z "${BOOTSTRAP_PEER_ID}" ]; then
  echo "Failed to determine bootstrap peer id." >&2
  exit 1
fi

rm -f "${DOCKER_DIR}/data/bootstrap/_tmp.bin" "${DOCKER_DIR}/data/bootstrap/_tmp_keypair.bin" \
      "${DOCKER_DIR}/data/full-node-1/_tmp_keypair.bin" \
      "${DOCKER_DIR}/data/full-node-2/_tmp_keypair.bin" \
      "${DOCKER_DIR}/data/miner/_tmp_keypair.bin" \
      "${DOCKER_DIR}/data/validator/_tmp_keypair.bin" || true

echo "Bootstrap peer id: ${BOOTSTRAP_PEER_ID}"

# Patch configs
for cfg in "${DOCKER_DIR}/configs/full-node-1.toml" \
           "${DOCKER_DIR}/configs/full-node-2.toml" \
           "${DOCKER_DIR}/configs/miner.toml" \
           "${DOCKER_DIR}/configs/validator.toml"; do
  if [ -f "${cfg}" ]; then
    sed -i.bak "s/BOOTSTRAP_PEER_ID/${BOOTSTRAP_PEER_ID}/g" "${cfg}"
    rm -f "${cfg}.bak" || true
  fi
done

echo "Network initialized."
echo "Topology:"
echo "- bootstrap-node: /ip4/172.20.0.10/tcp/9000/p2p/${BOOTSTRAP_PEER_ID}"
