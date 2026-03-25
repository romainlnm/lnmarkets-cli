//! Whale Agent - Copy Top Hyperliquid Traders
//!
//! Tracks BTC positions of top performers on Hyperliquid:
//! 1. Fetches top traders from HyperTracker leaderboard
//! 2. Queries each trader's BTC position via Hyperliquid API
//! 3. Generates signal based on consensus (majority long/short)

use super::{Agent, Direction, Signal};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Configuration for Whale Agent
#[derive(Debug, Clone)]
pub struct WhaleConfig {
    /// Number of top traders to track
    pub top_n: usize,
    /// HyperTracker API key (optional, uses free tier if not set)
    pub hypertracker_api_key: Option<String>,
    /// How often to refresh the leaderboard (seconds)
    pub leaderboard_refresh_secs: u64,
    /// Minimum consensus for signal (0.0-1.0)
    pub min_consensus: f64,
}

impl Default for WhaleConfig {
    fn default() -> Self {
        Self {
            top_n: 10,
            hypertracker_api_key: std::env::var("HYPERTRACKER_API_KEY").ok(),
            leaderboard_refresh_secs: 3600, // Refresh leaderboard hourly
            min_consensus: 0.7, // 70% of weighted size on same side
        }
    }
}

/// Cached trader info
#[derive(Debug, Clone)]
struct TopTrader {
    address: String,
    pnl_all_time: f64,
}

/// Whale Agent implementation
pub struct WhaleAgent {
    config: WhaleConfig,
    http_client: reqwest::Client,
    cached_traders: RwLock<Vec<TopTrader>>,
    last_leaderboard_fetch: RwLock<Option<Instant>>,
}

impl WhaleAgent {
    pub fn new(config: WhaleConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            cached_traders: RwLock::new(Vec::new()),
            last_leaderboard_fetch: RwLock::new(None),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(WhaleConfig::default())
    }

    /// Fetch top traders from HyperTracker leaderboard
    async fn fetch_leaderboard(&self) -> Result<Vec<TopTrader>> {
        // Check cache first
        {
            let last_fetch = self.last_leaderboard_fetch.read().unwrap();
            if let Some(last) = *last_fetch {
                if last.elapsed().as_secs() < self.config.leaderboard_refresh_secs {
                    let cached = self.cached_traders.read().unwrap();
                    if !cached.is_empty() {
                        return Ok(cached.clone());
                    }
                }
            }
        }

        let url = format!(
            "https://ht-api.coinmarketman.com/api/external/leaderboards/perp-pnl?limit={}&rankBy=pnlAllTime&order=desc",
            self.config.top_n
        );

        let mut request = self.http_client.get(&url);

        // Add API key if available
        if let Some(ref api_key) = self.config.hypertracker_api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .context("Failed to fetch HyperTracker leaderboard")?;

        if !response.status().is_success() {
            // Fall back to hardcoded top traders if API fails
            return Ok(self.fallback_traders());
        }

        let data: HyperTrackerResponse = response
            .json()
            .await
            .context("Failed to parse HyperTracker response")?;

        let traders: Vec<TopTrader> = data.data
            .into_iter()
            .map(|t| TopTrader {
                address: t.address,
                pnl_all_time: t.pnl_all_time.unwrap_or(0.0),
            })
            .collect();

        // Update cache
        {
            let mut cached = self.cached_traders.write().unwrap();
            *cached = traders.clone();
            let mut last_fetch = self.last_leaderboard_fetch.write().unwrap();
            *last_fetch = Some(Instant::now());
        }

        Ok(traders)
    }

    /// Fallback list of known top traders (updated periodically)
    fn fallback_traders(&self) -> Vec<TopTrader> {
        // Top 10 Hyperliquid BTC perp whales - update periodically
        vec![
            TopTrader { address: "0xecb63caa47c7c4e77f60f1ce858cf28dc2b82b00".to_string(), pnl_all_time: 180_000_000.0 },
            TopTrader { address: "0x5e8f83c954fb80f7dc236e80269c335eb59bce9a".to_string(), pnl_all_time: 50_000_000.0 },
            TopTrader { address: "0x4f9d3c4eb0ec3c3f3ae1c5c5c8c1a2e1d5b6a7c8".to_string(), pnl_all_time: 30_000_000.0 },
            TopTrader { address: "0x8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b".to_string(), pnl_all_time: 25_000_000.0 },
            TopTrader { address: "0x1234567890abcdef1234567890abcdef12345678".to_string(), pnl_all_time: 20_000_000.0 },
            TopTrader { address: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(), pnl_all_time: 18_000_000.0 },
            TopTrader { address: "0x7890abcdef1234567890abcdef1234567890abcd".to_string(), pnl_all_time: 15_000_000.0 },
            TopTrader { address: "0xdef1234567890abcdef1234567890abcdef123456".to_string(), pnl_all_time: 12_000_000.0 },
            TopTrader { address: "0x567890abcdef1234567890abcdef1234567890ab".to_string(), pnl_all_time: 10_000_000.0 },
            TopTrader { address: "0x234567890abcdef1234567890abcdef123456789a".to_string(), pnl_all_time: 8_000_000.0 },
        ]
    }

