// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "aigen-node")]
#[command(version, about = "AIGEN L1 Blockchain Node", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize node configuration and generate keypair
    Init(InitArgs),
    /// Start the node
    Start(StartArgs),
    /// Generate a new keypair
    Keygen(KeygenArgs),
    /// Display version information
    Version(VersionArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    /// Config file path
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Data directory path
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Node identifier
    #[arg(long)]
    pub node_id: Option<String>,
    /// Overwrite existing config/keypair
    #[arg(long, default_value_t = false)]
    pub force: bool,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub role: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct StartArgs {
    /// Config file path
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Data directory override
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
    /// Listen address override (multiaddr)
    #[arg(long)]
    pub listen_addr: Option<String>,
    /// Comma-separated bootstrap peers
    #[arg(long)]
    pub bootstrap_peers: Option<String>,
    /// Node type (miner, validator, full_node, light_client)
    #[arg(long)]
    pub node_type: Option<String>,
    /// Validator wallet address (validator nodes)
    #[arg(long)]
    pub validator_address: Option<String>,
    /// Validator stake amount (validator nodes)
    #[arg(long)]
    pub stake_amount: Option<u64>,

    /// RPC bind address override (e.g. 127.0.0.1:9944)
    #[arg(long)]
    pub rpc_addr: Option<String>,

    /// RPC port override (applies to current rpc_addr host if set, else 127.0.0.1)
    #[arg(long)]
    pub rpc_port: Option<u16>,

    /// Disable RPC server
    #[arg(long, default_value_t = false)]
    pub disable_rpc: bool,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub role: Option<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct KeygenArgs {
    /// Output path for keypair
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Display peer ID after generation
    #[arg(long, default_value_t = false)]
    pub show_peer_id: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct VersionArgs {
    /// Show detailed build information
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}
