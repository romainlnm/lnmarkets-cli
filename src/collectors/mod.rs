//! Data collectors.
//!
//! Each collector gathers structured observations from one data source
//! (price action, order flow, calendar, news, whale positions). The LLM
//! arbiter (`crate::llm::LlmArbiter`) — the single decision maker — reads
//! all of them together and makes the trade decision. Collectors don't
//! score Long/Short.
//!
//! See issue #16.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// Shared HTTP client constructor with a hard request timeout. Without one,
/// a hung connection blocks the daemon loop until the OS abandons the TCP
/// connection (~15 min observed) — during which TP/SL checks don't run.
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}

pub mod flow;
pub mod macro_cal;
pub mod news;
pub mod pattern;
pub mod whale;

/// Trade side, used by execution code and the arbiter's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Long,
    Short,
    Neutral,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Long => write!(f, "LONG"),
            Direction::Short => write!(f, "SHORT"),
            Direction::Neutral => write!(f, "NEUTRAL"),
        }
    }
}

/// Trait for agents that collect structured observations.
///
/// The return value is intentionally `serde_json::Value` so each collector
/// can shape its output without dragging schema decisions into a shared type.
/// The LLM prompt renders whatever each collector returns.
#[async_trait]
pub trait DataCollector: Send + Sync {
    fn name(&self) -> &str;
    async fn collect(&self) -> anyhow::Result<Value>;
}
