//! Pattern collector — fetches recent BTC candles from Binance and returns the
//! computed technical indicators as raw numbers. The LLM arbiter interprets
//! them; no Long/Short scoring here.

use super::DataCollector;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct PatternConfig {
    pub rsi_period: usize,
    pub ema_fast: usize,
    pub ema_slow: usize,
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
    pub bb_period: usize,
    pub bb_std_dev: f64,
    pub symbol: String,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            rsi_period: 14,
            ema_fast: 9,
            ema_slow: 21,
            macd_fast: 12,
            macd_slow: 26,
            macd_signal: 9,
            bb_period: 20,
            bb_std_dev: 2.0,
            symbol: "BTCUSDT".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct OhlcPoint {
    high: f64,
    low: f64,
    close: f64,
}

pub struct PatternAgent {
    config: PatternConfig,
    http_client: reqwest::Client,
}

impl PatternAgent {
    pub fn new(config: PatternConfig) -> Self {
        Self {
            config,
            http_client: super::http_client(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PatternConfig::default())
    }

    async fn fetch_ohlc(&self, interval: &str, limit: u32) -> Result<Vec<OhlcPoint>> {
        let url = format!(
            "https://api.binance.com/api/v3/klines?symbol={}&interval={}&limit={}",
            self.config.symbol, interval, limit
        );
        let response: Vec<Vec<serde_json::Value>> = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("fetch Binance klines")?
            .json()
            .await
            .context("parse Binance response")?;

        let ohlc = response
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

    fn closes(ohlc: &[OhlcPoint]) -> Vec<f64> {
        ohlc.iter().map(|p| p.close).collect()
    }

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
            let tr = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
            tr_sum += tr;
        }
        Some(tr_sum / period as f64)
    }

    fn calculate_atr_pct(ohlc: &[OhlcPoint], period: usize) -> Option<f64> {
        let atr = Self::calculate_atr(ohlc, period)?;
        let current_price = ohlc.last()?.close;
        if current_price > 0.0 {
            Some((atr / current_price) * 100.0)
        } else {
            None
        }
    }

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

    fn calculate_bollinger(
        prices: &[f64],
        period: usize,
        std_dev_mult: f64,
    ) -> Option<(f64, f64, f64)> {
        if prices.len() < period {
            return None;
        }
        let recent: &[f64] = &prices[prices.len() - period..];
        let sma: f64 = recent.iter().sum::<f64>() / period as f64;
        let variance: f64 =
            recent.iter().map(|p| (p - sma).powi(2)).sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();
        let upper = sma + std_dev_mult * std_dev;
        let lower = sma - std_dev_mult * std_dev;
        Some((lower, sma, upper))
    }

    fn calculate_macd(
        prices: &[f64],
        fast: usize,
        slow: usize,
        signal: usize,
    ) -> Option<(f64, f64, f64)> {
        if prices.len() < slow + signal {
            return None;
        }
        let ema_fast = Self::calculate_ema(prices, fast)?;
        let ema_slow = Self::calculate_ema(prices, slow)?;
        let macd_line = ema_fast - ema_slow;
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
            return Some((macd_line, macd_line, 0.0));
        }
        let multiplier = 2.0 / (signal as f64 + 1.0);
        let mut signal_line = macd_values[0..signal].iter().sum::<f64>() / signal as f64;
        for val in macd_values.iter().skip(signal) {
            signal_line = (val - signal_line) * multiplier + signal_line;
        }
        let histogram = macd_line - signal_line;
        Some((macd_line, signal_line, histogram))
    }
}

