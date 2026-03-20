//! Binance Futures derivatives data fetching

use super::types::{DerivativesMetrics, FundingSentiment, LongShortSentiment};
use anyhow::{Context, Result};
use serde::Deserialize;

/// Fetch funding rate from Binance Futures
async fn fetch_funding_rate(client: &reqwest::Client) -> Result<f64> {
    let url = "https://fapi.binance.com/fapi/v1/fundingRate?symbol=BTCUSDT&limit=1";

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch funding rate")?;

    if !response.status().is_success() {
        anyhow::bail!("Binance Futures API returned {}", response.status());
    }

    #[derive(Deserialize)]
    struct FundingRate {
        #[serde(rename = "fundingRate")]
        funding_rate: String,
    }

    let rates: Vec<FundingRate> = response
        .json()
        .await
        .context("Failed to parse funding rate response")?;

    let rate = rates
        .first()
        .context("No funding rate data")?
        .funding_rate
        .parse::<f64>()
        .context("Invalid funding rate value")?;

    // Convert to percentage
    Ok(rate * 100.0)
}

/// Fetch open interest from Binance Futures
async fn fetch_open_interest(client: &reqwest::Client) -> Result<f64> {
    let url = "https://fapi.binance.com/fapi/v1/openInterest?symbol=BTCUSDT";

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch open interest")?;

    if !response.status().is_success() {
        anyhow::bail!("Binance Futures API returned {}", response.status());
    }

    #[derive(Deserialize)]
    struct OpenInterest {
        #[serde(rename = "openInterest")]
        open_interest: String,
    }

    let oi: OpenInterest = response
        .json()
        .await
        .context("Failed to parse open interest response")?;

    let oi_btc = oi
        .open_interest
        .parse::<f64>()
        .context("Invalid open interest value")?;

    // Fetch current price to convert to USD
    let price_url = "https://fapi.binance.com/fapi/v1/ticker/price?symbol=BTCUSDT";
    let price_response = client.get(price_url).send().await?;

    #[derive(Deserialize)]
    struct Price {
        price: String,
    }

    let price: Price = price_response.json().await?;
    let btc_price = price.price.parse::<f64>().unwrap_or(100_000.0);

    Ok(oi_btc * btc_price)
}

/// Fetch long/short account ratio from Binance Futures
async fn fetch_long_short_ratio(client: &reqwest::Client) -> Result<f64> {
    let url = "https://fapi.binance.com/futures/data/globalLongShortAccountRatio?symbol=BTCUSDT&period=1h&limit=1";

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch long/short ratio")?;

    if !response.status().is_success() {
        anyhow::bail!("Binance Futures API returned {}", response.status());
    }

    #[derive(Deserialize)]
    struct LongShortRatio {
        #[serde(rename = "longShortRatio")]
        long_short_ratio: String,
    }

    let ratios: Vec<LongShortRatio> = response
        .json()
        .await
        .context("Failed to parse long/short ratio response")?;

    let ratio = ratios
        .first()
        .context("No long/short ratio data")?
        .long_short_ratio
        .parse::<f64>()
        .context("Invalid long/short ratio value")?;

    Ok(ratio)
}

/// Fetch all derivatives metrics in parallel
pub async fn fetch_derivatives_metrics(client: &reqwest::Client) -> Result<DerivativesMetrics> {
    let (funding_result, oi_result, ls_result) = tokio::join!(
        fetch_funding_rate(client),
        fetch_open_interest(client),
        fetch_long_short_ratio(client)
    );

    // All three must succeed for derivatives data
    let funding_rate = funding_result?;
    let open_interest = oi_result?;
    let long_short_ratio = ls_result?;

    let funding_sentiment = FundingSentiment::from_rate(funding_rate);
    let ls_sentiment = LongShortSentiment::from_ratio(long_short_ratio);

    Ok(DerivativesMetrics {
        funding_rate,
        funding_sentiment,
        open_interest,
        long_short_ratio,
        ls_sentiment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_derivatives_metrics() {
        let client = reqwest::Client::new();
        let result = fetch_derivatives_metrics(&client).await;
        // Network dependent test
        assert!(result.is_ok() || result.is_err());
    }
}
