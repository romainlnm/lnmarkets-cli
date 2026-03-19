//! Daemon mode for continuous agent-based trading
//!
//! Runs agents in a loop, combines signals, and optionally executes trades.

use crate::agents::{pattern::PatternAgent, macro_cal::MacroAgent, news::NewsAgent, flow::FlowAgent, Agent, AgentRegistry, Direction, Signal};
use crate::api::LnmClient;
use crate::stats::StatsDb;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Mutex;
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
    /// Maximum position size in USD
    pub max_position_usd: u64,
    /// Leverage (1-100)
    pub leverage: u32,
    /// Take profit percentage (e.g., 5.0 = 5%)
    pub take_profit_pct: Option<f64>,
    /// Stop loss percentage (e.g., 3.0 = 3%)
    pub stop_loss_pct: Option<f64>,
    /// Enabled agents
    pub agents: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            mode: TradingMode::DryRun,
            min_confidence: 0.7,
            max_position_usd: 10,
            leverage: 10,
            take_profit_pct: Some(5.0),
            stop_loss_pct: Some(3.0),
            agents: vec!["pattern".to_string()],
        }
    }
}

/// Cross margin position info
#[derive(Debug, Clone)]
struct CrossPosition {
    side: Direction,
    quantity: f64,
    entry_price: f64,
    margin: f64,
    pl: f64,
    pl_pct: f64,
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
    stats_db: Option<Mutex<StatsDb>>,
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

        // Initialize stats database (optional, don't fail if it errors)
        let stats_db = StatsDb::open()
            .map(|db| Mutex::new(db))
            .ok();

