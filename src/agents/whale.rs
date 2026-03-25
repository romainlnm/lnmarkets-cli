//! Whale Agent - Copy Top Hyperliquid BTC Traders
//!
//! Tracks BTC positions of top performers on Hyperliquid:
//! 1. Fetches top 10 traders by 30D PnL from HyperTracker at startup
//! 2. Queries each trader's BTC position via Hyperliquid API
//! 3. Weights signal by their 30D trading volume/PnL

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
    /// Minimum consensus for signal (0.0-1.0)
    pub min_consensus: f64,
}

impl Default for WhaleConfig {
    fn default() -> Self {
        Self {
            top_n: 10,
            hypertracker_api_key: std::env::var("HYPERTRACKER_API_KEY").ok(),
            min_consensus: 0.7, // 70% of weighted volume on same side
        }
    }
}

/// Trader info from leaderboard
#[derive(Debug, Clone)]
struct TopTrader {
    address: String,
    pnl_30d: f64,  // 30D PnL as proxy for activity/volume
}

/// Whale Agent implementation
pub struct WhaleAgent {
    config: WhaleConfig,
    http_client: reqwest::Client,
    cached_traders: RwLock<Vec<TopTrader>>,
    last_fetch: RwLock<Option<Instant>>,
    initialized: RwLock<bool>,
}

impl WhaleAgent {
    pub fn new(config: WhaleConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            cached_traders: RwLock::new(Vec::new()),
            last_fetch: RwLock::new(None),
            initialized: RwLock::new(false),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(WhaleConfig::default())
    }

    /// Fetch top 10 traders by 30D PnL from HyperTracker (once at startup)
    async fn fetch_leaderboard(&self) -> Result<Vec<TopTrader>> {
        // Return cached if already fetched
        {
            let initialized = self.initialized.read().unwrap();
            if *initialized {
                let cached = self.cached_traders.read().unwrap();
                if !cached.is_empty() {
                    return Ok(cached.clone());
                }
            }
        }

        println!("  [whale] Fetching top {} traders from HyperTracker...", self.config.top_n);

        let url = format!(
            "https://ht-api.coinmarketman.com/api/external/leaderboards/perp-pnl?limit={}&rankBy=pnlMonth&order=desc",
            self.config.top_n
        );

        let mut request = self.http_client.get(&url);

        if let Some(ref api_key) = self.config.hypertracker_api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .send()
            .await
            .context("Failed to fetch HyperTracker leaderboard")?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow::anyhow!("HyperTracker API returned {}", status));
        }

        let data: HyperTrackerResponse = response
            .json()
            .await
            .context("Failed to parse HyperTracker response")?;

        // Filter traders with 30D PnL > 0 (active traders)
        let traders: Vec<TopTrader> = data.data
            .into_iter()
            .filter(|t| t.pnl_month.unwrap_or(0.0) > 0.0)
            .map(|t| TopTrader {
                address: t.address,
                pnl_30d: t.pnl_month.unwrap_or(0.0),
            })
            .collect();

        if traders.is_empty() {
            return Err(anyhow::anyhow!("No active traders found on leaderboard"));
        }

        // Print fetched traders
        println!("  [whale] Found {} active traders:", traders.len());
        for (i, t) in traders.iter().take(5).enumerate() {
            println!("    {}. {} (30D: +${:.0}K)", i + 1, &t.address[..10], t.pnl_30d / 1000.0);
        }
        if traders.len() > 5 {
            println!("    ... and {} more", traders.len() - 5);
        }

        // Cache traders
        {
            let mut cached = self.cached_traders.write().unwrap();
            *cached = traders.clone();
            let mut init = self.initialized.write().unwrap();
            *init = true;
            let mut last = self.last_fetch.write().unwrap();
            *last = Some(Instant::now());
        }

        Ok(traders)
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
    #[serde(rename = "pnlMonth")]
    pnl_month: Option<f64>,
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
    #[serde(rename = "unrealizedPnl")]
    unrealized_pnl: String,
}

#[async_trait]
impl Agent for WhaleAgent {
    fn name(&self) -> &str {
        "whale"
    }

    async fn analyze(&self) -> Result<Signal> {
        // 1. Get top traders (fetched once at startup, then cached)
        let traders = match self.fetch_leaderboard().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(Signal::neutral("whale", &format!("API error: {}", e)));
            }
        };

        if traders.is_empty() {
            return Ok(Signal::neutral("whale", "No traders to track"));
        }

        // 2. Fetch each trader's BTC position and weight by their 30D PnL
        let mut long_weight = 0.0;
        let mut short_weight = 0.0;
        let mut long_count = 0;
        let mut short_count = 0;
        let mut long_size = 0.0;
        let mut short_size = 0.0;
        let mut total_pnl = 0.0;

        for trader in &traders {
            match self.fetch_btc_position(&trader.address).await {
                Ok(Some(pos)) => {
                    total_pnl += pos.unrealized_pnl;
                    // Weight by trader's 30D PnL (proxy for volume/activity)
                    let weight = trader.pnl_30d.abs();
                    match pos.side {
                        Direction::Long => {
                            long_count += 1;
                            long_size += pos.size_btc;
                            long_weight += weight;
                        }
                        Direction::Short => {
                            short_count += 1;
                            short_size += pos.size_btc;
                            short_weight += weight;
                        }
                        Direction::Neutral => {}
                    }
                }
                Ok(None) => {
                    // No BTC position - neutral
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

        // Require at least 3 traders with positions
        if total_with_position < 3 {
            return Ok(Signal::neutral("whale", &format!(
                "Only {}/{} whales have BTC positions (need 3+)",
                total_with_position, traders.len()
            )));
        }

        // 3. Calculate VOLUME-WEIGHTED consensus (by 30D PnL)
        let total_weight = long_weight + short_weight;
        let long_pct = if total_weight > 0.0 { long_weight / total_weight } else { 0.5 };
        let short_pct = if total_weight > 0.0 { short_weight / total_weight } else { 0.5 };

        let (direction, consensus) = if long_pct > short_pct {
            (Direction::Long, long_pct)
        } else if short_pct > long_pct {
            (Direction::Short, short_pct)
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
        let confidence = 0.5 + (consensus - 0.5) * 0.8;

        let pnl_str = if total_pnl >= 0.0 {
            format!("+${:.0}K", total_pnl / 1000.0)
        } else {
            format!("-${:.0}K", total_pnl.abs() / 1000.0)
        };

        let reasoning = format!(
            "{} long ({:.1} BTC) vs {} short ({:.1} BTC) | vol weight: {:.0}%/{:.0}% | PnL: {}",
            long_count, long_size,
            short_count, short_size,
            long_pct * 100.0, short_pct * 100.0,
            pnl_str
        );

        Ok(Signal::new(final_direction, confidence, "whale", &reasoning))
    }
}
