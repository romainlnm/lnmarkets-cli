mod agents;
mod api;
mod cli;
mod config;
mod daemon;
mod mcp;
mod models;
mod stats;

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
                max_position_usd: args.max_position,
                leverage: args.leverage,
                take_profit_pct: Some(args.take_profit),
                stop_loss_pct: Some(args.stop_loss),
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

        Commands::Stats(args) => {
            use stats::{StatsDb, format_stats};

            let db = StatsDb::open()?;

            let mode = if args.live {
                Some("live")
            } else if args.paper {
                Some("paper")
            } else {
                None // Show all
            };

            if args.trades {
                // Show recent trades
                let trades = db.get_recent_trades(args.limit, mode)?;
                if trades.is_empty() {
                    println!("No trades recorded yet.");
                } else {
                    println!("\nRecent Trades");
                    println!("{}", "─".repeat(70));
                    for trade in trades {
                        let status = if trade.closed { "CLOSED" } else { "OPEN" };
                        let pnl_str = trade.pnl_sats
                            .map(|p| format!("{:+} sats", p))
                            .unwrap_or_else(|| "—".to_string());
                        let dir_icon = if trade.direction == "long" { "▲" } else { "▼" };
                        println!(
                            "  {} {} ${:.0} @ ${:.0} | {} | {} | {}",
                            dir_icon,
                            trade.direction.to_uppercase(),
                            trade.quantity_usd,
                            trade.entry_price,
                            pnl_str,
                            status,
                            trade.mode.to_uppercase()
                        );
                    }
                    println!();
                }
            } else {
                // Show stats summary
                if mode.is_none() {
                    // Show both live and paper stats
                    let live_stats = db.get_stats(Some("live"))?;
                    let paper_stats = db.get_stats(Some("paper"))?;

                    if live_stats.total_trades > 0 {
                        println!("{}", format_stats(&live_stats, "live"));
                    }
                    if paper_stats.total_trades > 0 {
                        println!("{}", format_stats(&paper_stats, "paper"));
                    }
                    if live_stats.total_trades == 0 && paper_stats.total_trades == 0 {
                        println!("No trades recorded yet. Run the daemon to start trading!");
                    }
                } else {
                    let stats = db.get_stats(mode)?;
                    if stats.total_trades > 0 {
                        println!("{}", format_stats(&stats, mode.unwrap()));
                    } else {
                        println!("No {} trades recorded yet.", mode.unwrap());
                    }
                }
            }
        }
    }

    Ok(())
}
