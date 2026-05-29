//! `lnmarkets stream` subcommand — long-running WS tails for scripting.
//!
//! Pipes raw subscription events to stdout as one JSON line per event. Status
//! changes (connect / reconnect) go to stderr so stdout stays clean for `jq`.

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;

use crate::api::stream::{self, RawStreamMsg, StreamCredentials, StreamStatus};
use crate::config::Credentials;

#[derive(Subcommand)]
pub enum StreamCommands {
    /// Subscribe to a channel and print each event as a JSON line
    Watch(WatchArgs),
}

#[derive(Args)]
pub struct WatchArgs {
    /// Channel to subscribe to
    #[arg(value_enum)]
    pub channel: StreamChannel,

    /// OHLC resolution (only with channel=ohlc)
    #[arg(long, default_value = "1m")]
    pub resolution: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum StreamChannel {
    /// Aggregated ticker (last price, index, funding)
    Ticker,
    /// Last trade price updates
    Lastprice,
    /// Index price updates
    Index,
    /// Orderbook depth ladder (volume buckets)
    Buckets,
    /// Funding rate updates
    Funding,
    /// OHLC candle updates (see --resolution)
    Ohlc,
    /// Isolated trade lifecycle + cross position state (authenticated)
    Positions,
    /// Cross-margin order events (authenticated)
    Orders,
    /// Deposit + withdrawal events (authenticated)
    Wallet,
    /// Everything the caller has access to
    All,
}

impl StreamChannel {
    fn topics(self, resolution: &str) -> Vec<String> {
        let pair = "futures/inverse/btc_usd";
        match self {
            Self::Ticker => vec![format!("{}/ticker", pair)],
            Self::Lastprice => vec![format!("{}/lastPrice", pair)],
            Self::Index => vec![format!("{}/index", pair)],
            Self::Buckets => vec![format!("{}/buckets", pair)],
            Self::Funding => vec![format!("{}/funding", pair)],
            Self::Ohlc => vec![format!("{}/ohlc/{}", pair, resolution)],
            Self::Positions => vec![
                format!("{}/isolated/trades", pair),
                format!("{}/cross/position", pair),
            ],
            Self::Orders => vec![format!("{}/cross/orders", pair)],
            Self::Wallet => vec!["wallet/deposit".to_string(), "wallet/withdrawal".to_string()],
            Self::All => vec![
                format!("{}/ticker", pair),
                format!("{}/lastPrice", pair),
                format!("{}/index", pair),
                format!("{}/buckets", pair),
                format!("{}/funding", pair),
                format!("{}/ohlc/{}", pair, resolution),
                format!("{}/isolated/trades", pair),
                format!("{}/cross/orders", pair),
                format!("{}/cross/position", pair),
                "wallet/deposit".to_string(),
                "wallet/withdrawal".to_string(),
            ],
        }
    }

    fn requires_auth(self) -> bool {
        matches!(self, Self::Positions | Self::Orders | Self::Wallet)
    }
}

impl StreamCommands {
    pub async fn execute(self, credentials: Credentials) -> Result<()> {
        match self {
            Self::Watch(args) => run_watch(args, credentials).await,
        }
    }
}

async fn run_watch(args: WatchArgs, credentials: Credentials) -> Result<()> {
    let topics = args.channel.topics(&args.resolution);
    let creds = StreamCredentials::from_config(&credentials);

    if args.channel.requires_auth() && creds.is_none() {
        anyhow::bail!(
            "Channel '{:?}' requires authentication. Run `lnmarkets auth login` first.",
            args.channel
        );
    }

    let mut rx = stream::start_raw(topics, creds);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!();
                break;
            }
            msg = rx.recv() => {
                match msg {
                    Some(RawStreamMsg::Status(s)) => {
                        let label = match s {
                            StreamStatus::Connecting => "connecting",
                            StreamStatus::Connected => "connected",
                            StreamStatus::Authenticated => "authenticated",
                            StreamStatus::Disconnected => "disconnected, reconnecting",
                        };
                        eprintln!("[stream] {}", label);
                    }
                    Some(RawStreamMsg::Data { topic, data }) => {
                        let line = json!({ "topic": topic, "data": data });
                        println!("{}", line);
                    }
                    None => break,
                }
            }
        }
    }
    Ok(())
}
