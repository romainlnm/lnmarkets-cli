//! Macro collector — fetches the economic calendar from TradingView and
//! returns recent releases (with actual/forecast/surprise) + upcoming events.
//! The LLM decides what any of it means for BTC.

use super::DataCollector;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct MacroConfig {
    pub lookback_hours: i64,
    pub lookahead_days: i64,
    pub max_events: usize,
}

impl Default for MacroConfig {
    fn default() -> Self {
        Self {
            lookback_hours: 6,
            lookahead_days: 3,
            max_events: 10,
        }
    }
}

pub struct MacroAgent {
    config: MacroConfig,
    http_client: reqwest::Client,
}

impl MacroAgent {
    pub fn new(config: MacroConfig) -> Self {
        Self {
            config,
            http_client: super::http_client(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(MacroConfig::default())
    }

    async fn fetch_events(&self, from: &str, to: &str) -> Result<Vec<TradingViewEvent>> {
        let url = format!(
            "https://economic-calendar.tradingview.com/events?from={}&to={}&countries=US",
            from, to
        );
        let response = self
            .http_client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .header("Accept", "application/json")
            .header("Origin", "https://www.tradingview.com")
            .header("Referer", "https://www.tradingview.com/")
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("TradingView API returned {}", response.status());
        }
        let data: TradingViewResponse = response.json().await?;
        Ok(data.result.unwrap_or_default())
    }
}

#[async_trait]
impl DataCollector for MacroAgent {
    fn name(&self) -> &str {
        "macro"
    }

    async fn collect(&self) -> Result<Value> {
        let now = Utc::now();
        let from = (now - chrono::Duration::hours(self.config.lookback_hours))
            .format("%Y-%m-%d")
            .to_string();
        let to = (now + chrono::Duration::days(self.config.lookahead_days))
            .format("%Y-%m-%d")
            .to_string();

        let events = self.fetch_events(&from, &to).await?;
        let mut recent = Vec::new();
        let mut upcoming = Vec::new();

        for event in events {
            if event.importance < 0 {
                continue;
            }
            let event_time = match chrono::DateTime::parse_from_rfc3339(&event.date) {
                Ok(t) => t.with_timezone(&Utc),
                Err(_) => continue,
            };
            let delta_min = (event_time - now).num_minutes();

            if delta_min < 0 {
                // Past — record only if it has an actual value (the surprise data).
                if let (Some(actual), Some(forecast)) = (event.actual, event.forecast) {
                    let surprise_pct = if forecast != 0.0 {
                        ((actual - forecast) / forecast.abs()) * 100.0
                    } else {
                        0.0
                    };
                    recent.push(json!({
                        "title": event.title,
                        "importance": event.importance,
                        "minutes_ago": -delta_min,
                        "actual": actual,
                        "forecast": forecast,
                        "previous": event.previous,
                        "surprise_pct": surprise_pct,
                    }));
                }
            } else {
                upcoming.push(json!({
                    "title": event.title,
                    "importance": event.importance,
                    "minutes_until": delta_min,
                    "forecast": event.forecast,
                    "previous": event.previous,
                }));
            }

            if recent.len() + upcoming.len() >= self.config.max_events {
                break;
            }
        }

        Ok(json!({
            "recent_releases": recent,
            "upcoming_events": upcoming,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct TradingViewResponse {
    #[serde(default)]
    result: Option<Vec<TradingViewEvent>>,
}

#[derive(Debug, Deserialize)]
struct TradingViewEvent {
    title: String,
    date: String,
    importance: i32,
    #[serde(default)]
    actual: Option<f64>,
    #[serde(default)]
    forecast: Option<f64>,
    #[serde(default)]
    previous: Option<f64>,
}
