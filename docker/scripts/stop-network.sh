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
