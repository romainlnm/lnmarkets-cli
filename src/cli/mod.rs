pub mod account;
pub mod alert;
pub mod auth;
pub mod funding;
pub mod futures;
pub mod market;
pub mod mcp;
pub mod output;
pub mod recap;
pub mod stream;

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

    /// Evaluate the daemon journal (offline performance stats)
    JournalStats(JournalStatsArgs),

    /// BTC market recap (24-48h overview)
    Recap(recap::RecapArgs),
    /// Launch interactive TUI dashboard
    Tui(TuiArgs),
    /// Subscribe to live WebSocket streams (scripting / tailing)
    #[command(subcommand)]
    Stream(stream::StreamCommands),

    /// Price / funding alerts with OS-native notifications
    #[command(subcommand)]
    Alert(alert::AlertCommands),
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

    /// Maximum position size in USD
    #[arg(long, default_value = "10")]
    pub max_position: u64,

    /// Leverage (1-100)
    #[arg(long, default_value = "10")]
    pub leverage: u32,

    /// Take profit percentage (e.g., 10 = close at +10% Net ROE)
    #[arg(long, default_value = "10")]
    pub take_profit: f64,

    /// Stop loss percentage (e.g., 5 = close at -5% Net ROE)
    #[arg(long, default_value = "5")]
    pub stop_loss: f64,

    /// Trailing stop percentage - close if ROE drops this much from peak (e.g., 3 = close if drops 3% from peak)
    #[arg(long, default_value = "3")]
    pub trailing_stop: f64,

    /// Daily loss circuit breaker in sats - stop opening new positions once
    /// net realized losses reach this amount in a UTC day (closes still run)
    #[arg(long)]
    pub max_daily_loss: Option<u64>,

    /// Data collectors to enable (comma-separated: pattern,flow,macro,news,whale)
    #[arg(
        short = 'c',
        long,
        alias = "agents",
        short_alias = 'a',
        value_delimiter = ',',
        default_value = "pattern,flow"
    )]
    pub collectors: Vec<String>,
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

/// Arguments for the journal-stats command
#[derive(clap::Args, Debug)]
pub struct JournalStatsArgs {
    /// Path to the journal file (defaults to <config>/lnmarkets/daemon_journal.jsonl)
    #[arg(short, long)]
    pub file: Option<String>,

    /// Only include events from this mode (paper, live, dry_run)
    #[arg(short, long)]
    pub mode: Option<String>,
}

/// Arguments for the TUI command
#[derive(clap::Args, Debug)]
pub struct TuiArgs {
    /// Refresh interval in seconds
    #[arg(long, default_value = "5")]
    pub refresh: u64,

    /// Disable the WebSocket stream and use REST polling only (debugging)
    #[arg(long)]
    pub no_stream: bool,
}
