//! Daemon mode for continuous agent-based trading
//!
//! Runs agents in a loop, combines signals, and optionally executes trades.

use std::collections::BTreeMap;

use crate::agents::{
    flow::FlowAgent,
    llm::{LlmArbiter, LlmAction, LlmDecision, MarketSnapshot, PositionBrief},
    macro_cal::MacroAgent,
    news::NewsAgent,
    pattern::PatternAgent,
    whale::WhaleAgent,
    DataCollector, Direction,
};
use crate::api::LnmClient;
use crate::stats::save_trade_id;
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

/// Daemon configuration.
///
/// Decision-shaping knobs (min confidence, conflict / reversal / ATR thresholds)
/// are gone — the LLM arbiter handles all of those contextually now. What
/// remains is the operational envelope: how often to think, what mode to run
/// in, hard exit thresholds, position sizing ceiling.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub interval_secs: u64,
    pub mode: TradingMode,
    pub max_position_usd: u64,
    pub leverage: u32,
    pub take_profit_pct: Option<f64>,
    pub stop_loss_pct: Option<f64>,
    pub trailing_stop_pct: Option<f64>,
    pub agents: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            mode: TradingMode::DryRun,
            max_position_usd: 10,
            leverage: 10,
            take_profit_pct: Some(10.0),
            stop_loss_pct: Some(5.0),
            trailing_stop_pct: Some(3.0),
            agents: vec!["pattern".to_string(), "flow".to_string()],
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

/// Trading daemon. The arbiter is the sole decision maker; collectors only
/// supply the data it reads.
pub struct Daemon {
    config: DaemonConfig,
    collectors: Vec<Box<dyn DataCollector>>,
    arbiter: LlmArbiter,
    client: Option<LnmClient>,
    paper_state: RwLock<PaperState>,
    /// Peak ROE achieved in current position (for trailing stop).
    peak_roe: RwLock<Option<f64>>,
}

impl Daemon {
    /// Build a daemon. Returns Err if the LLM arbiter can't be initialized
    /// (e.g. ANTHROPIC_API_KEY is missing) — the daemon can't run without it.
    pub fn new(config: DaemonConfig, client: Option<LnmClient>) -> Result<Self> {
        let arbiter = LlmArbiter::from_env()?;
        let mut collectors: Vec<Box<dyn DataCollector>> = Vec::new();
        for agent_name in &config.agents {
            match agent_name.as_str() {
                "pattern" => collectors.push(Box::new(PatternAgent::with_defaults())),
                "macro" => collectors.push(Box::new(MacroAgent::with_defaults())),
                "news" => collectors.push(Box::new(NewsAgent::with_defaults())),
                "flow" => collectors.push(Box::new(FlowAgent::with_defaults())),
                "whale" => collectors.push(Box::new(WhaleAgent::with_defaults())),
                other => eprintln!("Unknown collector: {}", other),
            }
        }

        Ok(Self {
            config,
            collectors,
            arbiter,
            client,
            paper_state: RwLock::new(PaperState {
                trades: Vec::new(),
                next_id: 1,
                total_pnl: 0,
                wins: 0,
                losses: 0,
            }),
            peak_roe: RwLock::new(None),
        })
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

        // Calculate position-specific margin (not total account margin)
        // For inverse perpetual: margin = (quantity / leverage) / price * 100_000_000 sats
        let position_margin = if entry_price > 0.0 && leverage > 0.0 {
            (quantity.abs() / leverage) / entry_price * 100_000_000.0
        } else {
            margin // fallback to account margin
        };

        // P&L percentage relative to position margin (not account margin)
        let pl_pct = if position_margin > 0.0 { (pl / position_margin) * 100.0 } else { 0.0 };

        Some(CrossPosition {
            side,
            quantity: quantity.abs(),
            entry_price,
            margin: position_margin,  // Position margin in sats
            pl,
            pl_pct,
        })
    }

