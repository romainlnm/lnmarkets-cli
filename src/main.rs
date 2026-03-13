mod agents;
mod api;
mod cli;
mod config;
mod daemon;
mod mcp;
mod models;

use anyhow::Result;
use clap::Parser;

use api::LnmClient;
use cli::{Cli, Commands};
use config::Config;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load().unwrap_or_default();
    let format = cli.output;
    let network = cli.network();

    match cli.command {
        Commands::Market(cmd) => {
            let client = LnmClient::new(network, None)?;
            cmd.execute(&client, format).await?;
        }

        Commands::Futures(cmd) => {
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;
            cmd.execute(&client, format).await?;
        }

        Commands::Account(cmd) => {
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;
            cmd.execute(&client, format).await?;
        }

        Commands::Funding(cmd) => {
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;
            cmd.execute(&client, format).await?;
        }

        Commands::Auth(cmd) => {
            cmd.execute(format).await?;
        }

        Commands::Config => {
            println!("Configuration:");
            println!("  Config file: {:?}", Config::config_path()?);
            println!("  Network: {:?}", config.settings.network);
            println!("  Output format: {:?}", config.settings.output_format);
            println!("  Credentials configured: {}", config.has_credentials());
            println!("\nEnvironment variables:");
            println!("  LNM_API_KEY: {}", if std::env::var("LNM_API_KEY").is_ok() { "set" } else { "not set" });
            println!("  LNM_API_SECRET: {}", if std::env::var("LNM_API_SECRET").is_ok() { "set" } else { "not set" });
            println!("  LNM_API_PASSPHRASE: {}", if std::env::var("LNM_API_PASSPHRASE").is_ok() { "set" } else { "not set" });
        }

        Commands::Mcp(args) => {
            use mcp::LnMarketsServer;

            // Load credentials (same as CLI commands)
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;

            // Create and run MCP server with configured services and safety mode
            let server = LnMarketsServer::new(client, &args.services, args.allow_dangerous);
            server.run().await?;
        }

        Commands::Daemon(args) => {
            use daemon::{Daemon, DaemonConfig, TradingMode};

            // Determine trading mode
            let mode = if args.live {
                TradingMode::Live
            } else if args.paper {
                TradingMode::Paper
            } else {
                TradingMode::DryRun
            };

            let daemon_config = DaemonConfig {
                interval_secs: args.interval,
                mode,
                min_confidence: args.min_confidence,
                max_position_sats: args.max_position,
                agents: args.agents.clone(),
            };

            // Only load client for live trading
            let client = if mode == TradingMode::Live {
                let credentials = config.get_credentials();
                Some(LnmClient::new(network, Some(credentials))?)
            } else {
                None
            };

            let daemon = Daemon::new(daemon_config, client);
            daemon.run().await?;
        }
    }

    Ok(())
}
