//! Whale collector — fetches the BTC positions of verified top Hyperliquid
//! traders. Returns the raw counts/sizes/PnLs. The LLM decides what (if
//! anything) to do with the consensus.

use super::DataCollector;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct WhaleConfig {}

/// Verified whale addresses from public sources (HyperTracker, Arkham,
/// OnchainDataNerd, Lookonchain, CoinAnk).
const WHALE_ADDRESSES: &[&str] = &[
    "0x5b5d51203a0f9079f8aeb098a6523a13f298c060",
    "0x0ddf9bae2af4b874b96d287a5ad42eb47138a902",
    "0xecb63caa47c7c4e77f60f1ce858cf28dc2b82b00",
    "0xb317d2bc2d3d2df5fa441b5bae0ab9d8b07283ae",
    "0x2eA18c23F72a4b6172c55B411823cdc5335923F4",
    "0x31ca8395cf837de08b24da3f660e77761dfb974b",
    "0xa523f47A5A19C52ADa3369552f6f7730fFaA4d15",
    "0xd260b10acf6779a519240a5bf6f1e2c5e2e53e14",
];

pub struct WhaleAgent {
    http_client: reqwest::Client,
}

impl WhaleAgent {
    pub fn new(_config: WhaleConfig) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(WhaleConfig::default())
    }

    async fn fetch_btc_position(&self, address: &str) -> Result<Option<TraderPosition>> {
        let url = "https://api.hyperliquid.xyz/info";
        let payload = json!({ "type": "clearinghouseState", "user": address });
        let response: HyperliquidState = self
            .http_client
            .post(url)
            .json(&payload)
            .send()
            .await
            .context("fetch Hyperliquid position")?
            .json()
            .await
            .context("parse Hyperliquid response")?;

        for pos in response.asset_positions {
            if pos.position.coin == "BTC" {
                let size: f64 = pos.position.szi.parse().unwrap_or(0.0);
                if size.abs() > 0.001 {
                    return Ok(Some(TraderPosition {
                        side_is_long: size > 0.0,
                        size_btc: size.abs(),
                        unrealized_pnl: pos.position.unrealized_pnl.parse().unwrap_or(0.0),
                    }));
                }
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl DataCollector for WhaleAgent {
    fn name(&self) -> &str {
        "whale"
    }

    async fn collect(&self) -> Result<Value> {
        let mut long_size = 0.0;
        let mut short_size = 0.0;
        let mut long_count = 0u32;
        let mut short_count = 0u32;
        let mut total_pnl = 0.0;
        let mut failed = 0u32;

        let results = futures_util::future::join_all(
            WHALE_ADDRESSES.iter().map(|a| self.fetch_btc_position(a)),
        )
        .await;

        for result in results {
            match result {
                Ok(Some(pos)) => {
                    total_pnl += pos.unrealized_pnl;
                    if pos.side_is_long {
                        long_count += 1;
                        long_size += pos.size_btc;
                    } else {
                        short_count += 1;
                        short_size += pos.size_btc;
                    }
                }
                Ok(None) => {}
                Err(_) => failed += 1,
            }
        }

        let total_size = long_size + short_size;
        let weighted_long_pct = if total_size > 0.0 {
            long_size / total_size * 100.0
        } else {
            0.0
        };
        let weighted_short_pct = if total_size > 0.0 {
            short_size / total_size * 100.0
        } else {
            0.0
        };

        Ok(json!({
            "addresses_total": WHALE_ADDRESSES.len(),
            "addresses_failed": failed,
            "addresses_with_btc_position": long_count + short_count,
            "long_count": long_count,
            "long_size_btc": long_size,
            "short_count": short_count,
            "short_size_btc": short_size,
            "size_weighted_long_pct": weighted_long_pct,
            "size_weighted_short_pct": weighted_short_pct,
            "aggregate_unrealized_pnl_usd": total_pnl,
        }))
    }
}

#[derive(Debug)]
struct TraderPosition {
    side_is_long: bool,
    size_btc: f64,
    unrealized_pnl: f64,
}

#[derive(Debug, Deserialize)]
struct HyperliquidState {
    #[serde(rename = "assetPositions", default)]
    asset_positions: Vec<AssetPosition>,
}

#[derive(Debug, Deserialize)]
struct AssetPosition {
    position: PositionData,
}

#[derive(Debug, Deserialize)]
struct PositionData {
    coin: String,
    szi: String,
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: String,
}
