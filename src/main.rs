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
            use stats::{load_trade_ids, calculate_stats, format_stats, TradeInfo};
            use reqwest::Method;

            // Load daemon trade IDs
            let daemon_ids = load_trade_ids()?;

            if daemon_ids.is_empty() {
                println!("No daemon trades recorded yet. Run the daemon with --live to start trading!");
                return Ok(());
            }

            // Fetch trades from API
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;

            // Fetch closed trades
            let closed: Vec<serde_json::Value> = client
                .request(Method::GET, "futures/closed?limit=100", None::<&()>)
                .await
                .unwrap_or_default();

            // Fetch running trades
            let running: Vec<serde_json::Value> = client
                .request(Method::GET, "futures/running", None::<&()>)
                .await
                .unwrap_or_default();

            // Filter to daemon trades only and convert to TradeInfo
            let mut trades: Vec<TradeInfo> = Vec::new();

            for trade in closed.iter().chain(running.iter()) {
                let id = trade["id"].as_str().unwrap_or_default().to_string();
                if !daemon_ids.contains(&id) {
                    continue;
                }

                let side = if trade["side"].as_str() == Some("b") { "long" } else { "short" };
                let quantity = trade["quantity"].as_f64().unwrap_or(0.0);
                let entry_price = trade["price"].as_f64().unwrap_or(0.0);
                let exit_price = trade["exit_price"].as_f64();
                let pl = trade["pl"].as_i64().unwrap_or(0);
                let closed = trade["closed"].as_bool().unwrap_or(false);
                let creation_ts = trade["creation_ts"].as_i64().unwrap_or(0);
                let last_update_ts = trade["last_update_ts"].as_i64().unwrap_or(0);

                trades.push(TradeInfo {
                    id,
                    side: side.to_string(),
                    quantity,
                    entry_price,
                    exit_price,
                    pl,
                    closed,
                    creation_ts,
                    last_update_ts,
                });
            }

            // Sort by creation time
            trades.sort_by_key(|t| t.creation_ts);

            if args.trades {
                // Show recent trades
                println!("\nDaemon Trades ({} total)", trades.len());
                println!("{}", "─".repeat(60));

                let display_trades: Vec<_> = trades.iter().rev().take(args.limit as usize).collect();

                for trade in display_trades {
                    let status = if trade.closed { "CLOSED" } else { "OPEN" };
                    let dir_icon = if trade.side == "long" { "▲" } else { "▼" };
                    let pnl_color = if trade.pl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
                    println!(
                        "  {} {} ${:.0} @ ${:.0} | {}{:+} sats\x1b[0m | {}",
                        dir_icon,
                        trade.side.to_uppercase(),
                        trade.quantity,
                        trade.entry_price,
                        pnl_color,
                        trade.pl,
                        status,
                    );
                }
                println!();
            } else {
                // Show stats summary
                let stats = calculate_stats(&trades);
                println!("{}", format_stats(&stats));
            }
        }
    }

    Ok(())
}
