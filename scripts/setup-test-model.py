#!/usr/bin/env python3
# Copyright (c) 2025-present Cesar Saguier Antebi
# All Rights Reserved.
#
# This file is part of the AIGEN Blockchain project.
# Licensed under the Business Source License 1.1 (BUSL-1.1).
# See LICENSE file in the project root for full license information.
#
# Commercial use requires express written consent and royalty agreements.
# Contact: Cesar Saguier Antebi

"""Setup script to generate a minimal test ONNX identity model for AIGEN.

This script creates a simple identity ONNX model that echoes input as output,
suitable for validating the inference pipeline without requiring a multi-gigabyte
production model.

Usage:
    python scripts/setup-test-model.py

Requirements:
    pip install onnx onnxruntime numpy

Output:
    data/models/mistral-7b/model.onnx - The identity ONNX model
"""

import os
import sys
import json
import time
import hashlib
from pathlib import Path

try:
    import onnx
    from onnx import helper, TensorProto
    import numpy as np
except ImportError as e:
    print(f"Error: Missing required dependencies. {e}")
    print("Please install: pip install onnx onnxruntime numpy")
    sys.exit(1)


def create_identity_model(output_path: str) -> int:
    """Create a minimal ONNX identity model.
    
    The model has:
    - Input: 'input_ids' with shape [1, dynamic]
    - Output: 'output' with shape [1, dynamic]
    - Identity operation that copies input to output
    
    Args:
        output_path: Path where the model will be saved
        
    Returns:
        Size of the generated model in bytes
    """
    # Define input with dynamic shape
    input_tensor = helper.make_tensor_value_info(
        'input_ids',
        TensorProto.INT64,
        [1, None]  # [batch_size, dynamic_sequence_length]
    )
    
    # Define output with same dynamic shape
    output_tensor = helper.make_tensor_value_info(
        'output',
        TensorProto.INT64,
        [1, None]
    )
    
    # Create Identity node
    identity_node = helper.make_node(
        'Identity',
        inputs=['input_ids'],
        outputs=['output'],
        name='identity_node'
    )
    
    # Create the graph
    graph = helper.make_graph(
        nodes=[identity_node],
        name='identity_graph',
        inputs=[input_tensor],
        outputs=[output_tensor],
        initializer=[]
    )
    
    # Create the model
    model = helper.make_model(
        graph,
        opset_imports=[helper.make_opsetid('', 13)],
        producer_name='aigen-test-setup',
        producer_version='1.0.0',
        ir_version=8
    )
    
    # Verify the model
    onnx.checker.check_model(model)
    print(f"  Model validation passed")
    
    # Serialize and save
    model_bytes = model.SerializeToString()
    
    # Ensure directory exists
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    
    with open(output_path, 'wb') as f:
        f.write(model_bytes)
    
    model_size = len(model_bytes)
    print(f"  Created: {output_path} ({model_size} bytes)")
    
    return model_size


def compute_file_hash(filepath: str) -> str:
    """Compute SHA-256 hash of a file."""
    sha256 = hashlib.sha256()
    with open(filepath, 'rb') as f:
        for chunk in iter(lambda: f.read(8192), b''):
            sha256.update(chunk)
    return sha256.hexdigest()