impl PatternAgent {
    /// Indicator block for one timeframe's candles.
    fn timeframe_report(&self, ohlc: &[OhlcPoint]) -> Value {
        let prices = Self::closes(ohlc);
        let current_price = *prices.last().unwrap_or(&0.0);
        let rsi = Self::calculate_rsi(&prices, self.config.rsi_period);
        let ema_fast = Self::calculate_ema(&prices, self.config.ema_fast);
        let ema_slow = Self::calculate_ema(&prices, self.config.ema_slow);
        let bollinger =
            Self::calculate_bollinger(&prices, self.config.bb_period, self.config.bb_std_dev);
        let macd = Self::calculate_macd(
            &prices,
            self.config.macd_fast,
            self.config.macd_slow,
            self.config.macd_signal,
        );

        json!({
            "rsi_14": rsi,
            "ema": ema_fast.zip(ema_slow).map(|(f, s)| json!({
                format!("ema_{}", self.config.ema_fast): f,
                format!("ema_{}", self.config.ema_slow): s,
                "fast_minus_slow_pct": (f - s) / s * 100.0,
            })),
            "macd": macd.map(|(line, sig, hist)| json!({
                "line": line,
                "signal": sig,
                "histogram": hist,
            })),
            "bollinger": bollinger.map(|(lo, mid, hi)| json!({
                "lower": lo,
                "middle": mid,
                "upper": hi,
                "position": if current_price <= lo { "at_or_below_lower" }
                    else if current_price >= hi { "at_or_above_upper" }
                    else { "inside" },
            })),
            "atr_pct": Self::calculate_atr_pct(ohlc, 14),
        })
    }
}

/// % change between the close `lookback` candles ago and the latest close.
fn change_pct(prices: &[f64], lookback: usize) -> Option<f64> {
    if prices.len() <= lookback {
        return None;
    }
    let current = *prices.last()?;
    let prev = prices[prices.len() - 1 - lookback];
    if prev > 0.0 {
        Some((current - prev) / prev * 100.0)
    } else {
        None
    }
}

fn highest(ohlc: &[OhlcPoint]) -> Option<f64> {
    ohlc.iter().map(|p| p.high).fold(None, |acc, h| {
        Some(acc.map_or(h, |a: f64| a.max(h)))
    })
}

fn lowest(ohlc: &[OhlcPoint]) -> Option<f64> {
    ohlc.iter().map(|p| p.low).fold(None, |acc, l| {
        Some(acc.map_or(l, |a: f64| a.min(l)))
    })
}

#[async_trait]
impl DataCollector for PatternAgent {
    fn name(&self) -> &str {
        "pattern"
    }

    async fn collect(&self) -> Result<Value> {
        // Multi-timeframe view: 1m for entry timing, 5m for momentum, 1h for
        // trend. Fetched concurrently.
        let (m1, m5, h1) = tokio::try_join!(
            self.fetch_ohlc("1m", 120),
            self.fetch_ohlc("5m", 100),
            self.fetch_ohlc("1h", 100),
        )?;
        if m1.len() < self.config.ema_slow {
            return Ok(json!({
                "error": format!("insufficient candles: {} < {}", m1.len(), self.config.ema_slow),
            }));
        }

        let m1_closes = Self::closes(&m1);
        let h1_closes = Self::closes(&h1);
        let current_price = *m1_closes.last().unwrap_or(&0.0);

        // Recent levels from 1h candles: last 24h and the full fetched window
        // (~4 days) — support/resistance context for the arbiter.
        let last_24h = if h1.len() > 24 { &h1[h1.len() - 24..] } else { &h1[..] };
        let window_hours = h1.len();

        Ok(json!({
            "price": current_price,
            "change_1h_pct": change_pct(&m1_closes, 60),
            "change_24h_pct": change_pct(&h1_closes, 24),
            "timeframes": {
                "1m": self.timeframe_report(&m1),
                "5m": self.timeframe_report(&m5),
                "1h": self.timeframe_report(&h1),
            },
            "levels": {
                "high_24h": highest(last_24h),
                "low_24h": lowest(last_24h),
                format!("high_{}h", window_hours): highest(&h1),
                format!("low_{}h", window_hours): lowest(&h1),
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rsi_calculation() {
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
        let prices: Vec<f64> =
            (0..25).map(|i| 100.0 + (i as f64 * 0.1).sin() * 5.0).collect();
        let bb = PatternAgent::calculate_bollinger(&prices, 20, 2.0);
        assert!(bb.is_some());
        let (lower, mid, upper) = bb.unwrap();
        assert!(lower < mid);
        assert!(mid < upper);
    }
}
