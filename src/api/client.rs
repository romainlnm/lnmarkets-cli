use anyhow::{Context, Result};
use reqwest::{Client, Method, Response};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::config::{Credentials, Network};
use super::auth::{generate_signature, get_timestamp};
use super::error::ApiError;

const API_VERSION: &str = "v3";
const USER_AGENT: &str = concat!("lnm-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct LnmClient {
    client: Client,
    base_url: String,
    credentials: Option<Credentials>,
}

impl LnmClient {
    pub fn new(network: Network, credentials: Option<Credentials>) -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: network.base_url().to_string(),
            credentials,
        })
    }

    /// Make an authenticated request
    pub async fn request<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let response = self.raw_request(method, path, body).await?;
        self.handle_response(response).await
    }

    /// Make a public (unauthenticated) request
    pub async fn public_request<T>(&self, method: Method, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}/{}/{}", self.base_url, API_VERSION, path.trim_start_matches('/'));

        let response = self.client
            .request(method, &url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("Failed to send request")?;

        self.handle_response(response).await
    }

    async fn raw_request<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Response>
    where
        B: Serialize,
    {
        let creds = self.credentials.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Authentication required. Run 'lnm auth login' first."))?;

        let api_key = creds.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("API key not configured"))?;
        let api_secret = creds.api_secret.as_ref()
            .ok_or_else(|| anyhow::anyhow!("API secret not configured"))?;
        let passphrase = creds.passphrase.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Passphrase not configured"))?;

        let full_path = format!("/{}/{}", API_VERSION, path.trim_start_matches('/'));
        let url = format!("{}{}", self.base_url, full_path);

        let body_str = match body {
            Some(b) => serde_json::to_string(b).context("Failed to serialize request body")?,
            None => String::new(),
        };

        let timestamp = get_timestamp();
        let signature = generate_signature(
            api_secret,
            timestamp,
            method.as_str(),
            &full_path,
            &body_str,
        );

        let mut request = self.client
            .request(method, &url)
            .header("Content-Type", "application/json")
            .header("LNM-ACCESS-KEY", api_key)
            .header("LNM-ACCESS-SIGNATURE", signature)
            .header("LNM-ACCESS-PASSPHRASE", passphrase)
            .header("LNM-ACCESS-TIMESTAMP", timestamp.to_string());

        if !body_str.is_empty() {
            request = request.body(body_str);
        }

        request.send().await.context("Failed to send request")
    }

    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        let status = response.status();
        let body = response.text().await.context("Failed to read response body")?;

        if !status.is_success() {
            // Try to parse error response
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&body) {
                anyhow::bail!("{}", api_error);
            }
            anyhow::bail!("API error ({}): {}", status, body);
        }

        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse response: {}", body))
    }

    /// Get raw JSON response (useful for debugging)
    pub async fn request_raw<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<serde_json::Value>
    where
        B: Serialize,
    {
        self.request(method, path, body).await
    }
}
