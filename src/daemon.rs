//! Daemon mode for continuous agent-based trading
//!
//! Runs agents in a loop, combines signals, and optionally executes trades.

use crate::agents::{pattern::PatternAgent, macro_cal::MacroAgent, news::NewsAgent, flow::FlowAgent, Agent, AgentRegistry, Direction, Signal};
use crate::api::LnmClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;

/// Trading mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradingMode {
    /// No trades, minimal logging
    DryRun,
    /// No real trades, detailed logging with simulated P&L
    Paper,
    /// Real trades
    Live,
}

/// Daemon configuration
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Analysis interval in seconds
    pub interval_secs: u64,
    /// Trading mode
    pub mode: TradingMode,
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
            mode: TradingMode::DryRun,
            min_confidence: 0.5,
            max_position_sats: 100_000,
            agents: vec!["pattern".to_string()],
        }
    }
}

/// Paper trade record
#[derive(Debug, Clone)]
struct PaperTrade {
    id: u64,
    direction: Direction,
    size_sats: u64,
    entry_price: f64,
    entry_time: DateTime<Utc>,
    confidence: f64,
    closed: bool,
    exit_price: Option<f64>,
    exit_time: Option<DateTime<Utc>>,
    pnl_sats: Option<i64>,
}

/// Paper trading state
struct PaperState {
    trades: Vec<PaperTrade>,
    next_id: u64,
    total_pnl: i64,
    wins: u32,
    losses: u32,
}

