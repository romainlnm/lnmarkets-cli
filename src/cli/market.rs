use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::Serialize;
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::Ticker;
use super::output::{print_single, print_list, format_price};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct IndexEntry {
    pub time: String,
    pub index: f64,
}

#[derive(Subcommand)]
pub enum MarketCommands {
    /// Get current ticker (bid, offer, index)
    Ticker {
        /// Watch live updates via the WebSocket stream (Ctrl+C to exit)
        #[arg(long)]
        watch: bool,
    },

    /// Get price/index history
    Prices {
        /// Start timestamp (ISO format or Unix ms)
        #[arg(long)]
        from: Option<String>,

        /// End timestamp (ISO format or Unix ms)
        #[arg(long)]
        to: Option<String>,

        /// Maximum number of data points
        #[arg(short, long, default_value = "100")]
        limit: u32,
    },

    /// Get index history (alias for prices)
    Index {
        /// Start timestamp (ISO format or Unix ms)
        #[arg(long)]
        from: Option<String>,

        /// End timestamp (ISO format or Unix ms)
        #[arg(long)]
        to: Option<String>,

        /// Maximum number of data points
        #[arg(short, long, default_value = "100")]
        limit: u32,
    },

    /// Get market information (limits, leverage)
    Info,

    /// Get current funding rate (from ticker)
    Funding,
}

#[derive(Debug, Tabled, Serialize)]
pub struct TickerRow {
    #[tabled(rename = "Index")]
    pub index: String,
    #[tabled(rename = "Bid")]
    pub bid: String,
    #[tabled(rename = "Ask")]
    pub ask: String,
    #[tabled(rename = "Last Price")]
    pub last_price: String,
    #[tabled(rename = "Funding Rate")]
    pub funding_rate: String,
}

impl From<Ticker> for TickerRow {
    fn from(t: Ticker) -> Self {
        // Get best bid/ask from the first price level (smallest size tier)
        let (bid, ask) = t.prices.first()
            .map(|p| (format_price(p.bid_price), format_price(p.ask_price)))
            .unwrap_or_else(|| ("-".to_string(), "-".to_string()));

        Self {
            index: format_price(t.index),
            bid,
            ask,
            last_price: t.last_price.map(format_price).unwrap_or_else(|| "-".to_string()),
            funding_rate: t.funding_rate
                .map(|r| format!("{:.6}%", r * 100.0))
                .unwrap_or_else(|| "-".to_string()),
        }
    }
}

#[derive(Debug, Tabled, Serialize)]
pub struct IndexRow {
    #[tabled(rename = "Time")]
    pub time: String,
    #[tabled(rename = "Index")]
    pub index: String,
}

impl From<IndexEntry> for IndexRow {
    fn from(e: IndexEntry) -> Self {
        Self {
            time: e.time,
            index: format_price(e.index),
        }
    }
}

impl MarketCommands {
    pub async fn execute(&self, client: &LnmClient, format: OutputFormat) -> Result<()> {
        match self {
            Self::Ticker { watch } => {
                let mut ticker: Ticker =
                    client.public_request(Method::GET, "futures/ticker").await?;
                if !watch {
                    print_single(TickerRow::from(ticker), format)?;
                } else {
                    watch_ticker(&mut ticker, format).await?;
                }
            }

            Self::Prices { from, to, limit } | Self::Index { from, to, limit } => {
                // v3 API uses oracle/index for price history
                let mut path = format!("oracle/index?limit={}", limit);
                if let Some(f) = from {
                    path.push_str(&format!("&from={}", urlencoding::encode(f)));
                }
                if let Some(t) = to {
                    path.push_str(&format!("&to={}", urlencoding::encode(t)));
                }

                let data: Vec<IndexEntry> = client.public_request(Method::GET, &path).await?;
                let rows: Vec<IndexRow> = data.into_iter().map(IndexRow::from).collect();
                print_list(rows, format)?;
            }

            Self::Info => {
                // The ticker endpoint includes all market info
                let info: serde_json::Value = client.public_request(Method::GET, "futures/ticker").await?;
                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&info)?),
                    OutputFormat::JsonPretty | OutputFormat::Table => {
                        println!("{}", serde_json::to_string_pretty(&info)?);
                    }
                }
            }

            Self::Funding => {
                let ticker: Ticker = client.public_request(Method::GET, "futures/ticker").await?;
                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        let funding = serde_json::json!({
                            "fundingRate": ticker.funding_rate,
                            "fundingTime": ticker.funding_time
                        });
                        if format == OutputFormat::Json {
                            println!("{}", serde_json::to_string(&funding)?);
                        } else {
                            println!("{}", serde_json::to_string_pretty(&funding)?);
                        }
                    }
                    OutputFormat::Table => {
                        println!("Funding Rate: {}", ticker.funding_rate
                            .map(|r| format!("{:.6}%", r * 100.0))
                            .unwrap_or_else(|| "-".to_string()));
                        println!("Next Funding: {}", ticker.funding_time.unwrap_or_else(|| "-".to_string()));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Re-renders the ticker table in place on every WS push until Ctrl+C.
/// Initial REST fetch supplies the orderbook levels (`prices`); the stream
/// only updates `last_price`, `index`, and funding.
async fn watch_ticker(ticker: &mut Ticker, format: OutputFormat) -> Result<()> {
    use crossterm::{cursor, execute, terminal};
    use std::io::stdout;

    use crate::api::stream::{self, RawStreamMsg};

    let mut rx = stream::start_raw(
        vec!["futures/inverse/btc_usd/ticker".to_string()],
        None,
    );

    let _ = execute!(stdout(), cursor::Hide);

    let render = |t: &Ticker| -> Result<()> {
        let _ = execute!(
            stdout(),
            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All)
        );
        print_single(TickerRow::from(t.clone()), format)?;
        println!("\n(Ctrl+C to exit)");
        Ok(())
    };

    render(ticker)?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            msg = rx.recv() => {
                match msg {
                    Some(RawStreamMsg::Data { data, .. }) => {
                        if let Some(lp) = data.get("lastPrice").and_then(|v| v.as_f64()) {
                            ticker.last_price = Some(lp);
                        }
                        if let Some(idx) = data.get("index").and_then(|v| v.as_f64()) {
                            ticker.index = idx;
                        }
                        if let Some(fund) = data.get("funding") {
                            if let Some(rate) = fund.get("rate").and_then(|v| v.as_f64()) {
                                ticker.funding_rate = Some(rate);
                            }
                        }
                        render(ticker)?;
                    }
                    Some(RawStreamMsg::Status(_)) => {}
                    None => break,
                }
            }
        }
    }

    let _ = execute!(stdout(), cursor::Show);
    Ok(())
}
