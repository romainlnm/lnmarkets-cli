//! Pattern Agent - Technical Analysis Signals
//!
//! Analyzes price data using technical indicators:
//! - RSI (Relative Strength Index)
//! - EMA crossover (9/21)
//! - Bollinger Bands

use super::{Agent, Direction, Signal};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::VecDeque;
use tokio::sync::RwLock;

/// Configuration for the Pattern Agent
#[derive(Debug, Clone)]
pub struct PatternConfig {
    /// RSI period (default: 14)
    pub rsi_period: usize,
    /// RSI overbought threshold (default: 70)
    pub rsi_overbought: f64,
    /// RSI oversold threshold (default: 30)
    pub rsi_oversold: f64,
    /// Fast EMA period (default: 9)
    pub ema_fast: usize,
    /// Slow EMA period (default: 21)
    pub ema_slow: usize,
    /// Bollinger Bands period (default: 20)
    pub bb_period: usize,
    /// Bollinger Bands std dev multiplier (default: 2.0)
    pub bb_std_dev: f64,
    /// Price fetch interval in seconds
    pub interval_secs: u64,
    /// Symbol to track
    pub symbol: String,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            rsi_period: 14,
            rsi_overbought: 70.0,
            rsi_oversold: 30.0,
            ema_fast: 9,
            ema_slow: 21,
            bb_period: 20,
            bb_std_dev: 2.0,
            interval_secs: 60,
            symbol: "BTCUSDT".to_string(),
        }
    }
}

/// Binance kline response
#[derive(Debug, Deserialize)]
struct BinanceKline {
    // [open_time, open, high, low, close, volume, ...]
    #[serde(rename = "0")]
    _open_time: u64,
    #[serde(rename = "1")]
    _open: String,
    #[serde(rename = "2")]
    _high: String,
    #[serde(rename = "3")]
    _low: String,
    #[serde(rename = "4")]
    close: String,
    #[serde(rename = "5")]
    _volume: String,
}

/// Price data point
#[derive(Debug, Clone)]
struct PricePoint {
    close: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Pattern Agent implementation
pub struct PatternAgent {
    config: PatternConfig,
    prices: RwLock<VecDeque<PricePoint>>,
    http_client: reqwest::Client,
}

impl PatternAgent {
    pub fn new(config: PatternConfig) -> Self {
        Self {
            config,
            prices: RwLock::new(VecDeque::with_capacity(100)),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PatternConfig::default())
    }

    /// Fetch recent klines from Binance
    async fn fetch_prices(&self) -> Result<Vec<f64>> {
        let url = format!(
            "https://api.binance.com/api/v3/klines?symbol={}&interval=1m&limit=50",
            self.config.symbol
        );

        let response: Vec<Vec<serde_json::Value>> = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Binance klines")?
            .json()
            .await
            .context("Failed to parse Binance response")?;

        let prices: Vec<f64> = response
            .iter()
            .filter_map(|kline| {
                kline.get(4)?.as_str()?.parse::<f64>().ok()
            })
            .collect();

        Ok(prices)
    }

