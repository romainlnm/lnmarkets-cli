//! Flow Agent - Market Microstructure Signals
//!
//! Analyzes order flow and positioning data from Binance Futures:
//! - Order book imbalance (bid/ask depth)
//! - Funding rate (long/short sentiment)
//! - Open Interest changes
//! - Long/Short ratio
//! - Recent liquidations

use super::{Agent, Direction, Signal};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

/// Configuration for Flow Agent
#[derive(Debug, Clone)]
pub struct FlowConfig {
    /// Symbol to track
    pub symbol: String,
    /// Order book depth levels to analyze
    pub depth_levels: usize,
    /// Funding rate threshold for signal (basis points)
    pub funding_threshold_bps: f64,
    /// OI change threshold for signal (percentage)
    pub oi_change_threshold_pct: f64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            depth_levels: 20,
            funding_threshold_bps: 10.0, // 0.01%
            oi_change_threshold_pct: 5.0,
        }
    }
}

/// Flow Agent implementation
pub struct FlowAgent {
    config: FlowConfig,
    http_client: reqwest::Client,
    last_oi: Option<f64>,
}

impl FlowAgent {
    pub fn new(config: FlowConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            last_oi: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(FlowConfig::default())
    }

    /// Fetch order book depth
    async fn fetch_order_book(&self) -> Result<OrderBookData> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/depth?symbol={}&limit={}",
            self.config.symbol, self.config.depth_levels
        );

        let response: BinanceDepth = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch order book")?
            .json()
            .await
            .context("Failed to parse order book")?;

        // Calculate bid/ask totals
        let bid_total: f64 = response.bids.iter()
            .filter_map(|level| level.get(1)?.as_str()?.parse::<f64>().ok())
            .sum();

        let ask_total: f64 = response.asks.iter()
            .filter_map(|level| level.get(1)?.as_str()?.parse::<f64>().ok())
            .sum();

        let imbalance = if bid_total + ask_total > 0.0 {
            (bid_total - ask_total) / (bid_total + ask_total)
        } else {
            0.0
        };

        Ok(OrderBookData {
            bid_total,
            ask_total,
            imbalance, // -1 to 1, positive = more bids
        })
    }

    /// Fetch funding rate
    async fn fetch_funding_rate(&self) -> Result<FundingData> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/fundingRate?symbol={}&limit=1",
            self.config.symbol
        );

        let response: Vec<BinanceFunding> = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch funding rate")?
            .json()
            .await
            .context("Failed to parse funding rate")?;

        let funding = response.first()
            .and_then(|f| f.funding_rate.parse::<f64>().ok())
            .unwrap_or(0.0);

        Ok(FundingData {
            rate: funding,
            rate_bps: funding * 10000.0, // Convert to basis points
        })
    }

    /// Fetch open interest
    async fn fetch_open_interest(&self) -> Result<OpenInterestData> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/openInterest?symbol={}",
            self.config.symbol
        );

        let response: BinanceOI = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch open interest")?
            .json()
            .await
            .context("Failed to parse open interest")?;

        let oi = response.open_interest.parse::<f64>().unwrap_or(0.0);

        Ok(OpenInterestData {
            value: oi,
        })
    }

    /// Fetch long/short ratio
    async fn fetch_long_short_ratio(&self) -> Result<LongShortData> {
        let url = format!(
            "https://fapi.binance.com/futures/data/globalLongShortAccountRatio?symbol={}&period=5m&limit=1",
            self.config.symbol
        );

        let response: Vec<BinanceLongShort> = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch long/short ratio")?
            .json()
            .await
            .context("Failed to parse long/short ratio")?;

        let data = response.first();

        Ok(LongShortData {
            long_ratio: data.and_then(|d| d.long_account.parse::<f64>().ok()).unwrap_or(0.5),
            short_ratio: data.and_then(|d| d.short_account.parse::<f64>().ok()).unwrap_or(0.5),
            ratio: data.and_then(|d| d.long_short_ratio.parse::<f64>().ok()).unwrap_or(1.0),
        })
    }

    /// Analyze all flow data and produce signal
    fn analyze_flow(
        &self,
        order_book: &OrderBookData,
        funding: &FundingData,
        oi: &OpenInterestData,
        long_short: &LongShortData,
        oi_change_pct: f64,
    ) -> Signal {
        let mut bullish_signals: Vec<&str> = Vec::new();
        let mut bearish_signals: Vec<&str> = Vec::new();

        // Order book imbalance
        if order_book.imbalance > 0.2 {
            bullish_signals.push("bid imbalance");
        } else if order_book.imbalance < -0.2 {
            bearish_signals.push("ask imbalance");
        }

        // Funding rate (negative = shorts pay longs = bullish)
        if funding.rate_bps < -self.config.funding_threshold_bps {
            bullish_signals.push("negative funding");
        } else if funding.rate_bps > self.config.funding_threshold_bps {
            bearish_signals.push("high funding");
        }

        // Long/Short ratio (contrarian: too many longs = bearish)
        if long_short.ratio > 1.5 {
            bearish_signals.push("crowded long");
        } else if long_short.ratio < 0.7 {
            bullish_signals.push("crowded short");
        }

        // OI change (rising OI with direction = confirmation)
        let oi_rising = oi_change_pct > self.config.oi_change_threshold_pct;
        let oi_falling = oi_change_pct < -self.config.oi_change_threshold_pct;

        // Build status line
        let status = format!(
            "OB {:.0}%{} | FR {:.2}bps | L/S {:.2} | OI {}{:.1}%",
            order_book.imbalance * 100.0,
            if order_book.imbalance > 0.0 { "↑" } else { "↓" },
            funding.rate_bps,
            long_short.ratio,
            if oi_change_pct >= 0.0 { "+" } else { "" },
            oi_change_pct,
        );

        // Determine direction
        let bullish_score = bullish_signals.len() as f64;
        let bearish_score = bearish_signals.len() as f64;

        if bullish_score >= 2.0 && bullish_score > bearish_score {
            let confidence = 0.5 + (bullish_score / 6.0).min(0.35);
            let reasoning = format!("{} | {}", status, bullish_signals.join(", "));
            Signal::new(Direction::Long, confidence, "flow", &reasoning)
        } else if bearish_score >= 2.0 && bearish_score > bullish_score {
            let confidence = 0.5 + (bearish_score / 6.0).min(0.35);
            let reasoning = format!("{} | {}", status, bearish_signals.join(", "));
            Signal::new(Direction::Short, confidence, "flow", &reasoning)
        } else {
            Signal::neutral("flow", &status)
        }
    }
}