/// Trading daemon
pub struct Daemon {
    config: DaemonConfig,
    registry: AgentRegistry,
    client: Option<LnmClient>,
    paper_state: RwLock<PaperState>,
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
                "news" => {
                    registry.register(Box::new(NewsAgent::with_defaults()));
                }
                "flow" => {
                    registry.register(Box::new(FlowAgent::with_defaults()));
                }
                _ => {
                    eprintln!("Unknown agent: {}", agent_name);
                }
            }
        }

        Self {
            config,
            registry,
            client,
            paper_state: RwLock::new(PaperState {
                trades: Vec::new(),
                next_id: 1,
                total_pnl: 0,
                wins: 0,
                losses: 0,
            }),
        }
    }

    /// Fetch current BTC price from Binance
    async fn get_current_price(&self) -> Result<f64> {
        let url = "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT";
        let client = reqwest::Client::new();
        let resp: serde_json::Value = client.get(url).send().await?.json().await?;
        let price = resp["price"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No price in response"))?
            .parse::<f64>()?;
        Ok(price)
    }

    /// Run the daemon loop
    pub async fn run(&self) -> Result<()> {
        let mode_str = match self.config.mode {
            TradingMode::DryRun => "DRY RUN",
            TradingMode::Paper => "PAPER TRADING",
            TradingMode::Live => "\x1b[31mLIVE TRADING\x1b[0m",
        };

        println!("Starting LN Markets trading daemon...");
        println!("  Mode: {}", mode_str);
        println!("  Interval: {}s", self.config.interval_secs);
        println!("  Min confidence: {:.0}%", self.config.min_confidence * 100.0);
        println!("  Max position: {} sats", self.config.max_position_sats);
        println!("  Agents: {:?}", self.config.agents);
        println!();

        if self.config.mode == TradingMode::Paper {
            println!("\x1b[36m  Paper trading tracks simulated P&L with real prices.\x1b[0m");
            println!();
        }

        let mut ticker = interval(Duration::from_secs(self.config.interval_secs));

        loop {
            ticker.tick().await;

            println!("[{}] Analyzing...", chrono::Utc::now().format("%H:%M:%S"));

            // In paper mode, check and close open positions
            if self.config.mode == TradingMode::Paper {
                self.check_paper_positions().await;
            }

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

            // Show paper trading stats
            if self.config.mode == TradingMode::Paper {
                self.print_paper_stats().await;
            }

            println!();
        }
    }

    /// Check paper positions and close if signal reversed
    async fn check_paper_positions(&self) {
        let price = match self.get_current_price().await {
            Ok(p) => p,
            Err(_) => return,
        };

        let mut state = self.paper_state.write().await;

        // Collect updates to apply after iteration
        let mut pnl_total: i64 = 0;
        let mut wins: u32 = 0;
        let mut losses: u32 = 0;
        let mut closed_trades: Vec<(u64, f64, i64)> = Vec::new(); // (id, entry_price, pnl)

        for trade in state.trades.iter_mut().filter(|t| !t.closed) {
            let hold_mins = (Utc::now() - trade.entry_time).num_minutes();

            // Auto-close after 30 minutes for paper testing
            if hold_mins >= 30 {
                let pnl = match trade.direction {
                    Direction::Long => ((price - trade.entry_price) / trade.entry_price * trade.size_sats as f64) as i64,
                    Direction::Short => ((trade.entry_price - price) / trade.entry_price * trade.size_sats as f64) as i64,
                    Direction::Neutral => 0,
                };

                trade.closed = true;
                trade.exit_price = Some(price);
                trade.exit_time = Some(Utc::now());
                trade.pnl_sats = Some(pnl);

                pnl_total += pnl;
                if pnl > 0 {
                    wins += 1;
                } else {
                    losses += 1;
                }

                closed_trades.push((trade.id, trade.entry_price, pnl));
            }
        }

        // Apply accumulated updates
        state.total_pnl += pnl_total;
        state.wins += wins;
        state.losses += losses;

        // Print closed trades
        for (id, entry_price, pnl) in closed_trades {
            let pnl_color = if pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
            println!(
                "  \x1b[36m[PAPER CLOSE]\x1b[0m #{} @ ${:.0} → ${:.0} | P&L: {}{}{} sats\x1b[0m",
                id,
                entry_price,
                price,
                pnl_color,
                if pnl >= 0 { "+" } else { "" },
                pnl,
            );
        }
    }

    /// Print paper trading statistics
    async fn print_paper_stats(&self) {
        let state = self.paper_state.read().await;
        let open_positions: Vec<_> = state.trades.iter().filter(|t| !t.closed).collect();

        if state.trades.is_empty() && open_positions.is_empty() {
            return;
        }

        // Calculate unrealized P&L for open positions
        let current_price = self.get_current_price().await.unwrap_or(0.0);
        let mut unrealized_pnl: i64 = 0;

        for trade in &open_positions {
            let pnl = match trade.direction {
                Direction::Long => ((current_price - trade.entry_price) / trade.entry_price * trade.size_sats as f64) as i64,
                Direction::Short => ((trade.entry_price - current_price) / trade.entry_price * trade.size_sats as f64) as i64,
                Direction::Neutral => 0,
            };
            unrealized_pnl += pnl;
        }

        let total_pnl = state.total_pnl + unrealized_pnl;
        let pnl_color = if total_pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
        let unrealized_color = if unrealized_pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
        let total = state.wins + state.losses;
        let win_rate = if total > 0 { state.wins as f64 / total as f64 * 100.0 } else { 0.0 };

        println!(
            "  \x1b[36m[PAPER]\x1b[0m Open: {} ({}{:+} sats\x1b[0m) | Closed: {} | W/L: {}/{} ({:.0}%) | Total P&L: {}{:+} sats\x1b[0m",
            open_positions.len(),
            unrealized_color,
            unrealized_pnl,
            total,
            state.wins,
            state.losses,
            win_rate,
            pnl_color,
            total_pnl,
        );
    }

    /// Decide on trading action based on combined signals
    fn decide(&self, signals: &[Signal]) -> Option<TradeAction> {
        if signals.is_empty() {
            return None;
        }

        // Calculate weighted direction
        let mut long_weight = 0.0;
        let mut short_weight = 0.0;
        let mut long_count = 0;
        let mut short_count = 0;

        for signal in signals {
            match signal.direction {
                Direction::Long => {
                    long_weight += signal.confidence;
                    long_count += 1;
                }
                Direction::Short => {
                    short_weight += signal.confidence;
                    short_count += 1;
                }
                Direction::Neutral => {}
            }
        }

        // Need at least one directional signal
        if long_count == 0 && short_count == 0 {
            return None;
        }

        // Direction and confidence based on winning side only
        let (direction, confidence) = if long_weight > short_weight {
            (Direction::Long, long_weight / long_count as f64)
        } else if short_weight > long_weight {
            (Direction::Short, short_weight / short_count as f64)
        } else {
            return None;
        };

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

        match self.config.mode {
            TradingMode::DryRun => {
                println!("  [DRY RUN] Would execute: {} {} sats", side, action.position_sats);
            }

            TradingMode::Paper => {
                // Get current price for paper trade
                let price = match self.get_current_price().await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("  [PAPER] Failed to get price: {}", e);
                        return;
                    }
                };

                // Record paper trade
                let mut state = self.paper_state.write().await;
                let trade_id = state.next_id;
                state.next_id += 1;

                let trade = PaperTrade {
                    id: trade_id,
                    direction: action.direction,
                    size_sats: action.position_sats,
                    entry_price: price,
                    entry_time: Utc::now(),
                    confidence: action.confidence,
                    closed: false,
                    exit_price: None,
                    exit_time: None,
                    pnl_sats: None,
                };

                println!(
                    "  \x1b[36m[PAPER OPEN]\x1b[0m #{} {} {} sats @ ${:.0}",
                    trade_id,
                    side.to_uppercase(),
                    action.position_sats,
                    price,
                );

                state.trades.push(trade);
            }

            TradingMode::Live => {
                // Execute actual trade
                if let Some(client) = &self.client {
                    match self.place_order(client, &action).await {
                        Ok(order_id) => {
                            println!("  \x1b[32m[LIVE] Order placed: {}\x1b[0m", order_id);
                        }
                        Err(e) => {
                            eprintln!("  \x1b[31m[LIVE] Order failed: {}\x1b[0m", e);
                        }
                    }
                } else {
                    eprintln!("  \x1b[31m[LIVE] No client configured!\x1b[0m");
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
