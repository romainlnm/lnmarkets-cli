use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::Serialize;
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::{Ticker, PriceHistory, Resolution, CarryingFees};
use super::output::{print_single, print_list, format_price};

#[derive(Subcommand)]
pub enum MarketCommands {
    /// Get current ticker (bid, offer, index)
    Ticker,

    /// Get price history (OHLC candles)
    Prices {
        /// Time resolution
        #[arg(short, long, default_value = "d1")]
        resolution: Resolution,

        /// Start timestamp (Unix ms)
        #[arg(long)]
        from: Option<i64>,

        /// End timestamp (Unix ms)
        #[arg(long)]
        to: Option<i64>,

        /// Maximum number of candles
        #[arg(short, long, default_value = "100")]
        limit: u32,
    },

    /// Get index history
    Index {
        /// Start timestamp (Unix ms)
        #[arg(long)]
        from: Option<i64>,

        /// End timestamp (Unix ms)
        #[arg(long)]
        to: Option<i64>,

        /// Maximum number of data points
        #[arg(short, long, default_value = "100")]
        limit: u32,
    },

    /// Get market information (limits, leverage)
    Info,

    /// Get carrying/funding fees history
    Fees {
        /// Start timestamp (Unix ms)
        #[arg(long)]
        from: Option<i64>,

        /// End timestamp (Unix ms)
        #[arg(long)]
        to: Option<i64>,

        /// Maximum number of records
        #[arg(short, long, default_value = "100")]
        limit: u32,
    },
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
pub struct PriceRow {
    #[tabled(rename = "Time")]
    pub time: String,
    #[tabled(rename = "Open")]
    pub open: String,
    #[tabled(rename = "High")]
    pub high: String,
    #[tabled(rename = "Low")]
    pub low: String,
    #[tabled(rename = "Close")]
    pub close: String,
}

impl From<PriceHistory> for PriceRow {
    fn from(p: PriceHistory) -> Self {
        Self {
            time: chrono::DateTime::from_timestamp_millis(p.time)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| p.time.to_string()),
            open: format_price(p.open),
            high: format_price(p.high),
            low: format_price(p.low),
            close: format_price(p.close),
        }
    }
}

#[derive(Debug, Tabled, Serialize)]
pub struct FeeRow {
    #[tabled(rename = "Time")]
    pub time: String,
    #[tabled(rename = "Rate")]
    pub rate: String,
}

impl From<CarryingFees> for FeeRow {
    fn from(f: CarryingFees) -> Self {
        Self {
            time: chrono::DateTime::from_timestamp_millis(f.time)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| f.time.to_string()),
            rate: format!("{:.4}%", f.rate * 100.0),
        }
    }
}

impl MarketCommands {
    pub async fn execute(&self, client: &LnmClient, format: OutputFormat) -> Result<()> {
        match self {
            Self::Ticker => {
                let ticker: Ticker = client.public_request(Method::GET, "futures/ticker").await?;
                print_single(TickerRow::from(ticker), format)?;
            }

            Self::Prices { resolution, from, to, limit } => {
                let mut path = format!(
                    "futures/history/price?resolution={}&limit={}",
                    resolution.to_minutes(),
                    limit
                );
                if let Some(f) = from {
                    path.push_str(&format!("&from={}", f));
                }
                if let Some(t) = to {
                    path.push_str(&format!("&to={}", t));
                }

                let prices: Vec<PriceHistory> = client.public_request(Method::GET, &path).await?;
                let rows: Vec<PriceRow> = prices.into_iter().map(PriceRow::from).collect();
                print_list(rows, format)?;
            }

            Self::Index { from, to, limit } => {
                let mut path = format!("futures/history/index?limit={}", limit);
                if let Some(f) = from {
                    path.push_str(&format!("&from={}", f));
                }
                if let Some(t) = to {
                    path.push_str(&format!("&to={}", t));
                }

                let data: serde_json::Value = client.public_request(Method::GET, &path).await?;
                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&data)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&data)?),
                    OutputFormat::Table => println!("{}", serde_json::to_string_pretty(&data)?),
                }
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

            Self::Fees { from, to, limit } => {
                let mut path = format!("futures/history/carrying-fees?limit={}", limit);
                if let Some(f) = from {
                    path.push_str(&format!("&from={}", f));
                }
                if let Some(t) = to {
                    path.push_str(&format!("&to={}", t));
                }

                let fees: Vec<CarryingFees> = client.public_request(Method::GET, &path).await?;
                let rows: Vec<FeeRow> = fees.into_iter().map(FeeRow::from).collect();
                print_list(rows, format)?;
            }
        }

        Ok(())
    }
}