def split_model_into_shards(model_path: str, output_dir: str, model_id: str) -> list:
    """Split model file into 4GB shards.
    
    For small test models, this typically creates a single shard.
    
    Args:
        model_path: Path to the ONNX model file
        output_dir: Directory where shards will be saved
        model_id: Model identifier used in shard filenames
        
    Returns:
        List of shard metadata dictionaries
    """
    SHARD_SIZE = 4 * 1024 * 1024 * 1024  # 4GB
    
    os.makedirs(output_dir, exist_ok=True)
    
    file_size = os.path.getsize(model_path)
    print(f"  Splitting model: {model_path} ({file_size} bytes)")
    
    shards = []
    total_shards = max(1, (file_size + SHARD_SIZE - 1) // SHARD_SIZE)
    
    with open(model_path, 'rb') as f:
        shard_index = 0
        while True:
            chunk = f.read(SHARD_SIZE)
            if not chunk:
                break
            
            shard_filename = f"{model_id}_shard_{shard_index}.bin"
            shard_path = os.path.join(output_dir, shard_filename)
            
            with open(shard_path, 'wb') as shard_file:
                shard_file.write(chunk)
            
            shard_hash = compute_file_hash(shard_path)
            shard_size = len(chunk)
            
            shards.append({
                'shard_index': shard_index,
                'total_shards': total_shards,
                'hash': shard_hash,
                'size': shard_size,
                'filename': shard_filename
            })
            
            print(f"  Created shard {shard_index + 1}/{total_shards}: {shard_filename} ({shard_size} bytes)")
            shard_index += 1
    
    return shards


def create_manifest(model_id: str, model_name: str, version: str, 
                    model_path: str, shards: list) -> dict:
    """Create manifest.json for model registration.
    
    Args:
        model_id: Unique model identifier
        model_name: Human-readable model name
        version: Model version string
        model_path: Path to the model file
        shards: List of shard metadata from split_model_into_shards()
        
    Returns:
        Manifest dictionary ready for JSON serialization
    """
    file_size = os.path.getsize(model_path)
    
    verification_hashes = [s['hash'] for s in shards]
    
    shard_infos = [
        {
            'shard_index': s['shard_index'],
            'total_shards': s['total_shards'],
            'hash': s['hash'],
            'size': s['size']
        }
        for s in shards
    ]
    
    manifest = {
        'model_id': model_id,
        'name': model_name,
        'version': version,
        'total_size': file_size,
        'shard_count': len(shards),
        'verification_hashes': verification_hashes,
        'is_core_model': True,
        'minimum_tier': None,
        'is_experimental': False,
        'created_at': int(time.time()),
        'shards': shard_infos
    }
    
    return manifest


def main():
    """Main entry point for the setup script."""
    # Configuration
    MODEL_ID = 'mistral-7b'
    MODEL_NAME = 'Mistral-7B-v0.1'
    VERSION = '1.0.0'
    DATA_DIR = Path('./data')
    
    MODEL_DIR = DATA_DIR / 'models' / MODEL_ID
    SHARD_DIR = MODEL_DIR / 'shards'
    MODEL_PATH = MODEL_DIR / 'model.onnx'
    MANIFEST_PATH = MODEL_DIR / 'manifest.json'
    
    print("=" * 60)
    print("AIGEN Test Model Setup")
    print("=" * 60)
    print(f"\nConfiguration:")
    print(f"  Model ID:   {MODEL_ID}")
    print(f"  Model Name: {MODEL_NAME}")
    print(f"  Data Dir:   {DATA_DIR.absolute()}")
    print(f"  Model Dir:  {MODEL_DIR.absolute()}")
    
    # Step 1: Create directories
    print(f"\n[1/4] Creating directories...")
    MODEL_DIR.mkdir(parents=True, exist_ok=True)
    SHARD_DIR.mkdir(parents=True, exist_ok=True)
    print(f"  Created: {MODEL_DIR}")
    print(f"  Created: {SHARD_DIR}")
    
    # Step 2: Generate ONNX model
    print(f"\n[2/4] Generating identity ONNX model...")
    try:
        model_size = create_identity_model(str(MODEL_PATH))
    except Exception as e:
        print(f"  Error: Failed to create ONNX model: {e}")
        sys.exit(1)
    
    # Step 3: Split into shards
    print(f"\n[3/4] Splitting model into shards...")
    shards = split_model_into_shards(
        str(MODEL_PATH),
        str(SHARD_DIR),
        MODEL_ID
    )
    print(f"  Total shards created: {len(shards)}")
    
    # Step 4: Generate manifest
    print(f"\n[4/4] Creating manifest file...")
    manifest = create_manifest(
        model_id=MODEL_ID,
        model_name=MODEL_NAME,
        version=VERSION,
        model_path=str(MODEL_PATH),
        shards=shards
    )
    
    with open(MANIFEST_PATH, 'w') as f:
        json.dump(manifest, f, indent=2)
    
    print(f"  Created: {MANIFEST_PATH}")
    
    # Summary
    print("\n" + "=" * 60)
    print("Setup Complete!")
    print("=" * 60)
    print(f"\nFiles created:")
    print(f"  Model:      {MODEL_PATH} ({model_size} bytes)")
    print(f"  Shards:     {len(shards)} shard(s) in {SHARD_DIR}")
    print(f"  Manifest:   {MANIFEST_PATH}")
    
    print(f"\nTo use this model, ensure your node/config.toml has:")
    print(f"  [model]")
    print(f"  core_model_id = \"{MODEL_ID}\"")
    print(f"  model_storage_path = \"./data/models\"")
    
    print(f"\nStart the node with:")
    print(f"  cargo run --bin aigen-node -- start --config node/config.toml")
    
    print(f"\nThe node will auto-register the model from the manifest on startup.")
    print("=" * 60)


if __name__ == '__main__':
    main()