    /// Calculate RSI
    fn calculate_rsi(prices: &[f64], period: usize) -> Option<f64> {
        if prices.len() < period + 1 {
            return None;
        }

        let mut gains = 0.0;
        let mut losses = 0.0;

        for i in 1..=period {
            let change = prices[prices.len() - i] - prices[prices.len() - i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses -= change;
            }
        }

        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;

        if avg_loss == 0.0 {
            return Some(100.0);
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    /// Calculate EMA
    fn calculate_ema(prices: &[f64], period: usize) -> Option<f64> {
        if prices.len() < period {
            return None;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = prices[0..period].iter().sum::<f64>() / period as f64;

        for price in prices.iter().skip(period) {
            ema = (price - ema) * multiplier + ema;
        }

        Some(ema)
    }

    /// Calculate Bollinger Bands
    fn calculate_bollinger(prices: &[f64], period: usize, std_dev_mult: f64) -> Option<(f64, f64, f64)> {
        if prices.len() < period {
            return None;
        }

        let recent: &[f64] = &prices[prices.len() - period..];
        let sma: f64 = recent.iter().sum::<f64>() / period as f64;

        let variance: f64 = recent.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();

        let upper = sma + std_dev_mult * std_dev;
        let lower = sma - std_dev_mult * std_dev;

        Some((lower, sma, upper))
    }

    /// Analyze indicators and produce signal
    fn analyze_indicators(&self, prices: &[f64]) -> Signal {
        let current_price = *prices.last().unwrap_or(&0.0);

        // Calculate indicators
        let rsi = Self::calculate_rsi(prices, self.config.rsi_period);
        let ema_fast = Self::calculate_ema(prices, self.config.ema_fast);
        let ema_slow = Self::calculate_ema(prices, self.config.ema_slow);
        let bollinger = Self::calculate_bollinger(prices, self.config.bb_period, self.config.bb_std_dev);

        let mut signals: Vec<(Direction, f64, &str)> = Vec::new();

        // RSI signal
        if let Some(rsi_val) = rsi {
            if rsi_val >= self.config.rsi_overbought {
                let strength = (rsi_val - self.config.rsi_overbought) / (100.0 - self.config.rsi_overbought);
                signals.push((Direction::Short, 0.5 + strength * 0.3, "RSI overbought"));
            } else if rsi_val <= self.config.rsi_oversold {
                let strength = (self.config.rsi_oversold - rsi_val) / self.config.rsi_oversold;
                signals.push((Direction::Long, 0.5 + strength * 0.3, "RSI oversold"));
            }
        }

        // EMA crossover signal
        if let (Some(fast), Some(slow)) = (ema_fast, ema_slow) {
            let diff_pct = (fast - slow) / slow * 100.0;
            if diff_pct > 0.1 {
                signals.push((Direction::Long, 0.5 + (diff_pct / 2.0).min(0.3), "EMA bullish crossover"));
            } else if diff_pct < -0.1 {
                signals.push((Direction::Short, 0.5 + (diff_pct.abs() / 2.0).min(0.3), "EMA bearish crossover"));
            }
        }

        // Bollinger Bands signal
        if let Some((lower, _mid, upper)) = bollinger {
            if current_price <= lower {
                let penetration = (lower - current_price) / lower * 100.0;
                signals.push((Direction::Long, 0.6 + (penetration / 5.0).min(0.2), "Price at lower BB"));
            } else if current_price >= upper {
                let penetration = (current_price - upper) / upper * 100.0;
                signals.push((Direction::Short, 0.6 + (penetration / 5.0).min(0.2), "Price at upper BB"));
            }
        }

        // Build status line even for neutral
        let status = format!(
            "BTC ${:.0} | RSI {:.1} | EMA9 {:.0} EMA21 {:.0}",
            current_price,
            rsi.unwrap_or(50.0),
            ema_fast.unwrap_or(0.0),
            ema_slow.unwrap_or(0.0),
        );

        // Combine signals
        if signals.is_empty() {
            return Signal::neutral("pattern", &format!("{} | No strong signals", status));
        }

        let long_score: f64 = signals
            .iter()
            .filter(|(d, _, _)| *d == Direction::Long)
            .map(|(_, c, _)| c)
            .sum();
        let short_score: f64 = signals
            .iter()
            .filter(|(d, _, _)| *d == Direction::Short)
            .map(|(_, c, _)| c)
            .sum();

        let reasons: Vec<&str> = signals.iter().map(|(_, _, r)| *r).collect();
        let reasoning = format!(
            "BTC ${:.0} | RSI: {:.1} | EMA9: {:.0} EMA21: {:.0} | {}",
            current_price,
            rsi.unwrap_or(50.0),
            ema_fast.unwrap_or(0.0),
            ema_slow.unwrap_or(0.0),
            reasons.join(", ")
        );

        if long_score > short_score && long_score > 0.5 {
            Signal::new(Direction::Long, (long_score / signals.len() as f64).min(0.9), "pattern", &reasoning)
        } else if short_score > long_score && short_score > 0.5 {
            Signal::new(Direction::Short, (short_score / signals.len() as f64).min(0.9), "pattern", &reasoning)
        } else {
            Signal::neutral("pattern", &reasoning)
        }
    }
}

#[async_trait]
impl Agent for PatternAgent {
    fn name(&self) -> &str {
        "pattern"
    }

    async fn analyze(&self) -> Result<Signal> {
        let prices = self.fetch_prices().await?;

        if prices.len() < self.config.ema_slow {
            return Ok(Signal::neutral(
                "pattern",
                &format!("Insufficient data: {} prices, need {}", prices.len(), self.config.ema_slow),
            ));
        }

        Ok(self.analyze_indicators(&prices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rsi_calculation() {
        // Prices that should give oversold RSI
        let prices: Vec<f64> = (0..20).map(|i| 100.0 - i as f64 * 0.5).collect();
        let rsi = PatternAgent::calculate_rsi(&prices, 14);
        assert!(rsi.is_some());
        assert!(rsi.unwrap() < 50.0);
    }

    #[test]
    fn test_ema_calculation() {
        let prices = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0];
        let ema = PatternAgent::calculate_ema(&prices, 5);
        assert!(ema.is_some());
        assert!(ema.unwrap() > 15.0);
    }

    #[test]
    fn test_bollinger_calculation() {
        let prices: Vec<f64> = (0..25).map(|i| 100.0 + (i as f64 * 0.1).sin() * 5.0).collect();
        let bb = PatternAgent::calculate_bollinger(&prices, 20, 2.0);
        assert!(bb.is_some());
        let (lower, mid, upper) = bb.unwrap();
        assert!(lower < mid);
        assert!(mid < upper);
    }
}
