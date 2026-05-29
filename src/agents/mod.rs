//! Data-collecting agents.
//!
//! Each agent gathers structured observations from one data source (price
//! action, order flow, calendar, news, whale positions). The LLM arbiter
//! (`llm::LlmArbiter`) interprets all of them together and makes the trade
//! decision. The agents themselves no longer score Long/Short.
//!
//! See issue #16.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod flow;
pub mod llm;
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
