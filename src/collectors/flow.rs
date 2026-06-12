//! Flow collector — fetches order flow + positioning data from Binance Futures
//! and returns the raw observations. No Long/Short scoring.

use super::DataCollector;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct FlowConfig {
    pub symbol: String,
    pub depth_levels: usize,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            symbol: "BTCUSDT".to_string(),
            depth_levels: 20,
        }
    }
}

pub struct FlowAgent {
    config: FlowConfig,
    http_client: reqwest::Client,
}

impl FlowAgent {
    pub fn new(config: FlowConfig) -> Self {
        Self {
            config,
            http_client: super::http_client(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(FlowConfig::default())
    }

    async fn fetch_order_book(&self) -> Result<OrderBookData> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/depth?symbol={}&limit={}",
            self.config.symbol, self.config.depth_levels
        );
        let response: BinanceDepth = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch order book")?
            .json()
            .await
            .context("parse order book")?;

        let bid_total: f64 = response
            .bids
            .iter()
            .filter_map(|l| l.get(1)?.as_str()?.parse::<f64>().ok())
            .sum();
        let ask_total: f64 = response
            .asks
            .iter()
            .filter_map(|l| l.get(1)?.as_str()?.parse::<f64>().ok())
            .sum();
        let imbalance = if bid_total + ask_total > 0.0 {
            (bid_total - ask_total) / (bid_total + ask_total)
        } else {
            0.0
        };
        Ok(OrderBookData {
            bid_total,
            ask_total,
            imbalance,
        })
    }

    async fn fetch_funding_rate(&self) -> Result<f64> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/fundingRate?symbol={}&limit=1",
            self.config.symbol
        );
        let response: Vec<BinanceFunding> = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch funding rate")?
            .json()
            .await
            .context("parse funding rate")?;
        Ok(response
            .first()
            .and_then(|f| f.funding_rate.parse::<f64>().ok())
            .unwrap_or(0.0))
    }

    async fn fetch_open_interest(&self) -> Result<f64> {
        let url = format!(
            "https://fapi.binance.com/fapi/v1/openInterest?symbol={}",
            self.config.symbol
        );
        let response: BinanceOI = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch open interest")?
            .json()
            .await
            .context("parse open interest")?;
        Ok(response.open_interest.parse::<f64>().unwrap_or(0.0))
    }

    async fn fetch_long_short_ratio(&self) -> Result<LongShortData> {
        let url = format!(
            "https://fapi.binance.com/futures/data/globalLongShortAccountRatio?symbol={}&period=5m&limit=1",
            self.config.symbol
        );
        let response: Vec<BinanceLongShort> = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch long/short ratio")?
            .json()
            .await
            .context("parse long/short ratio")?;
        let data = response.first();
        Ok(LongShortData {
            long_pct: data
                .and_then(|d| d.long_account.parse::<f64>().ok())
                .unwrap_or(0.5),
            short_pct: data
                .and_then(|d| d.short_account.parse::<f64>().ok())
                .unwrap_or(0.5),
            ratio: data
                .and_then(|d| d.long_short_ratio.parse::<f64>().ok())
                .unwrap_or(1.0),
        })
    }

    async fn fetch_taker_volume(&self) -> Result<TakerVolumeData> {
        let url = format!(
            "https://fapi.binance.com/futures/data/takerlongshortRatio?symbol={}&period=5m&limit=1",
            self.config.symbol
        );
        let response: Vec<BinanceTakerRatio> = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch taker volume")?
            .json()
            .await
            .context("parse taker volume")?;
        let data = response.first();
        Ok(TakerVolumeData {
            buy_pct: data
                .and_then(|d| d.buy_vol.parse::<f64>().ok())
                .unwrap_or(0.5),
            sell_pct: data
                .and_then(|d| d.sell_vol.parse::<f64>().ok())
                .unwrap_or(0.5),
            ratio: data
                .and_then(|d| d.buy_sell_ratio.parse::<f64>().ok())
                .unwrap_or(1.0),
        })
    }
}

#[async_trait]
impl DataCollector for FlowAgent {
    fn name(&self) -> &str {
        "flow"
    }

    async fn collect(&self) -> Result<Value> {
        let (book, funding, oi, ls, taker) = tokio::try_join!(
            self.fetch_order_book(),
            self.fetch_funding_rate(),
            self.fetch_open_interest(),
            self.fetch_long_short_ratio(),
            self.fetch_taker_volume(),
        )?;

        Ok(json!({
            "order_book": {
                "bid_total_btc": book.bid_total,
                "ask_total_btc": book.ask_total,
                "imbalance_pct": book.imbalance * 100.0,
            },
            "funding_rate_pct": funding * 100.0,
            "open_interest_btc": oi,
            "long_short_ratio": {
                "long_account_pct": ls.long_pct * 100.0,
                "short_account_pct": ls.short_pct * 100.0,
                "ratio": ls.ratio,
            },
            "taker_volume_5m": {
                "buy_pct": taker.buy_pct * 100.0,
                "sell_pct": taker.sell_pct * 100.0,
                "buy_sell_ratio": taker.ratio,
            },
        }))
    }
}

#[derive(Debug)]
struct OrderBookData {
    bid_total: f64,
    ask_total: f64,
    imbalance: f64,
}

#[derive(Debug)]
struct LongShortData {
    long_pct: f64,
    short_pct: f64,
    ratio: f64,
}

#[derive(Debug)]
struct TakerVolumeData {
    buy_pct: f64,
    sell_pct: f64,
    ratio: f64,
}

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
