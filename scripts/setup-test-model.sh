#!/bin/bash
# Setup test model for AIGEN playground
# Copyright (c) 2025-present Cesar Saguier Antebi
# All Rights Reserved.

set -e

echo "=========================================="
echo "AIGEN Test Model Setup Script"
echo "=========================================="

# Check if Python is available
if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is not installed"
    exit 1
fi

# Check required Python packages
echo "Checking Python dependencies..."
python3 -c "import onnx, numpy" 2>/dev/null || {
    echo "Installing required Python packages..."
    pip install onnx numpy
}

# Run the setup script
echo ""
echo "Running setup script..."
python3 scripts/setup-test-model.py

echo ""
echo "=========================================="
echo "Setup complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "1. Verify node/config.toml has core_model_id = \"mistral-7b\""
echo "2. Start the node: cargo run --bin aigen-node -- start --config node/config.toml"
echo "3. Open docs/playground/index.html in your browser"
