pub mod market;
pub mod futures;
pub mod account;
pub mod funding;
pub mod auth;
pub mod output;
pub mod mcp;

use clap::{Parser, Subcommand};
use crate::config::{Network, OutputFormat};

#[derive(Parser)]
#[command(name = "lnm")]
#[command(author = "LN Markets CLI")]
#[command(version)]
#[command(about = "Command-line interface for LN Markets API v3", long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format
    #[arg(short, long, global = true, default_value = "table")]
    pub output: OutputFormat,

    /// Use testnet instead of mainnet
    #[arg(long, global = true)]
    pub testnet: bool,
}

impl Cli {
    pub fn network(&self) -> Network {
        if self.testnet {
            Network::Testnet
        } else {
            Network::Mainnet
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Market data commands (ticker, prices, index)
    #[command(subcommand)]
    Market(market::MarketCommands),

    /// Futures trading commands
    #[command(subcommand)]
    Futures(futures::FuturesCommands),

    /// Account management commands
    #[command(subcommand)]
    Account(account::AccountCommands),

    /// Funding commands (deposits, withdrawals)
    #[command(subcommand)]
    Funding(funding::FundingCommands),

    /// Authentication and credential management
    #[command(subcommand)]
    Auth(auth::AuthCommands),

    /// Show current configuration
    Config,

    /// Start MCP server for AI agent integration
    #[command(hide = true)]
    Mcp(mcp::McpArgs),

    /// Run trading daemon with automated agents
    Daemon(DaemonArgs),

    /// Show trading statistics
    Stats(StatsArgs),
}

/// Arguments for the daemon command
#[derive(clap::Args, Debug)]
pub struct DaemonArgs {
    /// Analysis interval in seconds
    #[arg(short, long, default_value = "60")]
    pub interval: u64,

    /// Paper trading mode (simulated trades with real prices)
    #[arg(long)]
    pub paper: bool,

    /// Live trading mode (real trades - use with caution!)
    #[arg(long)]
    pub live: bool,

    /// Minimum confidence threshold (0.0-1.0)
    #[arg(long, default_value = "0.7")]
    pub min_confidence: f64,

    /// Maximum position size in USD
    #[arg(long, default_value = "10")]
    pub max_position: u64,

    /// Leverage (1-100)
    #[arg(long, default_value = "10")]
    pub leverage: u32,

    /// Take profit percentage (e.g., 5 = close at +5%)
    #[arg(long, default_value = "5")]
    pub take_profit: f64,

    /// Stop loss percentage (e.g., 3 = close at -3%)
    #[arg(long, default_value = "3")]
    pub stop_loss: f64,

    /// Agents to enable (comma-separated: pattern,macro,news,flow)
    #[arg(short, long, value_delimiter = ',', default_value = "pattern")]
    pub agents: Vec<String>,
}

/// Arguments for the stats command
#[derive(clap::Args, Debug)]
pub struct StatsArgs {
    /// Show recent trades list
    #[arg(short, long)]
    pub trades: bool,

    /// Number of recent trades to show
    #[arg(short, long, default_value = "10")]
    pub limit: u32,
}