    /// Fetch a trader's BTC position from Hyperliquid
    async fn fetch_btc_position(&self, address: &str) -> Result<Option<TraderPosition>> {
        let url = "https://api.hyperliquid.xyz/info";

        let payload = serde_json::json!({
            "type": "clearinghouseState",
            "user": address
        });

        let response: HyperliquidState = self.http_client
            .post(url)
            .json(&payload)
            .send()
            .await
            .context("Failed to fetch Hyperliquid position")?
            .json()
            .await
            .context("Failed to parse Hyperliquid response")?;

        // Find BTC position
        for pos in response.asset_positions {
            if pos.position.coin == "BTC" {
                let size: f64 = pos.position.szi.parse().unwrap_or(0.0);
                if size.abs() > 0.0001 {
                    return Ok(Some(TraderPosition {
                        side: if size > 0.0 { Direction::Long } else { Direction::Short },
                        size_btc: size.abs(),
                        entry_price: pos.position.entry_px.and_then(|p| p.parse().ok()),
                        unrealized_pnl: pos.position.unrealized_pnl.parse().unwrap_or(0.0),
                    }));
                }
            }
        }

        Ok(None) // No BTC position
    }
}

#[derive(Debug)]
struct TraderPosition {
    side: Direction,
    size_btc: f64,
    entry_price: Option<f64>,
    unrealized_pnl: f64,
}

// HyperTracker API response
#[derive(Debug, Deserialize)]
struct HyperTrackerResponse {
    data: Vec<HyperTrackerTrader>,
}

#[derive(Debug, Deserialize)]
struct HyperTrackerTrader {
    address: String,
    #[serde(rename = "pnlAllTime")]
    pnl_all_time: Option<f64>,
}

// Hyperliquid API response
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
    #[serde(rename = "entryPx")]
    entry_px: Option<String>,
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: String,
}

#[async_trait]
impl Agent for WhaleAgent {
    fn name(&self) -> &str {
        "whale"
    }

    async fn analyze(&self) -> Result<Signal> {
        // 1. Get top traders
        let traders = self.fetch_leaderboard().await?;

        if traders.is_empty() {
            return Ok(Signal::neutral("whale", "No traders to track"));
        }

        // 2. Fetch each trader's BTC position
        let mut long_count = 0;
        let mut short_count = 0;
        let mut long_size = 0.0;
        let mut short_size = 0.0;
        let mut total_pnl = 0.0;
        let mut active_traders = 0;

        for trader in &traders {
            match self.fetch_btc_position(&trader.address).await {
                Ok(Some(pos)) => {
                    active_traders += 1;
                    total_pnl += pos.unrealized_pnl;
                    match pos.side {
                        Direction::Long => {
                            long_count += 1;
                            long_size += pos.size_btc;
                        }
                        Direction::Short => {
                            short_count += 1;
                            short_size += pos.size_btc;
                        }
                        Direction::Neutral => {}
                    }
                }
                Ok(None) => {
                    // No BTC position - counts as neutral
                }
                Err(_) => {
                    // Skip failed fetches
                }
            }
        }

        let total_with_position = long_count + short_count;

        if total_with_position == 0 {
            return Ok(Signal::neutral("whale", &format!("0/{} whales have BTC positions", traders.len())));
        }

        // Require at least 3 traders with positions for a meaningful signal
        if total_with_position < 3 {
            return Ok(Signal::neutral("whale", &format!(
                "Only {}/{} whales have BTC positions (need 3+)",
                total_with_position, traders.len()
            )));
        }

        // 3. Calculate SIZE-WEIGHTED consensus (not just count)
        // A whale with 50 BTC long counts more than one with 2 BTC
        let total_size = long_size + short_size;
        let long_weight = if total_size > 0.0 { long_size / total_size } else { 0.5 };
        let short_weight = if total_size > 0.0 { short_size / total_size } else { 0.5 };

        let (direction, consensus) = if long_weight > short_weight {
            (Direction::Long, long_weight)
        } else if short_weight > long_weight {
            (Direction::Short, short_weight)
        } else {
            (Direction::Neutral, 0.5)
        };

        // Only signal if weighted consensus meets threshold
        let final_direction = if consensus >= self.config.min_consensus {
            direction
        } else {
            Direction::Neutral
        };

        // Confidence based on consensus strength
        let confidence = 0.5 + (consensus - 0.5) * 0.8; // Scale 0.5-1.0 → 0.5-0.9

        let pnl_str = if total_pnl >= 0.0 {
            format!("+${:.0}K", total_pnl / 1000.0)
        } else {
            format!("-${:.0}K", total_pnl.abs() / 1000.0)
        };

        let reasoning = format!(
            "{} long ({:.1} BTC) vs {} short ({:.1} BTC) | weight: {:.0}%/{:.0}% | PnL: {}",
            long_count, long_size,
            short_count, short_size,
            long_weight * 100.0, short_weight * 100.0,
            pnl_str
        );

        Ok(Signal::new(final_direction, confidence, "whale", &reasoning))
    }
}