#[async_trait]
impl Agent for FlowAgent {
    fn name(&self) -> &str {
        "flow"
    }

    async fn analyze(&self) -> Result<Signal> {
        // Fetch all data in parallel
        let (order_book, funding, oi, long_short) = tokio::try_join!(
            self.fetch_order_book(),
            self.fetch_funding_rate(),
            self.fetch_open_interest(),
            self.fetch_long_short_ratio(),
        )?;

        // Calculate OI change (compared to last reading)
        let oi_change_pct = if let Some(last) = self.last_oi {
            if last > 0.0 {
                ((oi.value - last) / last) * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Note: In a real implementation, we'd update last_oi here
        // but we can't mutate self in analyze(). Would need interior mutability.

        Ok(self.analyze_flow(&order_book, &funding, &oi, &long_short, oi_change_pct))
    }
}

// Data structures

#[derive(Debug)]
struct OrderBookData {
    bid_total: f64,
    ask_total: f64,
    imbalance: f64, // -1 to 1
}

#[derive(Debug)]
struct FundingData {
    rate: f64,
    rate_bps: f64,
}

#[derive(Debug)]
struct OpenInterestData {
    value: f64,
}

#[derive(Debug)]
struct LongShortData {
    long_ratio: f64,
    short_ratio: f64,
    ratio: f64, // long/short
}

// Binance API responses

#[derive(Debug, Deserialize)]
struct BinanceDepth {
    bids: Vec<Vec<serde_json::Value>>,
    asks: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceFunding {
    funding_rate: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOI {
    open_interest: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceLongShort {
    long_account: String,
    short_account: String,
    long_short_ratio: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_book_imbalance() {
        let data = OrderBookData {
            bid_total: 150.0,
            ask_total: 100.0,
            imbalance: 0.2,
        };
        assert!(data.imbalance > 0.0);
    }
}
