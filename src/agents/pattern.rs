//! Pattern Agent - Technical Analysis Signals
//!
//! Analyzes price data using technical indicators:
//! - RSI (Relative Strength Index)
//! - EMA crossover (9/21)
//! - MACD (12/26/9)
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
    /// MACD fast period (default: 12)
    pub macd_fast: usize,
    /// MACD slow period (default: 26)
    pub macd_slow: usize,
    /// MACD signal period (default: 9)
    pub macd_signal: usize,
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
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
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

/// OHLC data point for ATR calculation
#[derive(Debug, Clone)]
struct OhlcPoint {
    high: f64,
    low: f64,
    close: f64,
}

/// Pattern Agent implementation
pub struct PatternAgent {
    config: PatternConfig,
    #[allow(dead_code)]
    ohlc_data: RwLock<VecDeque<OhlcPoint>>,
    http_client: reqwest::Client,
}

impl PatternAgent {
    pub fn new(config: PatternConfig) -> Self {
        Self {
            config,
            ohlc_data: RwLock::new(VecDeque::with_capacity(100)),
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PatternConfig::default())
    }

    /// Fetch recent klines from Binance (returns OHLC data)
    async fn fetch_ohlc(&self) -> Result<Vec<OhlcPoint>> {
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

        let ohlc: Vec<OhlcPoint> = response
            .iter()
            .filter_map(|kline| {
                let high = kline.get(2)?.as_str()?.parse::<f64>().ok()?;
                let low = kline.get(3)?.as_str()?.parse::<f64>().ok()?;
                let close = kline.get(4)?.as_str()?.parse::<f64>().ok()?;
                Some(OhlcPoint { high, low, close })
            })
            .collect();

        Ok(ohlc)
    }

    /// Extract close prices from OHLC data
    fn closes(ohlc: &[OhlcPoint]) -> Vec<f64> {
        ohlc.iter().map(|p| p.close).collect()
    }

    /// Calculate ATR (Average True Range)
    fn calculate_atr(ohlc: &[OhlcPoint], period: usize) -> Option<f64> {
        if ohlc.len() < period + 1 {
            return None;
        }

        let mut tr_sum = 0.0;
        let start = ohlc.len() - period;

        for i in start..ohlc.len() {
            let high = ohlc[i].high;
            let low = ohlc[i].low;
            let prev_close = ohlc[i - 1].close;

            // True Range = max(high-low, |high-prev_close|, |low-prev_close|)
            let tr = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
            tr_sum += tr;
        }

        Some(tr_sum / period as f64)
    }

