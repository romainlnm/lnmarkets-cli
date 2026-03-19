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
            use stats::load_trade_ids;
            use reqwest::Method;

            // Load daemon order IDs
            let daemon_ids = load_trade_ids()?;

            if daemon_ids.is_empty() {
                println!("No daemon orders recorded yet. Run the daemon with --live to start trading!");
                return Ok(());
            }

            // Fetch data from API
            let credentials = config.get_credentials();
            let client = LnmClient::new(network, Some(credentials))?;

            // Fetch cross margin filled orders (history)
            let cross_history: serde_json::Value = client
                .request(Method::GET, "futures/cross/orders/filled?limit=100", None::<&serde_json::Value>)
                .await
                .unwrap_or_default();

            let cross_orders: Vec<serde_json::Value> = cross_history["data"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            // Fetch current cross position
            let position: serde_json::Value = client
                .request(Method::GET, "futures/cross/position", None::<&serde_json::Value>)
                .await
                .unwrap_or_default();

            // Filter to daemon orders only
            let daemon_orders: Vec<_> = cross_orders.iter()
                .filter(|o| {
                    let id = o["id"].as_str().unwrap_or_default();
                    daemon_ids.contains(id)
                })
                .collect();

            // Calculate stats from orders
            let total_orders = daemon_orders.len();
            let mut buy_qty = 0.0;
            let mut sell_qty = 0.0;
            let mut total_fees = 0i64;

            // Track weighted prices for P&L calculation
            let mut buy_value = 0.0;  // sum of qty/price (BTC terms)
            let mut sell_value = 0.0; // sum of qty/price (BTC terms)

            for order in &daemon_orders {
                let qty = order["quantity"].as_f64().unwrap_or(0.0);
                let price = order["price"].as_f64().unwrap_or(0.0);
                let fee = order["tradingFee"].as_i64().unwrap_or(0);
                total_fees += fee;

                if price > 0.0 {
                    match order["side"].as_str() {
                        Some("buy") => {
                            buy_qty += qty;
                            buy_value += qty / price;
                        }
                        Some("sell") => {
                            sell_qty += qty;
                            sell_value += qty / price;
                        }
                        _ => {}
                    }
                }
            }

            // Calculate realized P&L for closed portion
            // Inverse perpetual: P&L = (sell_value - buy_value) * 100_000_000
            let closed_qty = buy_qty.min(sell_qty);
            let realized_pl = if closed_qty > 0.0 {
                // Scale to the closed portion
                let buy_ratio = closed_qty / buy_qty.max(0.001);
                let sell_ratio = closed_qty / sell_qty.max(0.001);
                ((sell_value * sell_ratio) - (buy_value * buy_ratio)) * 100_000_000.0
            } else {
                0.0
            } as i64;

            // Current position info
            let pos_qty = position["quantity"].as_f64().unwrap_or(0.0);
            let pos_entry = position["entryPrice"].as_f64().unwrap_or(0.0);
            let pos_margin = position["margin"].as_f64().unwrap_or(0.0);
            let pos_pl = position["pl"].as_f64().unwrap_or(0.0);
            let pos_side = if pos_qty > 0.0 { "LONG" } else if pos_qty < 0.0 { "SHORT" } else { "FLAT" };

            // Calculate total P&L (realized + unrealized)
            let unrealized_pl = pos_pl as i64;
            let total_pl = realized_pl + unrealized_pl;
            let net_pl = total_pl - total_fees; // Net after fees

            // Print stats
            println!("\nDaemon Stats (cross margin)");
            println!("{}", "─".repeat(40));
            println!("Orders placed:   {}", total_orders);
            println!("Total bought:    ${:.0} USD", buy_qty);
            println!("Total sold:      ${:.0} USD", sell_qty);
            println!("Trading fees:    {} sats", total_fees);

            // P&L section
            let realized_color = if realized_pl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
            let net_color = if net_pl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
            println!();
            println!("Realized P&L:    {}{:+} sats\x1b[0m", realized_color, realized_pl);
            println!("Net P&L:         {}{:+} sats\x1b[0m (after fees)", net_color, net_pl);

            // Position info
            println!();
            if pos_qty.abs() > 0.0 {
                let pl_color = if pos_pl >= 0.0 { "\x1b[32m" } else { "\x1b[31m" };
                println!("Open Position:");
                println!("  {} ${:.0} @ ${:.0}", pos_side, pos_qty.abs(), pos_entry);
                println!("  Margin: {:.0} sats", pos_margin);
                println!("  Unrealized P&L: {}{:+.0} sats\x1b[0m", pl_color, pos_pl);
            } else {
                println!("Position: FLAT");
            }

            if args.trades {
                // Show recent orders
                println!("\nDaemon Orders ({} total)", daemon_orders.len());
                println!("{}", "─".repeat(50));

                let display_orders: Vec<_> = daemon_orders.iter().rev().take(args.limit as usize).collect();

                for order in display_orders {
                    let side = order["side"].as_str().unwrap_or("?");
                    let dir_icon = if side == "buy" { "\x1b[32m▲\x1b[0m" } else { "\x1b[31m▼\x1b[0m" };
                    let qty = order["quantity"].as_f64().unwrap_or(0.0);
                    let price = order["price"].as_f64().unwrap_or(0.0);
                    let fee = order["tradingFee"].as_i64().unwrap_or(0);
                    let time = order["filledAt"].as_str().unwrap_or("?");

                    println!(
                        "  {} {} ${:.0} @ ${:.0} (fee: {} sats) - {}",
                        dir_icon,
                        side.to_uppercase(),
                        qty,
                        price,
                        fee,
                        &time[..16], // Truncate timestamp
                    );
                }
            }
            println!();
        }
    }

    Ok(())
}
