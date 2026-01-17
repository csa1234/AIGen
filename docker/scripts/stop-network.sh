#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CLEAN=false
if [ "${1:-}" = "--clean" ]; then
  CLEAN=true
fi

if command -v docker-compose >/dev/null 2>&1; then
  DC="docker-compose"
else
  DC="docker compose"
fi

echo "Stopping AIGEN testnet..."
${DC} down

if [ "${CLEAN}" = "true" ]; then
  echo "Cleaning data directories..."
  rm -rf "${ROOT_DIR}/docker/data" || true
fi
