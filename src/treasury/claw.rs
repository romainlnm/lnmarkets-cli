//! Claw-cash integration for AI treasury management
//!
//! Connects to a running claw-cash daemon to:
//! - Check wallet balance
//! - Request Lightning invoices (for deposits)
//! - Pay Lightning invoices (for withdrawals)
//!
//! See: https://github.com/tiero/claw-cash

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Claw-cash daemon client
#[derive(Debug, Clone)]
pub struct ClawClient {
    base_url: String,
    http: reqwest::Client,
}

/// Balance response from claw-cash
#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    /// Total balance in sats
    #[serde(default)]
    pub total: u64,
    /// Confirmed balance in sats
    #[serde(default)]
    pub confirmed: u64,
    /// Pending balance in sats
    #[serde(default)]
    pub pending: u64,
    /// Offchain (Lightning/Ark) balance in sats
    #[serde(default)]
    pub offchain: u64,
}

/// Receive request for creating invoices
#[derive(Debug, Serialize)]
pub struct ReceiveRequest {
    pub currency: String,
    pub amount: u64,
    #[serde(rename = "where")]
    pub destination: String,
}

/// Receive response with invoice/address
#[derive(Debug, Deserialize)]
pub struct ReceiveResponse {
    /// Lightning invoice (bolt11)
    #[serde(default)]
    pub invoice: Option<String>,
    /// On-chain or Ark address
    #[serde(default)]
    pub address: Option<String>,
    /// Payment URL
    #[serde(default)]
    pub url: Option<String>,
}

/// Send request for paying invoices
#[derive(Debug, Serialize)]
pub struct SendRequest {
    /// Bolt11 invoice to pay
    pub bolt11: String,
}

/// Send response
#[derive(Debug, Deserialize)]
pub struct SendResponse {
    /// Payment preimage (proof of payment)
    #[serde(default)]
    pub preimage: Option<String>,
    /// Payment hash
    #[serde(default)]
    pub payment_hash: Option<String>,
    /// Amount paid in sats
    #[serde(default)]
    pub amount: Option<u64>,
    /// Fee paid in sats
    #[serde(default)]
    pub fee: Option<u64>,
}

impl ClawClient {
    /// Create a new claw-cash client
    pub fn new(daemon_url: &str) -> Self {
        let base_url = daemon_url.trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self { base_url, http }
    }

    /// Default localhost connection
    pub fn localhost(port: u16) -> Self {
        Self::new(&format!("http://127.0.0.1:{}", port))
    }

    /// Check if claw-cash daemon is running
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Get wallet balance
    pub async fn get_balance(&self) -> Result<BalanceResponse> {
        let url = format!("{}/balance", self.base_url);
        let resp = self.http
            .get(&url)
            .send()
            .await
            .context("Failed to connect to claw-cash daemon")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Claw-cash error ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse balance response")
    }

    /// Request a Lightning invoice to receive sats
    pub async fn create_invoice(&self, amount_sats: u64) -> Result<String> {
        let url = format!("{}/receive", self.base_url);
        let req = ReceiveRequest {
            currency: "sats".to_string(),
            amount: amount_sats,
            destination: "lightning".to_string(),
        };

        let resp = self.http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("Failed to connect to claw-cash daemon")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Claw-cash error ({}): {}", status, body);
        }

        let data: ReceiveResponse = resp.json().await
            .context("Failed to parse receive response")?;

        data.invoice
            .ok_or_else(|| anyhow::anyhow!("No invoice in response"))
    }

    /// Pay a Lightning invoice
    pub async fn pay_invoice(&self, bolt11: &str) -> Result<SendResponse> {
        let url = format!("{}/send", self.base_url);
        let req = SendRequest {
            bolt11: bolt11.to_string(),
        };

        let resp = self.http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("Failed to connect to claw-cash daemon")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Claw-cash payment failed ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse send response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = ClawClient::new("http://localhost:3000");
        assert_eq!(client.base_url, "http://localhost:3000");

        let client = ClawClient::localhost(3001);
        assert_eq!(client.base_url, "http://127.0.0.1:3001");
    }
}
