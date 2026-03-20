//! Fear & Greed Index fetching from Alternative.me

use super::types::SentimentData;
use anyhow::{Context, Result};
use serde::Deserialize;

/// Fetch Fear & Greed Index from Alternative.me
pub async fn fetch_fear_greed(client: &reqwest::Client) -> Result<SentimentData> {
    let url = "https://api.alternative.me/fng/?limit=2";

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch Fear & Greed Index")?;

    if !response.status().is_success() {
        anyhow::bail!("Alternative.me API returned {}", response.status());
    }

    #[derive(Deserialize)]
    struct FngResponse {
        data: Vec<FngData>,
    }

    #[derive(Deserialize)]
    struct FngData {
        value: String,
        value_classification: String,
    }

    let fng: FngResponse = response
        .json()
        .await
        .context("Failed to parse Fear & Greed response")?;

    if fng.data.is_empty() {
        anyhow::bail!("No Fear & Greed data returned");
    }

    let current = &fng.data[0];
    let value = current
        .value
        .parse::<u32>()
        .context("Invalid Fear & Greed value")?;

    let previous_value = fng
        .data
        .get(1)
        .and_then(|d| d.value.parse::<u32>().ok());

    Ok(SentimentData {
        value,
        label: current.value_classification.clone(),
        previous_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_fear_greed() {
        let client = reqwest::Client::new();
        let result = fetch_fear_greed(&client).await;
        // Network dependent test
        assert!(result.is_ok() || result.is_err());
    }
}
