//! Daemon mode for continuous agent-based trading
//!
//! Runs agents in a loop, combines signals, and optionally executes trades.

use crate::agents::{pattern::PatternAgent, macro_cal::MacroAgent, Agent, AgentRegistry, Direction, Signal};
use crate::api::LnmClient;
use anyhow::Result;
use std::time::Duration;
use tokio::time::interval;

/// Daemon configuration
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Analysis interval in seconds
    pub interval_secs: u64,
    /// Dry run mode (no actual trades)
    pub dry_run: bool,
    /// Minimum confidence to act
    pub min_confidence: f64,
    /// Maximum position size in sats
    pub max_position_sats: u64,
    /// Enabled agents
    pub agents: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            dry_run: true,
            min_confidence: 0.7,
            max_position_sats: 100_000,
            agents: vec!["pattern".to_string()],
        }
    }
}

/// Trading daemon
pub struct Daemon {
    config: DaemonConfig,
    registry: AgentRegistry,
    client: Option<LnmClient>,
}

impl Daemon {
    pub fn new(config: DaemonConfig, client: Option<LnmClient>) -> Self {
        let mut registry = AgentRegistry::new();

        // Register enabled agents
        for agent_name in &config.agents {
            match agent_name.as_str() {
                "pattern" => {
                    registry.register(Box::new(PatternAgent::with_defaults()));
                }
                "macro" => {
                    registry.register(Box::new(MacroAgent::with_defaults()));
                }
                // "news" => { ... }
                _ => {
                    eprintln!("Unknown agent: {}", agent_name);
                }
            }
        }

        Self {
            config,
            registry,
            client,
        }
    }

    /// Run the daemon loop
    pub async fn run(&self) -> Result<()> {
        println!("Starting LN Markets trading daemon...");
        println!("  Interval: {}s", self.config.interval_secs);
        println!("  Dry run: {}", self.config.dry_run);
        println!("  Min confidence: {:.0}%", self.config.min_confidence * 100.0);
        println!("  Agents: {:?}", self.config.agents);
        println!();

        let mut ticker = interval(Duration::from_secs(self.config.interval_secs));

        loop {
            ticker.tick().await;

            println!("[{}] Analyzing...", chrono::Utc::now().format("%H:%M:%S"));

            // Collect signals from all agents
            let signals = self.registry.analyze_all().await;

            if signals.is_empty() {
                println!("  No signals received");
                continue;
            }

            // Print signals
            for signal in &signals {
                let conf_pct = signal.confidence * 100.0;
                let icon = match signal.direction {
                    Direction::Long => "\x1b[32m▲\x1b[0m",
                    Direction::Short => "\x1b[31m▼\x1b[0m",
                    Direction::Neutral => "\x1b[33m●\x1b[0m",
                };
                println!(
                    "  {} [{}] {} ({:.0}%): {}",
                    icon, signal.source, signal.direction, conf_pct, signal.reasoning
                );
            }

            // Combine signals and decide
            if let Some(action) = self.decide(&signals) {
                self.execute_action(action).await;
            }

            println!();
        }
    }

    /// Decide on trading action based on combined signals
    fn decide(&self, signals: &[Signal]) -> Option<TradeAction> {
        if signals.is_empty() {
            return None;
        }

        // Calculate weighted direction
        let mut long_weight = 0.0;
        let mut short_weight = 0.0;

        for signal in signals {
            match signal.direction {
                Direction::Long => long_weight += signal.confidence,
                Direction::Short => short_weight += signal.confidence,
                Direction::Neutral => {}
            }
        }

        let total = long_weight + short_weight;
        if total == 0.0 {
            return None;
        }

        let direction = if long_weight > short_weight {
            Direction::Long
        } else if short_weight > long_weight {
            Direction::Short
        } else {
            return None;
        };

        let confidence = (long_weight.max(short_weight)) / signals.len() as f64;

        if confidence < self.config.min_confidence {
            println!(
                "  → Confidence {:.0}% below threshold {:.0}%, no action",
                confidence * 100.0,
                self.config.min_confidence * 100.0
            );
            return None;
        }

        // Calculate position size based on confidence
        let size_factor = (confidence - self.config.min_confidence) / (1.0 - self.config.min_confidence);
        let position_sats = (self.config.max_position_sats as f64 * size_factor * 0.5) as u64;

        Some(TradeAction {
            direction,
            confidence,
            position_sats,
            reasons: signals.iter().map(|s| s.reasoning.clone()).collect(),
        })
    }

    /// Execute a trading action
    async fn execute_action(&self, action: TradeAction) {
        let side = match action.direction {
            Direction::Long => "buy",
            Direction::Short => "sell",
            Direction::Neutral => return,
        };

        println!(
            "  \x1b[1m→ ACTION: {} {} sats ({:.0}% confidence)\x1b[0m",
            side.to_uppercase(),
            action.position_sats,
            action.confidence * 100.0
        );

        if self.config.dry_run {
            println!("  [DRY RUN] Would execute: {} {} sats", side, action.position_sats);
            return;
        }

        // Execute actual trade
        if let Some(client) = &self.client {
            match self.place_order(client, &action).await {
                Ok(order_id) => {
                    println!("  Order placed: {}", order_id);
                }
                Err(e) => {
                    eprintln!("  Order failed: {}", e);
                }
            }
        }
    }

    async fn place_order(&self, _client: &LnmClient, action: &TradeAction) -> Result<String> {
        // TODO: Implement actual order placement using client
        // For now, return a mock order ID
        let side = match action.direction {
            Direction::Long => "b",
            Direction::Short => "s",
            Direction::Neutral => "n",
        };
        Ok(format!("mock-{}-{}", side, action.position_sats))
    }
}

/// A trading action to execute
#[derive(Debug)]
struct TradeAction {
    direction: Direction,
    confidence: f64,
    position_sats: u64,
    #[allow(dead_code)]
    reasons: Vec<String>,
}
