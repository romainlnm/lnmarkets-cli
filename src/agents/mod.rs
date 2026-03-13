//! Trading agents module
//!
//! Each agent analyzes a specific data source and produces trading signals.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod pattern;
pub mod macro_cal;

/// Trading direction
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

/// Signal produced by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Trading direction recommendation
    pub direction: Direction,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
    /// Agent that produced this signal
    pub source: String,
    /// Human-readable reasoning
    pub reasoning: String,
    /// Timestamp of the signal
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Signal {
    pub fn new(direction: Direction, confidence: f64, source: &str, reasoning: &str) -> Self {
        Self {
            direction,
            confidence: confidence.clamp(0.0, 1.0),
            source: source.to_string(),
            reasoning: reasoning.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn neutral(source: &str, reasoning: &str) -> Self {
        Self::new(Direction::Neutral, 0.5, source, reasoning)
    }
}

/// Trait for trading agents
#[async_trait]
pub trait Agent: Send + Sync {
    /// Agent name for logging and identification
    fn name(&self) -> &str;

    /// Analyze current market conditions and produce a signal
    async fn analyze(&self) -> anyhow::Result<Signal>;

    /// Initialize the agent (connect to data sources, etc.)
    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Cleanup resources
    async fn shutdown(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Registry of available agents
pub struct AgentRegistry {
    agents: Vec<Box<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self { agents: Vec::new() }
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) {
        self.agents.push(agent);
    }

    pub fn agents(&self) -> &[Box<dyn Agent>] {
        &self.agents
    }

    pub async fn analyze_all(&self) -> Vec<Signal> {
        let mut signals = Vec::new();
        for agent in &self.agents {
            match agent.analyze().await {
                Ok(signal) => signals.push(signal),
                Err(e) => {
                    eprintln!("[{}] Analysis failed: {}", agent.name(), e);
                }
            }
        }
        signals
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