    /// Close cross margin position by placing opposite order
    /// This ensures we get an order ID for stats tracking
    async fn close_cross_position(&self, reason: &str) -> Result<()> {
        use reqwest::Method;

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No client configured"))?;

        // Get current position to know size and direction
        let position = self.get_cross_position().await
            .ok_or_else(|| anyhow::anyhow!("No position to close"))?;

        // Place opposite order to close
        let close_side = match position.side {
            Direction::Long => "sell",   // Sell to close long
            Direction::Short => "buy",   // Buy to close short
            Direction::Neutral => return Ok(()),
        };

        let request = serde_json::json!({
            "side": close_side,
            "type": "market",
            "quantity": position.quantity as u64,
        });

        let response: serde_json::Value = client
            .request(Method::POST, "futures/cross/order", Some(&request))
            .await?;

        // Save the closing order ID for stats
        let order_id = response["id"].as_str().unwrap_or("unknown");
        if let Err(e) = save_trade_id(order_id) {
            eprintln!("  Warning: Could not save close order ID: {}", e);
        }

        println!("  \x1b[33m[CLOSE]\x1b[0m {} - Order: {}", reason, order_id);
        Ok(())
    }

    /// Check TP/SL and close if triggered (based on Net ROE after fees)
    async fn check_tp_sl(&self) -> bool {
        if self.config.mode != TradingMode::Live {
            return false;
        }

        let position = match self.get_cross_position().await {
            Some(p) => p,
            None => {
                // No position - reset peak ROE (position may have been closed externally)
                *self.peak_roe.write().await = None;
                return false;
            }
        };

        // Calculate Net ROE (after estimated fees)
        // Fee = 0.1% of margin for open + 0.1% for close = 0.2% total
        let est_fees = position.margin * 0.002;
        let net_pl = position.pl - est_fees;
        let net_roe = if position.margin > 0.0 { (net_pl / position.margin) * 100.0 } else { 0.0 };

        // Check take profit (based on net ROE)
        if let Some(tp_pct) = self.config.take_profit_pct {
            if net_roe >= tp_pct {
                let reason = format!(
                    "Take profit triggered (Net ROE {:+.2}% >= +{:.1}%)",
                    net_roe, tp_pct
                );
                match self.close_cross_position(&reason).await {
                    Ok(_) => {
                        // Reset peak ROE on position close
                        *self.peak_roe.write().await = None;
                        return true;
                    }
                    Err(e) => eprintln!("  \x1b[31m[ERROR]\x1b[0m Failed to close: {}", e),
                }
            }
        }

        // Check stop loss (based on net ROE)
        if let Some(sl_pct) = self.config.stop_loss_pct {
            if net_roe <= -sl_pct {
                let reason = format!(
                    "Stop loss triggered (Net ROE {:+.2}% <= -{:.1}%)",
                    net_roe, sl_pct
                );
                match self.close_cross_position(&reason).await {
                    Ok(_) => {
                        // Reset peak ROE on position close
                        *self.peak_roe.write().await = None;
                        return true;
                    }
                    Err(e) => eprintln!("  \x1b[31m[ERROR]\x1b[0m Failed to close: {}", e),
                }
            }
        }

        // Trailing stop logic - only activates when in profit
        if let Some(trail_pct) = self.config.trailing_stop_pct {
            let mut peak = self.peak_roe.write().await;

            // Update peak ROE if we're in profit and current ROE is higher
            if net_roe > 0.0 {
                match *peak {
                    Some(p) if net_roe > p => *peak = Some(net_roe),
                    None => *peak = Some(net_roe),
                    _ => {}
                }
            }

            // Check if ROE dropped too far from peak
            if let Some(peak_val) = *peak {
                let trail_threshold = peak_val - trail_pct;
                if net_roe <= trail_threshold && peak_val >= trail_pct {
                    // Only trigger if we had meaningful gains (peak >= trail%)
                    let reason = format!(
                        "Trailing stop triggered (Net ROE {:+.2}% dropped from peak {:+.2}%, trail: {:.1}%)",
                        net_roe, peak_val, trail_pct
                    );
                    drop(peak); // Release lock before async call
                    match self.close_cross_position(&reason).await {
                        Ok(_) => {
                            *self.peak_roe.write().await = None;
                            return true;
                        }
                        Err(e) => eprintln!("  \x1b[31m[ERROR]\x1b[0m Failed to close: {}", e),
                    }
                }
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
        println!("  Arbiter: {} (Claude)", self.arbiter.model_name());
        println!("  Max position: ${} USD", self.config.max_position_usd);
        println!("  Leverage: {}x", self.config.leverage);
        if let Some(tp) = self.config.take_profit_pct {
            println!("  Take profit: +{:.1}%", tp);
        }
        if let Some(sl) = self.config.stop_loss_pct {
            println!("  Stop loss: -{:.1}%", sl);
        }
        if let Some(trail) = self.config.trailing_stop_pct {
            println!("  Trailing stop: {:.1}% from peak", trail);
        }
        println!(
            "  Collectors: {:?}",
            self.collectors.iter().map(|c| c.name()).collect::<Vec<_>>()
        );
        println!();

        // Set cross margin leverage at startup (Live mode only)
        if self.config.mode == TradingMode::Live {
            if let Some(ref client) = self.client {
                use reqwest::Method;
                let request = serde_json::json!({ "leverage": self.config.leverage });
                match client.request::<serde_json::Value, _>(Method::PUT, "futures/cross/leverage", Some(&request)).await {
                    Ok(_) => println!("  Cross leverage set to {}x", self.config.leverage),
                    Err(e) => eprintln!("  \x1b[33m[WARN]\x1b[0m Failed to set cross leverage: {}", e),
                }
                println!();
            }
        }

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
                    // Estimate fees: 0.1% of margin for open + 0.1% for close = 0.2% total
                    let est_fees = (pos.margin * 0.002) as i64;
                    let net_pl = pos.pl as i64 - est_fees;
                    // Net ROE = net P&L / margin
                    let net_roe = if pos.margin > 0.0 { (net_pl as f64 / pos.margin) * 100.0 } else { 0.0 };
                    let roe_color = if net_roe >= 0.0 { "\x1b[32m" } else { "\x1b[31m" };
                    let tp = self.config.take_profit_pct.unwrap_or(5.0);
                    let sl = self.config.stop_loss_pct.unwrap_or(3.0);

                    // Build trailing stop info if enabled
                    let trail_info = if let Some(trail_pct) = self.config.trailing_stop_pct {
                        let peak = self.peak_roe.read().await;
                        match *peak {
                            Some(p) => format!(" | Trail: {:.1}% from peak {:.1}%", trail_pct, p),
                            None => format!(" | Trail: {:.1}% (no peak yet)", trail_pct),
                        }
                    } else {
                        String::new()
                    };

                    println!(
                        "  \x1b[36m[POSITION]\x1b[0m {} ${:.0} @ ${:.0} | Net ROE: {}{:+.2}%\x1b[0m (TP: +{:.0}% / SL: -{:.0}%){} | Net P&L: {:+} sats (fees: ~{} sats)",
                        side_icon, pos.quantity, pos.entry_price, roe_color, net_roe, tp, sl, trail_info, net_pl, est_fees
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

            // Collect raw observations from every enabled collector (in parallel
            // would be ideal — for v1 do them sequentially, the cost is bounded
            // by network IO not CPU).
            let mut agent_data: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for c in &self.collectors {
                match c.collect().await {
                    Ok(v) => {
                        agent_data.insert(c.name().to_string(), v);
                    }
                    Err(e) => {
                        eprintln!("  [{}] collector failed: {}", c.name(), e);
                    }
                }
            }

            let current_position = if self.config.mode == TradingMode::Live {
                self.get_cross_position().await
            } else {
                None
            };

            if let Some(action) = self.decide_via_llm(&agent_data, current_position.as_ref()).await
            {
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

    /// Send the snapshot to Claude, return a TradeAction (or None to skip).
    /// On API failure, warn + skip — never trade blind.
    async fn decide_via_llm(
        &self,
        agent_data: &BTreeMap<String, serde_json::Value>,
        current_position: Option<&CrossPosition>,
    ) -> Option<TradeAction> {
        let price = match self.get_current_price().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  \x1b[33m[LLM]\x1b[0m no price available: {}", e);
                return None;
            }
        };

        let position_brief = current_position.map(|p| PositionBrief {
            side: p.side,
            size_usd: p.quantity,
            entry_price: p.entry_price,
            pl_pct: p.pl_pct,
        });

        let snapshot = MarketSnapshot {
            price,
            agent_data,
            current_position: position_brief.as_ref(),
            max_position_usd: self.config.max_position_usd,
            leverage: self.config.leverage,
            mode: match self.config.mode {
                TradingMode::Live => "live",
                TradingMode::Paper => "paper",
                TradingMode::DryRun => "dry_run",
            },
        };

        let decision: LlmDecision = match self.arbiter.decide(&snapshot).await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("  \x1b[33m[LLM]\x1b[0m skip cycle: {}", e);
                return None;
            }
        };

        let action_label = match decision.action {
            LlmAction::OpenLong => "OPEN_LONG",
            LlmAction::OpenShort => "OPEN_SHORT",
            LlmAction::Close => "CLOSE",
            LlmAction::Hold => "HOLD",
        };
        println!(
            "  \x1b[35m[LLM]\x1b[0m {} ({:.0}%, size {:.0}%): {}",
            action_label,
            decision.confidence * 100.0,
            decision.position_pct * 100.0,
            decision.reasoning
        );

        // Explicit close: hit the close path directly, don't open anything.
        if decision.action == LlmAction::Close {
            if current_position.is_some() && self.config.mode == TradingMode::Live {
                let _ = self.close_cross_position("LLM close").await;
            }
            return None;
        }

        let direction = decision.direction();
        if direction == Direction::Neutral {
            return None;
        }

        let position_usd =
            ((self.config.max_position_usd as f64 * decision.position_pct) as u64).max(1);
        Some(TradeAction {
            direction,
            confidence: decision.confidence,
            position_usd,
            reasons: vec![decision.reasoning],
        })
    }

    /// Execute a trading action
    async fn execute_action(&self, action: TradeAction, current_position: Option<&CrossPosition>) {
        let side = match action.direction {
            Direction::Long => "buy",
            Direction::Short => "sell",
            Direction::Neutral => return,
        };

        // Position vs new action handling. No more reversal premium / cooldown —
        // the LLM is responsible for deciding when to reverse.
        if let Some(pos) = current_position {
            let is_reversal = (pos.side == Direction::Long && action.direction == Direction::Short)
                || (pos.side == Direction::Short && action.direction == Direction::Long);
            if is_reversal {
                println!(
                    "  \x1b[33m→ REVERSAL: {} → {}\x1b[0m",
                    pos.side, action.direction
                );
                let _ = self.close_cross_position("LLM reversal").await;
            } else if pos.side == action.direction {
                println!("  → Already {} - skipping", pos.side);
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
            }

            TradingMode::Live => {
                // Execute actual trade
                if let Some(client) = &self.client {
                    match self.place_order(client, &action).await {
                        Ok(order_id) => {
                            println!("  \x1b[32m[LIVE] Order placed: {}\x1b[0m", order_id);

                            // Save trade ID for stats tracking
                            if let Err(e) = save_trade_id(&order_id) {
                                eprintln!("  Warning: Could not save trade ID: {}", e);
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
