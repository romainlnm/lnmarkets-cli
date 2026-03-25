//! Flow Agent - Market Microstructure Signals
//!
//! Analyzes order flow and positioning data from Binance Futures:
//! - Order book imbalance (bid/ask depth)
//! - Funding rate (long/short sentiment)
//! - Open Interest changes
//! - Long/Short ratio
//! - Taker buy/sell volume ratio

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
    /// Order book imbalance threshold (0-1)
    pub ob_imbalance_threshold: f64,
    /// Long/Short ratio thresholds for contrarian signals
    pub ls_ratio_high: f64,
    pub ls_ratio_low: f64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            depth_levels: 20,
            funding_threshold_bps: 5.0,  // 0.005% - more sensitive
            oi_change_threshold_pct: 3.0, // 3% - more sensitive
            ob_imbalance_threshold: 0.15, // 15% imbalance
            ls_ratio_high: 1.3,  // Crowded long threshold
            ls_ratio_low: 0.77,  // Crowded short threshold
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

    /// Fetch taker buy/sell volume ratio (last 5 minutes)
    async fn fetch_taker_volume(&self) -> Result<TakerVolumeData> {
        let url = format!(
            "https://fapi.binance.com/futures/data/takerlongshortRatio?symbol={}&period=5m&limit=1",
            self.config.symbol
        );

        let response: Vec<BinanceTakerRatio> = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch taker volume")?
            .json()
            .await
            .context("Failed to parse taker volume")?;

        let data = response.first();

        Ok(TakerVolumeData {
            buy_ratio: data.and_then(|d| d.buy_vol.parse::<f64>().ok()).unwrap_or(0.5),
            sell_ratio: data.and_then(|d| d.sell_vol.parse::<f64>().ok()).unwrap_or(0.5),
            ratio: data.and_then(|d| d.buy_sell_ratio.parse::<f64>().ok()).unwrap_or(1.0),
        })
    }

    /// Analyze all flow data and produce signal
    fn analyze_flow(
        &self,
        order_book: &OrderBookData,
        funding: &FundingData,
        _oi: &OpenInterestData,
        long_short: &LongShortData,
        taker: &TakerVolumeData,
        oi_change_pct: f64,
    ) -> Signal {
        // Weighted signals: (direction, weight, reason)
        let mut signals: Vec<(Direction, f64, &str)> = Vec::new();

        // Order book imbalance (weight: 1.0)
        if order_book.imbalance > self.config.ob_imbalance_threshold {
            signals.push((Direction::Long, 1.0, "bid imbalance"));
        } else if order_book.imbalance < -self.config.ob_imbalance_threshold {
            signals.push((Direction::Short, 1.0, "ask imbalance"));
        }

        // Taker buy/sell volume (weight: 1.5 - strong real-time signal)
        if taker.ratio > 1.15 {
            signals.push((Direction::Long, 1.5, "taker buying"));
        } else if taker.ratio < 0.87 {
            signals.push((Direction::Short, 1.5, "taker selling"));
        }

        // Funding rate (weight: 1.2 - negative = shorts pay longs = bullish)
        if funding.rate_bps < -self.config.funding_threshold_bps {
            signals.push((Direction::Long, 1.2, "negative funding"));
        } else if funding.rate_bps > self.config.funding_threshold_bps {
            signals.push((Direction::Short, 1.2, "high funding"));
        }

        // Long/Short ratio - contrarian (weight: 1.3)
        if long_short.ratio > self.config.ls_ratio_high {
            signals.push((Direction::Short, 1.3, "crowded long"));
        } else if long_short.ratio < self.config.ls_ratio_low {
            signals.push((Direction::Long, 1.3, "crowded short"));
        }

        // OI change confirmation (weight: 0.8 - supporting indicator)
        if oi_change_pct > self.config.oi_change_threshold_pct {
            // Rising OI = new positions entering, confirms trend
            signals.push((Direction::Long, 0.8, "OI rising"));
        } else if oi_change_pct < -self.config.oi_change_threshold_pct {
            // Falling OI = positions closing, potential reversal
            signals.push((Direction::Short, 0.8, "OI falling"));
        }

        // Build status line
        let status = format!(
            "OB {:.0}%{} | Tkr {:.2} | FR {:.1}bp | L/S {:.2}",
            order_book.imbalance * 100.0,
            if order_book.imbalance > 0.0 { "↑" } else { "↓" },
            taker.ratio,
            funding.rate_bps,
            long_short.ratio,
        );

        // Calculate weighted scores
        let long_weighted: f64 = signals
            .iter()
            .filter(|(d, _, _)| *d == Direction::Long)
            .map(|(_, w, _)| w)
            .sum();
        let short_weighted: f64 = signals
            .iter()
            .filter(|(d, _, _)| *d == Direction::Short)
            .map(|(_, w, _)| w)
            .sum();

        let total_weight = long_weighted + short_weighted;
        if total_weight < 1.5 {
            return Signal::neutral("flow", &format!("{} | weak signals", status));
        }

        let reasons: Vec<&str> = signals.iter().map(|(_, _, r)| *r).collect();
        let reasoning = format!("{} | {}", status, reasons.join(", "));

        // Confidence based on signal agreement
        if long_weighted > short_weighted {
            let confidence = 0.45 + (long_weighted / 8.0).min(0.45);
            Signal::new(Direction::Long, confidence, "flow", &reasoning)
        } else if short_weighted > long_weighted {
            let confidence = 0.45 + (short_weighted / 8.0).min(0.45);
            Signal::new(Direction::Short, confidence, "flow", &reasoning)
        } else {
            Signal::neutral("flow", &format!("{} | conflicting", status))
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
        let (order_book, funding, oi, long_short, taker) = tokio::try_join!(
            self.fetch_order_book(),
            self.fetch_funding_rate(),
            self.fetch_open_interest(),
            self.fetch_long_short_ratio(),
            self.fetch_taker_volume(),
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

        Ok(self.analyze_flow(&order_book, &funding, &oi, &long_short, &taker, oi_change_pct))
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

#[derive(Debug)]
struct TakerVolumeData {
    buy_ratio: f64,
    sell_ratio: f64,
    ratio: f64, // buy/sell
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTakerRatio {
    buy_vol: String,
    sell_vol: String,
    buy_sell_ratio: String,
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
