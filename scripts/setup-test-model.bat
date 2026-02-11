@echo off
REM Copyright (c) 2025-present Cesar Saguier Antebi
REM All Rights Reserved.
REM
REM This file is part of the AIGEN Blockchain project.
REM Licensed under the Business Source License 1.1 (BUSL-1.1).
REM See LICENSE file in the project root for full license information.
REM
REM Commercial use requires express written consent and royalty agreements.
REM Contact: Cesar Saguier Antebi

REM Setup script for AIGEN test model on Windows
REM This script generates a test ONNX model, splits it into shards, and creates a manifest

setlocal EnableDelayedExpansion

echo ============================================================
echo AIGEN Test Model Setup (Windows)
echo ============================================================
echo.

REM Default configuration
set MODEL_ID=mistral-7b
set MODEL_NAME=Mistral-7B-v0.1
set DATA_DIR=./data
set USE_PYTHON=false

REM Parse arguments
:parse_args
if "%~1"=="" goto :done_parsing
if "%~1"=="--python" (
    set USE_PYTHON=true
    shift
    goto :parse_args
)
if "%~1"=="--model-id" (
    set MODEL_ID=%~2
    shift
    shift
    goto :parse_args
)
if "%~1"=="--data-dir" (
    set DATA_DIR=%~2
    shift
    shift
    goto :parse_args
)
shift
goto :parse_args
:done_parsing

set MODEL_DIR=%DATA_DIR%/models/%MODEL_ID%
set MODEL_PATH=%MODEL_DIR%/model.onnx
set SHARD_DIR=%MODEL_DIR%/shards
set MANIFEST_PATH=%MODEL_DIR%/manifest.json

echo Configuration:
echo   Model ID:   %MODEL_ID%
echo   Model Name: %MODEL_NAME%
echo   Data Dir:   %DATA_DIR%
echo   Model Dir:  %MODEL_DIR%
echo   Use Python: %USE_PYTHON%
echo.

REM Create directories
echo [1/3] Creating directories...
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"
if not exist "%SHARD_DIR%" mkdir "%SHARD_DIR%"
echo   Created: %MODEL_DIR%
echo   Created: %SHARD_DIR%
echo.

REM Check which method to use
if "%USE_PYTHON%"=="true" (
    echo Using Python method...
    
    REM Check if Python is available
    python --version >nul 2>&1
    if errorlevel 1 (
        echo Error: Python is not installed or not in PATH
        echo Please install Python from https://python.org
        exit /b 1
    )
    
    REM Check if required packages are installed
    python -c "import onnx" >nul 2>&1
    if errorlevel 1 (
        echo Installing required Python packages...
        pip install onnx onnxruntime numpy
        if errorlevel 1 (
            echo Error: Failed to install Python packages
            exit /b 1
        )
    )
    
    echo [2/3] Generating test ONNX model with Python...
    python scripts\setup-test-model.py
    if errorlevel 1 (
        echo Error: Python setup script failed
        exit /b 1
    )
) else (
    echo Using Rust method...
    
    REM Check if cargo is available
    cargo --version >nul 2>&1
    if errorlevel 1 (
        echo Error: Cargo is not installed or not in PATH
        echo Please install Rust from https://rustup.rs
        exit /b 1
    )
    
    echo [2/3] Generating test ONNX model with Rust...
    cargo run --bin setup-test-model -- --model-id %MODEL_ID% --data-dir %DATA_DIR%
    if errorlevel 1 (
        echo Error: Rust setup script failed
        exit /b 1
    )
)

echo.
echo [3/3] Setup verification...

REM Verify files were created
if not exist "%MODEL_PATH%" (
    echo Error: Model file not found: %MODEL_PATH%
    exit /b 1
)

if not exist "%MANIFEST_PATH%" (
    echo Error: Manifest file not found: %MANIFEST_PATH%
    exit /b 1
)

REM Get file sizes
for %%F in ("%MODEL_PATH%") do set MODEL_SIZE=%%~zF
for %%F in ("%MANIFEST_PATH%") do set MANIFEST_SIZE=%%~zF

echo   Model file:    %MODEL_PATH% (%MODEL_SIZE% bytes)
echo   Manifest file: %MANIFEST_PATH% (%MANIFEST_SIZE% bytes)

REM Count shards
set SHARD_COUNT=0
for %%F in ("%SHARD_DIR%\*_shard_*.bin") do set /a SHARD_COUNT+=1
echo   Shard files:   %SHARD_COUNT% in %SHARD_DIR%

echo.
echo ============================================================
echo Setup Complete!
echo ============================================================
echo.
echo To use this model, ensure your node/config.toml has:
echo   [model]
echo   core_model_id = "%MODEL_ID%"
echo   model_storage_path = "./data/models"
echo.
echo Start the node with:
echo   cargo run --bin aigen-node -- start --config node/config.toml
echo.
echo The node will auto-register the model from the manifest on startup.
echo.
echo ============================================================

endlocal