        if stats_db.is_none() {
            eprintln!("Warning: Could not open stats database, stats will not be recorded");
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
            stats_db,
        }
    }

    /// Fetch current BTC price from Binance (for agents)
    async fn get_current_price(&self) -> Result<f64> {
        let url = "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT";
        let client = reqwest::Client::new();
        let resp: serde_json::Value = client.get(url).send().await?.json().await?;
        let price = resp["price"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No price in response"))?
            .parse::<f64>()?;
        Ok(price)
    }

    /// Fetch bid/ask prices from LN Markets ticker
    async fn get_lnm_prices(&self) -> Result<(f64, f64)> {
        use crate::models::market::Ticker;
        use reqwest::Method;

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No client configured"))?;

        let ticker: Ticker = client
            .public_request(Method::GET, "futures/ticker")
            .await?;

        let (bid, ask) = ticker.prices.first()
            .map(|p| (p.bid_price, p.ask_price))
            .unwrap_or((ticker.index, ticker.index));

        Ok((bid, ask))
    }

    /// Get current cross margin position
    async fn get_cross_position(&self) -> Option<CrossPosition> {
        use reqwest::Method;

        let client = self.client.as_ref()?;
        let resp: serde_json::Value = client
            .request(Method::GET, "futures/cross/position", None::<&()>)
            .await
            .ok()?;

        // Parse position - returns null or empty if no position
        let quantity = resp["quantity"].as_f64().unwrap_or(0.0);
        if quantity == 0.0 {
            return None;
        }

        let side = if quantity > 0.0 { Direction::Long } else { Direction::Short };
        let entry_price = resp["entryPrice"].as_f64().unwrap_or(0.0);
        let margin = resp["margin"].as_f64().unwrap_or(0.0);
        let leverage = resp["leverage"].as_f64().unwrap_or(self.config.leverage as f64);

        // Calculate live P&L using LN Markets bid/ask (actual exit price)
        // Long closes at bid (sell), Short closes at ask (buy)
        let (bid, ask) = self.get_lnm_prices().await.unwrap_or((entry_price, entry_price));
        let exit_price = match side {
            Direction::Long => bid,   // Sell at bid to close long
            Direction::Short => ask,  // Buy at ask to close short
            Direction::Neutral => entry_price,
        };

        // LN Markets inverse perpetual P&L formula:
        // P&L (sats) = Quantity × (1/exit_price - 1/entry_price) × 100_000_000
        // For LONG: negate (profit when price goes UP)
        // For SHORT: as-is (profit when price goes DOWN)
        let inv_diff = (1.0 / exit_price) - (1.0 / entry_price);
        let pl = match side {
            Direction::Long => -quantity.abs() * inv_diff * 100_000_000.0,
            Direction::Short => quantity.abs() * inv_diff * 100_000_000.0,
            Direction::Neutral => 0.0,
        };

        // P&L percentage relative to margin
        let pl_pct = if margin > 0.0 { (pl / margin) * 100.0 } else { 0.0 };

        Some(CrossPosition {
            side,
            quantity: quantity.abs(),
            entry_price,
            margin,
            pl,
            pl_pct,
        })
    }

    /// Close cross margin position
    async fn close_cross_position(&self, reason: &str) -> Result<()> {
        use reqwest::Method;

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No client configured"))?;

        // Get exit price before closing
        let exit_price = self.get_current_price().await.unwrap_or(0.0);

        let _resp: serde_json::Value = client
            .request(Method::DELETE, "futures/cross/position", None::<&()>)
            .await?;

        println!("  \x1b[33m[CLOSE]\x1b[0m Position closed: {}", reason);

        // Close open trades in stats database
        let mode = match self.config.mode {
            TradingMode::Live => "live",
            TradingMode::Paper => "paper",
            TradingMode::DryRun => return Ok(()),
        };

        if let Some(db) = &self.stats_db {
            if let Ok(db) = db.lock() {
                let _ = db.close_all_open(exit_price, mode);
            }
        }

        Ok(())
    }

    /// Check TP/SL and close if triggered
    async fn check_tp_sl(&self) -> bool {
        if self.config.mode != TradingMode::Live {
            return false;
        }

        let position = match self.get_cross_position().await {
            Some(p) => p,
            None => return false,
        };

        // Check take profit
        if let Some(tp_pct) = self.config.take_profit_pct {
            if position.pl_pct >= tp_pct {
                let _ = self.close_cross_position(&format!(
                    "Take profit triggered ({:+.2}% >= +{:.1}%)",
                    position.pl_pct, tp_pct
                )).await;
                return true;
            }
        }

        // Check stop loss
        if let Some(sl_pct) = self.config.stop_loss_pct {
            if position.pl_pct <= -sl_pct {
                let _ = self.close_cross_position(&format!(
                    "Stop loss triggered ({:+.2}% <= -{:.1}%)",
                    position.pl_pct, sl_pct
                )).await;
                return true;
            }
        }

        false
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
        println!("  Max position: ${} USD", self.config.max_position_usd);
        println!("  Leverage: {}x", self.config.leverage);
        if let Some(tp) = self.config.take_profit_pct {
            println!("  Take profit: +{:.1}%", tp);
        }
        if let Some(sl) = self.config.stop_loss_pct {
            println!("  Stop loss: -{:.1}%", sl);
        }
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

            // In live mode, show position and check TP/SL
            if self.config.mode == TradingMode::Live {
                if let Some(pos) = self.get_cross_position().await {
                    let side_icon = if pos.side == Direction::Long { "▲" } else { "▼" };
                    let pl_color = if pos.pl >= 0.0 { "\x1b[32m" } else { "\x1b[31m" };
                    println!(
                        "  \x1b[36m[POSITION]\x1b[0m {} ${:.0} @ ${:.0} | P&L: {}{:+.0} sats ({:+.2}%)\x1b[0m",
                        side_icon, pos.quantity, pos.entry_price, pl_color, pos.pl, pos.pl_pct
                    );
                }

                // Check TP/SL
                if self.check_tp_sl().await {
                    println!();
                    continue;
                }
            }

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

            // Get current position for signal reversal check
            let current_position = if self.config.mode == TradingMode::Live {
                self.get_cross_position().await
            } else {
                None
            };

            // Combine signals and decide
            if let Some(action) = self.decide(&signals) {
                self.execute_action(action, current_position.as_ref()).await;
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

        // Calculate position size based on confidence (in USD)
        let size_factor = (confidence - self.config.min_confidence) / (1.0 - self.config.min_confidence);
        let position_usd = ((self.config.max_position_usd as f64 * size_factor * 0.5) as u64).max(1);

        Some(TradeAction {
            direction,
            confidence,
            position_usd,
            reasons: signals.iter().map(|s| s.reasoning.clone()).collect(),
        })
    }

    /// Execute a trading action
    async fn execute_action(&self, action: TradeAction, current_position: Option<&CrossPosition>) {
        let side = match action.direction {
            Direction::Long => "buy",
            Direction::Short => "sell",
            Direction::Neutral => return,
        };

        // Check for signal reversal (position is opposite to new signal)
        if let Some(pos) = current_position {
            let is_reversal = (pos.side == Direction::Long && action.direction == Direction::Short)
                || (pos.side == Direction::Short && action.direction == Direction::Long);

            if is_reversal {
                println!(
                    "  \x1b[33m→ REVERSAL: {} → {} ({:.0}% confidence)\x1b[0m",
                    pos.side, action.direction, action.confidence * 100.0
                );
                // Close current position first (cross margin will net out)
                let _ = self.close_cross_position("Signal reversal").await;
            } else if pos.side == action.direction {
                // Same direction - skip to avoid adding to position
                println!(
                    "  → Already {} - skipping (P&L: {:+.2}%)",
                    pos.side, pos.pl_pct
                );
                return;
            }
        }

        println!(
            "  \x1b[1m→ ACTION: {} ${} USD @ {}x ({:.0}% confidence)\x1b[0m",
            side.to_uppercase(),
            action.position_usd,
            self.config.leverage,
            action.confidence * 100.0
        );

        match self.config.mode {
            TradingMode::DryRun => {
                println!("  [DRY RUN] Would execute: {} ${} @ {}x", side, action.position_usd, self.config.leverage);
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

                // Convert USD position to sats for P&L tracking
                let size_sats = ((action.position_usd as f64 / price) * 100_000_000.0) as u64;

                // Record paper trade
                let mut state = self.paper_state.write().await;
                let trade_id = state.next_id;
                state.next_id += 1;

                let trade = PaperTrade {
                    id: trade_id,
                    direction: action.direction,
                    size_sats,
                    entry_price: price,
                    entry_time: Utc::now(),
                    confidence: action.confidence,
                    closed: false,
                    exit_price: None,
                    exit_time: None,
                    pnl_sats: None,
                };

                println!(
                    "  \x1b[36m[PAPER OPEN]\x1b[0m #{} {} ${} @ ${:.0}",
                    trade_id,
                    side.to_uppercase(),
                    action.position_usd,
                    price,
                );

                state.trades.push(trade);

                // Record in stats database
                if let Some(db) = &self.stats_db {
                    if let Ok(db) = db.lock() {
                        let dir = if action.direction == Direction::Long { "long" } else { "short" };
                        let agents = action.reasons.join("; ");
                        let _ = db.record_open(dir, action.position_usd as f64, price, action.confidence, &agents, "paper");
                    }
                }
            }

            TradingMode::Live => {
                // Execute actual trade
                if let Some(client) = &self.client {
                    // Get entry price before placing order
                    let entry_price = self.get_current_price().await.unwrap_or(0.0);

                    match self.place_order(client, &action).await {
                        Ok(order_id) => {
                            println!("  \x1b[32m[LIVE] Order placed: {}\x1b[0m", order_id);

                            // Record in stats database
                            if let Some(db) = &self.stats_db {
                                if let Ok(db) = db.lock() {
                                    let dir = if action.direction == Direction::Long { "long" } else { "short" };
                                    let agents = action.reasons.join("; ");
                                    let _ = db.record_open(dir, action.position_usd as f64, entry_price, action.confidence, &agents, "live");
                                }
                            }
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

    async fn place_order(&self, client: &LnmClient, action: &TradeAction) -> Result<String> {
        use reqwest::Method;

        let side = match action.direction {
            Direction::Long => "buy",
            Direction::Short => "sell",
            Direction::Neutral => return Err(anyhow::anyhow!("Cannot place neutral order")),
        };

        // Cross margin order - quantity is in USD
        let request = serde_json::json!({
            "side": side,
            "type": "market",
            "quantity": action.position_usd,
            "leverage": self.config.leverage
        });

        let response: serde_json::Value = client
            .request(Method::POST, "futures/cross/order", Some(&request))
            .await?;

        // Extract order ID from response
        let id = response["id"].as_str()
            .or_else(|| response["orderId"].as_str())
            .unwrap_or("unknown");

        Ok(id.to_string())
    }
}

/// A trading action to execute
#[derive(Debug)]
struct TradeAction {
    direction: Direction,
    confidence: f64,
    position_usd: u64,
    #[allow(dead_code)]
    reasons: Vec<String>,
}
