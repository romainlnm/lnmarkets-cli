pub mod market;
pub mod futures;
pub mod account;
pub mod funding;
pub mod auth;
pub mod output;

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
}