    /// Calculate ATR as percentage of current price
    fn calculate_atr_pct(ohlc: &[OhlcPoint], period: usize) -> Option<f64> {
        let atr = Self::calculate_atr(ohlc, period)?;
        let current_price = ohlc.last()?.close;
        if current_price > 0.0 {
            Some((atr / current_price) * 100.0)
        } else {
            None
        }
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

    /// Calculate MACD (returns macd_line, signal_line, histogram)
    fn calculate_macd(prices: &[f64], fast: usize, slow: usize, signal: usize) -> Option<(f64, f64, f64)> {
        if prices.len() < slow + signal {
            return None;
        }

        let ema_fast = Self::calculate_ema(prices, fast)?;
        let ema_slow = Self::calculate_ema(prices, slow)?;
        let macd_line = ema_fast - ema_slow;

        // Calculate signal line (EMA of MACD values)
        // We need to compute MACD for several periods to get signal line
        let mut macd_values = Vec::with_capacity(signal + 5);
        for i in (0..signal + 5).rev() {
            if prices.len() > slow + i {
                let slice = &prices[..prices.len() - i];
                if let (Some(f), Some(s)) = (
                    Self::calculate_ema(slice, fast),
                    Self::calculate_ema(slice, slow),
                ) {
                    macd_values.push(f - s);
                }
            }
        }

        if macd_values.len() < signal {
            return Some((macd_line, macd_line, 0.0)); // Not enough data for signal
        }

        // Simple EMA of MACD values for signal line
        let multiplier = 2.0 / (signal as f64 + 1.0);
        let mut signal_line = macd_values[0..signal].iter().sum::<f64>() / signal as f64;
        for val in macd_values.iter().skip(signal) {
            signal_line = (val - signal_line) * multiplier + signal_line;
        }

        let histogram = macd_line - signal_line;
        Some((macd_line, signal_line, histogram))
    }

    /// Analyze indicators and produce signal
    fn analyze_indicators(&self, ohlc: &[OhlcPoint]) -> Signal {
        let prices = Self::closes(ohlc);
        let current_price = *prices.last().unwrap_or(&0.0);
        let atr_pct = Self::calculate_atr_pct(ohlc, 14).unwrap_or(0.0);

        // Calculate indicators
        let rsi = Self::calculate_rsi(&prices, self.config.rsi_period);
        let ema_fast = Self::calculate_ema(&prices, self.config.ema_fast);
        let ema_slow = Self::calculate_ema(&prices, self.config.ema_slow);
        let bollinger = Self::calculate_bollinger(&prices, self.config.bb_period, self.config.bb_std_dev);
        let macd = Self::calculate_macd(&prices, self.config.macd_fast, self.config.macd_slow, self.config.macd_signal);

        // Weighted signals: (direction, weight, confidence, reason)
        // Weight represents importance: RSI=1.5, MACD=1.3, EMA=1.0, BB=0.8
        let mut signals: Vec<(Direction, f64, f64, &str)> = Vec::new();

        // RSI signal (weight: 1.5 - strong mean reversion indicator)
        if let Some(rsi_val) = rsi {
            if rsi_val >= self.config.rsi_overbought {
                let strength = (rsi_val - self.config.rsi_overbought) / (100.0 - self.config.rsi_overbought);
                signals.push((Direction::Short, 1.5, 0.5 + strength * 0.35, "RSI overbought"));
            } else if rsi_val <= self.config.rsi_oversold {
                let strength = (self.config.rsi_oversold - rsi_val) / self.config.rsi_oversold;
                signals.push((Direction::Long, 1.5, 0.5 + strength * 0.35, "RSI oversold"));
            }
        }

        // MACD signal (weight: 1.3 - good trend/momentum indicator)
        if let Some((macd_line, signal_line, histogram)) = macd {
            // MACD crossover
            if macd_line > signal_line && histogram > 0.0 {
                let strength = (histogram.abs() / current_price * 10000.0).min(1.0); // Normalize
                signals.push((Direction::Long, 1.3, 0.5 + strength * 0.3, "MACD bullish"));
            } else if macd_line < signal_line && histogram < 0.0 {
                let strength = (histogram.abs() / current_price * 10000.0).min(1.0);
                signals.push((Direction::Short, 1.3, 0.5 + strength * 0.3, "MACD bearish"));
            }
        }

        // EMA crossover signal (weight: 1.0 - trend confirmation)
        if let (Some(fast), Some(slow)) = (ema_fast, ema_slow) {
            let diff_pct = (fast - slow) / slow * 100.0;
            if diff_pct > 0.05 {
                signals.push((Direction::Long, 1.0, 0.5 + (diff_pct / 2.0).min(0.3), "EMA bullish"));
            } else if diff_pct < -0.05 {
                signals.push((Direction::Short, 1.0, 0.5 + (diff_pct.abs() / 2.0).min(0.3), "EMA bearish"));
            }
        }

        // Bollinger Bands signal (weight: 0.8 - volatility/reversal)
        if let Some((lower, _mid, upper)) = bollinger {
            if current_price <= lower {
                let penetration = (lower - current_price) / lower * 100.0;
                signals.push((Direction::Long, 0.8, 0.55 + (penetration / 5.0).min(0.25), "BB lower touch"));
            } else if current_price >= upper {
                let penetration = (current_price - upper) / upper * 100.0;
                signals.push((Direction::Short, 0.8, 0.55 + (penetration / 5.0).min(0.25), "BB upper touch"));
            }
        }

        // Build status line
        let macd_str = macd.map(|(_, _, h)| format!("{:.0}", h)).unwrap_or("-".into());
        let status = format!(
            "BTC ${:.0} | RSI {:.1} | MACD {} | EMA {:.0}/{:.0} | ATR {:.2}%",
            current_price,
            rsi.unwrap_or(50.0),
            macd_str,
            ema_fast.unwrap_or(0.0),
            ema_slow.unwrap_or(0.0),
            atr_pct,
        );

        // Combine signals with weighted voting
        if signals.is_empty() {
            let mut signal = Signal::neutral("pattern", &format!("{} | No signals", status));
            signal.atr_pct = Some(atr_pct);
            return signal;
        }

        let long_weighted: f64 = signals
            .iter()
            .filter(|(d, _, _, _)| *d == Direction::Long)
            .map(|(_, w, c, _)| w * c)
            .sum();
        let short_weighted: f64 = signals
            .iter()
            .filter(|(d, _, _, _)| *d == Direction::Short)
            .map(|(_, w, c, _)| w * c)
            .sum();

        let long_weight_total: f64 = signals
            .iter()
            .filter(|(d, _, _, _)| *d == Direction::Long)
            .map(|(_, w, _, _)| w)
            .sum();
        let short_weight_total: f64 = signals
            .iter()
            .filter(|(d, _, _, _)| *d == Direction::Short)
            .map(|(_, w, _, _)| w)
            .sum();

        let reasons: Vec<&str> = signals.iter().map(|(_, _, _, r)| *r).collect();
        let reasoning = format!("{} | {}", status, reasons.join(", "));

        // Calculate final confidence as weighted average
        if long_weighted > short_weighted && long_weight_total > 0.0 {
            let confidence = (long_weighted / long_weight_total).min(0.9);
            if confidence >= 0.5 {
                Signal::with_atr(Direction::Long, confidence, "pattern", &reasoning, atr_pct)
            } else {
                let mut signal = Signal::neutral("pattern", &reasoning);
                signal.atr_pct = Some(atr_pct);
                signal
            }
        } else if short_weighted > long_weighted && short_weight_total > 0.0 {
            let confidence = (short_weighted / short_weight_total).min(0.9);
            if confidence >= 0.5 {
                Signal::with_atr(Direction::Short, confidence, "pattern", &reasoning, atr_pct)
            } else {
                let mut signal = Signal::neutral("pattern", &reasoning);
                signal.atr_pct = Some(atr_pct);
                signal
            }
        } else {
            let mut signal = Signal::neutral("pattern", &reasoning);
            signal.atr_pct = Some(atr_pct);
            signal
        }
    }
}

#[async_trait]
impl Agent for PatternAgent {
    fn name(&self) -> &str {
        "pattern"
    }

    async fn analyze(&self) -> Result<Signal> {
        let ohlc = self.fetch_ohlc().await?;

        if ohlc.len() < self.config.ema_slow {
            return Ok(Signal::neutral(
                "pattern",
                &format!("Insufficient data: {} candles, need {}", ohlc.len(), self.config.ema_slow),
            ));
        }

        Ok(self.analyze_indicators(&ohlc))
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
