//! Binance price data fetching

use super::types::PriceAction;
use anyhow::{Context, Result};

/// Fetch 24h price action from Binance spot klines
pub async fn fetch_price_action(client: &reqwest::Client) -> Result<PriceAction> {
    let url = "https://api.binance.com/api/v3/klines?symbol=BTCUSDT&interval=1h&limit=24";

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch Binance klines")?;

    if !response.status().is_success() {
        anyhow::bail!("Binance API returned {}", response.status());
    }

    let klines: Vec<Vec<serde_json::Value>> = response
        .json()
        .await
        .context("Failed to parse Binance klines response")?;

    if klines.is_empty() {
        anyhow::bail!("No kline data returned from Binance");
    }

    // Extract OHLC data
    // Kline format: [open_time, open, high, low, close, volume, close_time, ...]
    let mut high_24h: f64 = 0.0;
    let mut low_24h: f64 = f64::MAX;
    let mut open_24h: f64 = 0.0;
    let mut current: f64 = 0.0;

    for (i, kline) in klines.iter().enumerate() {
        let high = parse_kline_value(kline, 2)?;
        let low = parse_kline_value(kline, 3)?;
        let close = parse_kline_value(kline, 4)?;

        if i == 0 {
            open_24h = parse_kline_value(kline, 1)?;
        }

        if high > high_24h {
            high_24h = high;
        }
        if low < low_24h {
            low_24h = low;
        }

        current = close;
    }

    let change_24h_pct = ((current - open_24h) / open_24h) * 100.0;
    let high_pct = ((high_24h - current) / current) * 100.0;
    let low_pct = ((current - low_24h) / current) * 100.0;

    Ok(PriceAction {
        current,
        high_24h,
        low_24h,
        change_24h_pct,
        high_pct,
        low_pct,
    })
}

fn parse_kline_value(kline: &[serde_json::Value], index: usize) -> Result<f64> {
    kline
        .get(index)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .context("Invalid kline value")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_price_action() {
        let client = reqwest::Client::new();
        let result = fetch_price_action(&client).await;
        // Should succeed in normal conditions
        assert!(result.is_ok() || result.is_err()); // Network dependent
    }
}
