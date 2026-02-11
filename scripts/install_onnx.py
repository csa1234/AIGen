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

"""
Setup script to download and install ONNX Runtime for the AIGEN inference service.

This script automatically:
1. Detects the operating system (Windows, Linux, macOS)
2. Downloads the appropriate ONNX Runtime library (v1.24.1)
3. Extracts the shared library (dll, so, dylib)
4. Places it in the correct location for the inference engine to find

Usage:
    python scripts/install_onnx.py
"""

import os
import sys
import platform
import shutil
import urllib.request
import zipfile
import tarfile
import tempfile
from pathlib import Path

# ONNX Runtime Version
# Must match the version compatible with the 'ort' crate version used in Cargo.toml
ORT_VERSION = "1.24.1"

# URL Templates
BASE_URL = "https://github.com/microsoft/onnxruntime/releases/download/v{version}/{filename}"

# Platform Mapping
PLATFORM_MAP = {
    'Windows': {
        'filename': f'onnxruntime-win-x64-{ORT_VERSION}.zip',
        'lib_path': f'onnxruntime-win-x64-{ORT_VERSION}/lib/onnxruntime.dll',
        'target_name': 'onnxruntime.dll'
    },
    'Linux': {
        'filename': f'onnxruntime-linux-x64-{ORT_VERSION}.tgz',
        'lib_path': f'onnxruntime-linux-x64-{ORT_VERSION}/lib/libonnxruntime.so.{ORT_VERSION}',
        'target_name': 'libonnxruntime.so'
    },
    'Darwin': { # macOS
        'filename': f'onnxruntime-osx-arm64-{ORT_VERSION}.tgz', # Defaulting to Apple Silicon for now, can be improved
        'lib_path': f'onnxruntime-osx-arm64-{ORT_VERSION}/lib/libonnxruntime.{ORT_VERSION}.dylib',
        'target_name': 'libonnxruntime.dylib'
    }
}

def get_platform_info():
    system = platform.system()
    machine = platform.machine()
    
    if system == 'Darwin' and machine == 'x86_64':
        # Adjust for Intel Mac if needed, though most dev is on Apple Silicon now
        return system, f'onnxruntime-osx-x86_64-{ORT_VERSION}.tgz', f'onnxruntime-osx-x86_64-{ORT_VERSION}/lib/libonnxruntime.{ORT_VERSION}.dylib'
    
    if system not in PLATFORM_MAP:
        print(f"Error: Unsupported operating system: {system}")
        sys.exit(1)
        
    return system, PLATFORM_MAP[system]['filename'], PLATFORM_MAP[system]['lib_path'], PLATFORM_MAP[system]['target_name']

def download_file(url, dest_path):
    print(f"Downloading {url}...")
    try:
        with urllib.request.urlopen(url) as response, open(dest_path, 'wb') as out_file:
            shutil.copyfileobj(response, out_file)
    except Exception as e:
        print(f"Error downloading file: {e}")
        sys.exit(1)

def extract_library(archive_path, lib_internal_path, target_path, is_zip):
    print(f"Extracting library from {archive_path}...")
    try:
        if is_zip:
            with zipfile.ZipFile(archive_path, 'r') as zip_ref:
                # Zip files use forward slashes internally usually
                # We need to read the specific file and write it to target
                with zip_ref.open(lib_internal_path) as source, open(target_path, 'wb') as target:
                    shutil.copyfileobj(source, target)
        else: # tar.gz
            with tarfile.open(archive_path, 'r:gz') as tar_ref:
                member = tar_ref.getmember(lib_internal_path)
                with tar_ref.extractfile(member) as source, open(target_path, 'wb') as target:
                    shutil.copyfileobj(source, target)
    except Exception as e:
        print(f"Error extracting library: {e}")
        # List contents to help debug
        print("Archive contents:")
        try:
            if is_zip:
                with zipfile.ZipFile(archive_path, 'r') as z:
                    for n in z.namelist(): print(f" - {n}")
            else:
                with tarfile.open(archive_path, 'r:gz') as t:
                    for n in t.getnames(): print(f" - {n}")
        except:
            pass
        sys.exit(1)

def main():
    print("=" * 60)
    print(f"AIGEN ONNX Runtime Installer (v{ORT_VERSION})")
    print("=" * 60)

    # 1. Detect Platform
    system, filename, lib_internal_path, target_name = get_platform_info()
    print(f"Detected Platform: {system} ({platform.machine()})")
    
    # 2. Determine paths
    project_root = Path(__file__).parent.parent.absolute()
    target_path = project_root / target_name
    
    print(f"Target Library: {target_path}")
    
    if target_path.exists():
        print("ONNX Runtime library already exists. Skipping installation.")
        print("To reinstall, delete the file and run this script again.")
        return

    # 3. Download
    url = BASE_URL.format(version=ORT_VERSION, filename=filename)
    
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_archive = Path(temp_dir) / filename
        download_file(url, temp_archive)
        
        # 4. Extract
        is_zip = filename.endswith('.zip')
        extract_library(temp_archive, lib_internal_path, target_path, is_zip)

    print("\n" + "=" * 60)
    print("Installation Complete!")
    print("=" * 60)
    print(f"Library installed at: {target_path}")
    print("\nNext steps:")
    
    if system == 'Windows':
        print("1. The library 'onnxruntime.dll' is in your project root.")
        print("   The AIGEN node will automatically find it when running from this directory.")
        print("   No environment variable configuration is required for local development.")
    else:
        print(f"1. The library '{target_name}' is in your project root.")
        print("2. You may need to set the library path if the node fails to find it:")
        print(f"   export ORT_DYLIB_PATH={target_path}")

if __name__ == "__main__":
    main()
