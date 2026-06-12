//! Daemon mode for continuous agent-based trading
//!
//! Runs data collectors in a loop, feeds the LLM arbiter, and optionally executes trades.

use std::collections::{BTreeMap, VecDeque};

use crate::collectors::{
    flow::FlowAgent,
    macro_cal::MacroAgent,
    news::NewsAgent,
    pattern::PatternAgent,
    whale::WhaleAgent,
    DataCollector, Direction,
};
use crate::llm::{LlmArbiter, LlmAction, LlmDecision, MarketSnapshot, PositionBrief};
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
    /// Circuit breaker: stop opening new positions once net realized losses
    /// in the current UTC day reach this many sats. Closes are always allowed.
    pub max_daily_loss_sats: Option<u64>,
    pub collectors: Vec<String>,
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
            max_daily_loss_sats: None,
            collectors: vec!["pattern".to_string(), "flow".to_string()],
        }
    }
}

/// Taker fee per side (0.1%). Charged on notional (quantity), not margin.
const FEE_RATE: f64 = 0.001;

const SATS_PER_BTC: f64 = 100_000_000.0;

/// LN Markets inverse perpetual P&L in sats:
/// P&L = ±quantity × (1/exit − 1/entry) × 1e8 (negated for longs).
fn inverse_pl_sats(side: Direction, quantity: f64, entry_price: f64, exit_price: f64) -> f64 {
    let inv_diff = (1.0 / exit_price) - (1.0 / entry_price);
    match side {
        Direction::Long => -quantity * inv_diff * SATS_PER_BTC,
        Direction::Short => quantity * inv_diff * SATS_PER_BTC,
        Direction::Neutral => 0.0,
    }
}

/// Estimated open + close fees in sats. Fees apply to the notional at each
/// fill price — sizing them off margin would understate them by ~leverage×.
fn estimated_fees_sats(quantity: f64, entry_price: f64, exit_price: f64) -> f64 {
    (quantity / entry_price + quantity / exit_price) * SATS_PER_BTC * FEE_RATE
}

/// Append one JSONL line to the daemon journal next to the trade-ID store
/// (e.g. ~/.config/lnmarkets/daemon_journal.jsonl).
fn append_journal_line(entry: &serde_json::Value) -> Result<()> {
    use std::io::Write;

    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?
        .join("lnmarkets");
    std::fs::create_dir_all(&dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("daemon_journal.jsonl"))?;
    writeln!(file, "{}", entry)?;
    Ok(())
}

/// Snapshot of the current position — real (live) or simulated (paper).
#[derive(Debug, Clone)]
struct CrossPosition {
    side: Direction,
    /// Notional in USD.
    quantity: f64,
    entry_price: f64,
    /// Position margin in sats.
    margin: f64,
    /// Unrealized P&L in sats, before fees.
    pl: f64,
    /// Gross ROE % (pl / margin).
    pl_pct: f64,
    /// Estimated open + close fees in sats.
    est_fees: f64,
}

/// Paper trade record
#[derive(Debug, Clone)]
struct PaperTrade {
    id: u64,
    direction: Direction,
    quantity_usd: f64,
    entry_price: f64,
    margin_sats: f64,
    entry_time: DateTime<Utc>,
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
    /// Shared HTTP client for public price endpoints (Binance fallback).
    http: reqwest::Client,
    paper_state: RwLock<PaperState>,
    /// Peak ROE achieved in current position (for trailing stop).
    peak_roe: RwLock<Option<f64>>,
    /// Ring buffer of recent decisions/trades, rendered into the LLM prompt
    /// so the arbiter sees what it did and how it turned out.
    history: RwLock<VecDeque<(DateTime<Utc>, String)>>,
    /// (UTC day, net realized P&L in sats) for the daily-loss circuit breaker.
    daily_pnl: RwLock<(chrono::NaiveDate, i64)>,
}

/// How many history entries the LLM sees.
const HISTORY_CAP: usize = 20;

impl Daemon {
    /// Build a daemon. Returns Err if the LLM arbiter can't be initialized
    /// (e.g. ANTHROPIC_API_KEY is missing) — the daemon can't run without it.
    pub fn new(config: DaemonConfig, client: Option<LnmClient>) -> Result<Self> {
        let arbiter = LlmArbiter::from_env()?;
        let mut collectors: Vec<Box<dyn DataCollector>> = Vec::new();
        for name in &config.collectors {
            match name.as_str() {
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
            http: crate::collectors::http_client(),
            paper_state: RwLock::new(PaperState {
                trades: Vec::new(),
                next_id: 1,
                total_pnl: 0,
                wins: 0,
                losses: 0,
            }),
            peak_roe: RwLock::new(None),
            history: RwLock::new(VecDeque::new()),
            daily_pnl: RwLock::new((Utc::now().date_naive(), 0)),
        })
    }

    /// Append an entry to the decision/trade history shown to the LLM.
    async fn log_event(&self, text: String) {
        let mut h = self.history.write().await;
        h.push_back((Utc::now(), text));
        while h.len() > HISTORY_CAP {
            h.pop_front();
        }
    }

    /// Render history as relative-time lines, oldest first.
    async fn history_lines(&self) -> Vec<String> {
        let now = Utc::now();
        self.history
            .read()
            .await
            .iter()
            .map(|(t, s)| format!("{}m ago: {}", (now - *t).num_minutes(), s))
            .collect()
    }

    /// Append a JSONL entry to the daemon journal (best-effort — journal
    /// failures must never affect trading).
    fn journal(&self, mut entry: serde_json::Value) {
        if let Some(obj) = entry.as_object_mut() {
            obj.insert("ts".to_string(), serde_json::json!(Utc::now().to_rfc3339()));
            let mode = match self.config.mode {
                TradingMode::Live => "live",
                TradingMode::Paper => "paper",
                TradingMode::DryRun => "dry_run",
            };
            obj.insert("mode".to_string(), serde_json::json!(mode));
        }
        if let Err(e) = append_journal_line(&entry) {
            eprintln!("  [journal] write failed: {}", e);
        }
    }

    /// Track realized P&L for the daily-loss circuit breaker.
    async fn record_realized(&self, net_pl_sats: f64) {
        let today = Utc::now().date_naive();
        let mut state = self.daily_pnl.write().await;
        if state.0 != today {
            *state = (today, 0);
        }
        state.1 += net_pl_sats.round() as i64;
    }

    /// True when the daily-loss breaker forbids opening new positions.
    async fn breaker_tripped(&self) -> bool {
        let Some(limit) = self.config.max_daily_loss_sats else {
            return false;
        };
        let (date, pnl) = *self.daily_pnl.read().await;
        date == Utc::now().date_naive() && pnl <= -(limit as i64)
    }

    /// Fetch current BTC price from Binance
    async fn get_current_price(&self) -> Result<f64> {
        let url = "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT";
        let resp: serde_json::Value = self.http.get(url).send().await?.json().await?;
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

    /// Bid/ask for marking and filling positions. Prefers the LN Markets
    /// ticker (the actual execution venue); falls back to Binance spot when
    /// no client is configured (e.g. paper mode without credentials).
    async fn exit_prices(&self) -> Result<(f64, f64)> {
        if self.client.is_some() {
            if let Ok(prices) = self.get_lnm_prices().await {
                return Ok(prices);
            }
        }
        let price = self.get_current_price().await?;
        Ok((price, price))
    }

    /// Get current cross margin position. Returns Err on API failure —
    /// callers must never treat a failed fetch as "no position", or the
    /// daemon could open a duplicate position it can't see.
    async fn get_cross_position(&self) -> Result<Option<CrossPosition>> {
        use reqwest::Method;

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No client configured"))?;
        let resp: serde_json::Value = client
            .request(Method::GET, "futures/cross/position", None::<&()>)
            .await?;

        // Parse position - returns null or empty if no position
        let quantity = resp["quantity"].as_f64().unwrap_or(0.0);
        if quantity == 0.0 {
            return Ok(None);
        }

        let side = if quantity > 0.0 { Direction::Long } else { Direction::Short };
        let quantity = quantity.abs();
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

        let pl = inverse_pl_sats(side, quantity, entry_price, exit_price);
        let est_fees = estimated_fees_sats(quantity, entry_price, exit_price);

        // Calculate position-specific margin (not total account margin)
        // For inverse perpetual: margin = (quantity / leverage) / price * 100_000_000 sats
        let position_margin = if entry_price > 0.0 && leverage > 0.0 {
            (quantity / leverage) / entry_price * SATS_PER_BTC
        } else {
            margin // fallback to account margin
        };

        // P&L percentage relative to position margin (not account margin)
        let pl_pct = if position_margin > 0.0 { (pl / position_margin) * 100.0 } else { 0.0 };

        Ok(Some(CrossPosition {
            side,
            quantity,
            entry_price,
            margin: position_margin,  // Position margin in sats
            pl,
            pl_pct,
            est_fees,
        }))
    }

    /// One position snapshot per cycle. Live reads the exchange; paper
    /// synthesizes the same shape from the simulated trade. Errors propagate
    /// so an API failure skips the cycle instead of reading as "flat".
    async fn position_snapshot(&self) -> Result<Option<CrossPosition>> {
        match self.config.mode {
            TradingMode::Live => self.get_cross_position().await,
            TradingMode::Paper => self.paper_position().await,
            TradingMode::DryRun => Ok(None),
        }
    }

    /// Synthesize a CrossPosition from the open paper trade so paper mode
    /// flows through the same decision and TP/SL paths as live.
    async fn paper_position(&self) -> Result<Option<CrossPosition>> {
        let trade = {
            let state = self.paper_state.read().await;
            state.trades.iter().find(|t| !t.closed).cloned()
        };
        let Some(trade) = trade else {
            return Ok(None);
        };

        let (bid, ask) = self.exit_prices().await?;
        let exit_price = match trade.direction {
            Direction::Long => bid,
            Direction::Short => ask,
            Direction::Neutral => trade.entry_price,
        };
        let pl = inverse_pl_sats(trade.direction, trade.quantity_usd, trade.entry_price, exit_price);
        let est_fees = estimated_fees_sats(trade.quantity_usd, trade.entry_price, exit_price);
        let pl_pct = if trade.margin_sats > 0.0 { (pl / trade.margin_sats) * 100.0 } else { 0.0 };

        Ok(Some(CrossPosition {
            side: trade.direction,
            quantity: trade.quantity_usd,
            entry_price: trade.entry_price,
            margin: trade.margin_sats,
            pl,
            pl_pct,
            est_fees,
        }))
    }

    /// Close cross margin position by placing opposite order
    /// This ensures we get an order ID for stats tracking
    async fn close_cross_position(&self, reason: &str) -> Result<()> {
        use reqwest::Method;

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No client configured"))?;

        // Get current position to know size and direction
        let position = self.get_cross_position().await?
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
            "quantity": position.quantity.round() as u64,
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

    /// Close the current position in whatever mode the daemon runs in.
    /// Records realized P&L (from the snapshot) and logs to history/journal.
    async fn close_position(&self, reason: &str, pos: &CrossPosition) -> Result<()> {
        match self.config.mode {
            TradingMode::Live => self.close_cross_position(reason).await?,
            TradingMode::Paper => self.close_paper_position(reason).await?,
            TradingMode::DryRun => {
                println!("  [DRY RUN] Would close: {}", reason);
                return Ok(());
            }
        }

        let net_pl = pos.pl - pos.est_fees;
        self.record_realized(net_pl).await;
        self.log_event(format!(
            "CLOSED {} ${:.0} ({:+.0} sats net): {}",
            pos.side, pos.quantity, net_pl, reason
        ))
        .await;
        self.journal(serde_json::json!({
            "type": "close",
            "side": pos.side.to_string(),
            "quantity_usd": pos.quantity,
            "entry_price": pos.entry_price,
            "net_pl_sats": net_pl,
            "reason": reason,
        }));
        Ok(())
    }

    /// Close the simulated paper position at current prices, net of fees.
    async fn close_paper_position(&self, reason: &str) -> Result<()> {
        let (bid, ask) = self.exit_prices().await?;
        let mut state = self.paper_state.write().await;
        let trade = state
            .trades
            .iter_mut()
            .find(|t| !t.closed)
            .ok_or_else(|| anyhow::anyhow!("No paper position to close"))?;

        let exit_price = match trade.direction {
            Direction::Long => bid,
            Direction::Short => ask,
            Direction::Neutral => trade.entry_price,
        };
        let gross = inverse_pl_sats(trade.direction, trade.quantity_usd, trade.entry_price, exit_price);
        let fees = estimated_fees_sats(trade.quantity_usd, trade.entry_price, exit_price);
        let pnl = (gross - fees).round() as i64;
        let hold_mins = (Utc::now() - trade.entry_time).num_minutes();

        trade.closed = true;
        trade.exit_price = Some(exit_price);
        trade.exit_time = Some(Utc::now());
        trade.pnl_sats = Some(pnl);
        let (id, entry_price) = (trade.id, trade.entry_price);

        state.total_pnl += pnl;
        if pnl > 0 {
            state.wins += 1;
        } else {
            state.losses += 1;
        }

        let pnl_color = if pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
        println!(
            "  \x1b[36m[PAPER CLOSE]\x1b[0m #{} @ ${:.0} → ${:.0} after {}m | P&L: {}{:+} sats\x1b[0m (incl. ~{:.0} sats fees) | {}",
            id, entry_price, exit_price, hold_mins, pnl_color, pnl, fees, reason,
        );
        Ok(())
    }

    /// Check TP/SL/trailing on the cycle's position snapshot and close if
    /// triggered (based on Net ROE after estimated fees). Live and paper.
    async fn check_tp_sl(&self, position: Option<&CrossPosition>) -> bool {
        let position = match position {
            Some(p) => p,
            None => {
                // No position - reset peak ROE (position may have been closed externally)
                *self.peak_roe.write().await = None;
                return false;
            }
        };

        let net_pl = position.pl - position.est_fees;
        let net_roe = if position.margin > 0.0 { (net_pl / position.margin) * 100.0 } else { 0.0 };

        // Check take profit (based on net ROE)
        if let Some(tp_pct) = self.config.take_profit_pct {
            if net_roe >= tp_pct {
                let reason = format!(
                    "Take profit triggered (Net ROE {:+.2}% >= +{:.1}%)",
                    net_roe, tp_pct
                );
                match self.close_position(&reason, position).await {
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
                match self.close_position(&reason, position).await {
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
                    match self.close_position(&reason, position).await {
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
        if let Some(limit) = self.config.max_daily_loss_sats {
            println!("  Daily loss breaker: {} sats", limit);
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
            println!("\x1b[36m  Paper trading simulates the live strategy: real bid/ask fills,\x1b[0m");
            println!("\x1b[36m  inverse-perp P&L net of fees, same TP/SL/trailing and LLM exits.\x1b[0m");
            println!();
        }

        let mut ticker = interval(Duration::from_secs(self.config.interval_secs));

        loop {
            ticker.tick().await;

            println!("[{}] Analyzing...", chrono::Utc::now().format("%H:%M:%S"));

            // One position snapshot per cycle (live: exchange, paper: simulated).
            // An API error skips the whole cycle — it must never read as "no
            // position", or we could double up a position we can't see.
            let current_position = match self.position_snapshot().await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!(
                        "  \x1b[33m[WARN]\x1b[0m position fetch failed: {} — skipping cycle",
                        e
                    );
                    println!();
                    continue;
                }
            };

            if let Some(ref pos) = current_position {
                self.print_position(pos).await;
            }

            // TP/SL/trailing applies to live and paper alike
            if self.check_tp_sl(current_position.as_ref()).await {
                println!();
                continue;
            }

            // Collect raw observations from every enabled collector,
            // concurrently — one slow source (news feeds, Hyperliquid) must
            // not eat the whole cycle.
            let mut collector_data: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            let results = futures_util::future::join_all(
                self.collectors
                    .iter()
                    .map(|c| async move { (c.name().to_string(), c.collect().await) }),
            )
            .await;
            for (name, result) in results {
                match result {
                    Ok(v) => {
                        collector_data.insert(name, v);
                    }
                    Err(e) => {
                        eprintln!("  [{}] collector failed: {}", name, e);
                    }
                }
            }

            if let Some(action) = self.decide_via_llm(&collector_data, current_position.as_ref()).await
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

    /// Print the position line with net ROE after estimated fees.
    /// Works on live and paper snapshots alike.
    async fn print_position(&self, pos: &CrossPosition) {
        let side_icon = if pos.side == Direction::Long { "▲" } else { "▼" };
        let net_pl = (pos.pl - pos.est_fees) as i64;
        let net_roe = if pos.margin > 0.0 {
            ((pos.pl - pos.est_fees) / pos.margin) * 100.0
        } else {
            0.0
        };
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

        let label = if self.config.mode == TradingMode::Paper {
            "PAPER POSITION"
        } else {
            "POSITION"
        };
        println!(
            "  \x1b[36m[{}]\x1b[0m {} ${:.0} @ ${:.0} | Net ROE: {}{:+.2}%\x1b[0m (TP: +{:.0}% / SL: -{:.0}%){} | Net P&L: {:+} sats (fees: ~{:.0} sats)",
            label, side_icon, pos.quantity, pos.entry_price, roe_color, net_roe, tp, sl, trail_info, net_pl, pos.est_fees
        );
    }

    /// Print paper trading statistics
    async fn print_paper_stats(&self) {
        let (open_trade, closed_pnl, wins, losses) = {
            let state = self.paper_state.read().await;
            if state.trades.is_empty() {
                return;
            }
            (
                state.trades.iter().find(|t| !t.closed).cloned(),
                state.total_pnl,
                state.wins,
                state.losses,
            )
        };

        // Unrealized P&L for the open position, net of estimated fees
        let mut unrealized_pnl: i64 = 0;
        let open_count = if let Some(trade) = open_trade {
            if let Ok((bid, ask)) = self.exit_prices().await {
                let exit_price = match trade.direction {
                    Direction::Long => bid,
                    Direction::Short => ask,
                    Direction::Neutral => trade.entry_price,
                };
                let gross =
                    inverse_pl_sats(trade.direction, trade.quantity_usd, trade.entry_price, exit_price);
                let fees =
                    estimated_fees_sats(trade.quantity_usd, trade.entry_price, exit_price);
                unrealized_pnl = (gross - fees).round() as i64;
            }
            1
        } else {
            0
        };

        let total_pnl = closed_pnl + unrealized_pnl;
        let pnl_color = if total_pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
        let unrealized_color = if unrealized_pnl >= 0 { "\x1b[32m" } else { "\x1b[31m" };
        let total = wins + losses;
        let win_rate = if total > 0 { wins as f64 / total as f64 * 100.0 } else { 0.0 };

        println!(
            "  \x1b[36m[PAPER]\x1b[0m Open: {} ({}{:+} sats net\x1b[0m) | Closed: {} | W/L: {}/{} ({:.0}%) | Total P&L: {}{:+} sats\x1b[0m",
            open_count,
            unrealized_color,
            unrealized_pnl,
            total,
            wins,
            losses,
            win_rate,
            pnl_color,
            total_pnl,
        );
    }

    /// Send the snapshot to Claude, return a TradeAction (or None to skip).
    /// On API failure, warn + skip — never trade blind.
    async fn decide_via_llm(
        &self,
        collector_data: &BTreeMap<String, serde_json::Value>,
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

        let history = self.history_lines().await;
        // Round-trip fees as % of margin: 2 × FEE_RATE on notional ≈
        // 2 × FEE_RATE × leverage on margin.
        let round_trip_fee_pct_of_margin =
            2.0 * FEE_RATE * 100.0 * self.config.leverage as f64;

        let snapshot = MarketSnapshot {
            price,
            collector_data,
            current_position: position_brief.as_ref(),
            max_position_usd: self.config.max_position_usd,
            leverage: self.config.leverage,
            mode: match self.config.mode {
                TradingMode::Live => "live",
                TradingMode::Paper => "paper",
                TradingMode::DryRun => "dry_run",
            },
            round_trip_fee_pct_of_margin,
            recent_history: &history,
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
        self.log_event(format!(
            "decided {} (conf {:.0}%, size {:.0}%) @ ${:.0}: {}",
            action_label,
            decision.confidence * 100.0,
            decision.position_pct * 100.0,
            price,
            decision.reasoning
        ))
        .await;
        self.journal(serde_json::json!({
            "type": "decision",
            "price": price,
            "action": action_label,
            "confidence": decision.confidence,
            "position_pct": decision.position_pct,
            "reasoning": decision.reasoning,
        }));

        // Explicit close: hit the close path directly, don't open anything.
        if decision.action == LlmAction::Close {
            if let Some(pos) = current_position {
                match self.close_position("LLM close", pos).await {
                    Ok(_) => *self.peak_roe.write().await = None,
                    Err(e) => eprintln!("  \x1b[31m[ERROR]\x1b[0m Failed to close: {}", e),
                }
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
                if let Err(e) = self.close_position("LLM reversal", pos).await {
                    eprintln!(
                        "  \x1b[31m[ERROR]\x1b[0m Reversal close failed: {} — not opening opposite position",
                        e
                    );
                    return;
                }
                // Fresh position next — the old trailing-stop peak must not
                // carry over or it can trip the trail immediately.
                *self.peak_roe.write().await = None;
            } else if pos.side == action.direction {
                println!("  → Already {} - skipping", pos.side);
                return;
            }
        }

        // Daily-loss circuit breaker: closes always go through (handled
        // above), but no new positions until UTC midnight.
        if self.breaker_tripped().await {
            let (_, pnl) = *self.daily_pnl.read().await;
            println!(
                "  \x1b[31m[BREAKER]\x1b[0m daily net loss {} sats hit the {} sat limit — no new positions until UTC midnight",
                pnl,
                self.config.max_daily_loss_sats.unwrap_or(0)
            );
            return;
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
                // Fill at the realistic price: longs lift the ask, shorts hit the bid.
                let (bid, ask) = match self.exit_prices().await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("  [PAPER] Failed to get price: {}", e);
                        return;
                    }
                };
                let entry_price = match action.direction {
                    Direction::Long => ask,
                    Direction::Short => bid,
                    Direction::Neutral => return,
                };
                let quantity_usd = action.position_usd as f64;
                let margin_sats =
                    (quantity_usd / self.config.leverage as f64) / entry_price * SATS_PER_BTC;

                // Record paper trade
                let mut state = self.paper_state.write().await;
                let trade_id = state.next_id;
                state.next_id += 1;

                let trade = PaperTrade {
                    id: trade_id,
                    direction: action.direction,
                    quantity_usd,
                    entry_price,
                    margin_sats,
                    entry_time: Utc::now(),
                    closed: false,
                    exit_price: None,
                    exit_time: None,
                    pnl_sats: None,
                };

                println!(
                    "  \x1b[36m[PAPER OPEN]\x1b[0m #{} {} ${} @ ${:.0} ({}x, margin {:.0} sats)",
                    trade_id,
                    side.to_uppercase(),
                    action.position_usd,
                    entry_price,
                    self.config.leverage,
                    margin_sats,
                );

                state.trades.push(trade);
                drop(state);

                self.log_event(format!(
                    "OPENED {} ${} @ ${:.0}",
                    action.direction, action.position_usd, entry_price
                ))
                .await;
                self.journal(serde_json::json!({
                    "type": "open",
                    "side": action.direction.to_string(),
                    "quantity_usd": action.position_usd,
                    "entry_price": entry_price,
                    "leverage": self.config.leverage,
                }));
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

                            self.log_event(format!(
                                "OPENED {} ${} (order {})",
                                action.direction, action.position_usd, order_id
                            ))
                            .await;
                            self.journal(serde_json::json!({
                                "type": "open",
                                "side": action.direction.to_string(),
                                "quantity_usd": action.position_usd,
                                "leverage": self.config.leverage,
                                "order_id": order_id,
                            }));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_pl_positive_when_price_rises() {
        // $100 from $50k to $55k: 100 × (1/50k − 1/55k) × 1e8 ≈ +18,182 sats
        let pl = inverse_pl_sats(Direction::Long, 100.0, 50_000.0, 55_000.0);
        assert!((pl - 18_181.8).abs() < 1.0, "pl = {}", pl);
        // Symmetric: the short loses the same amount
        let short = inverse_pl_sats(Direction::Short, 100.0, 50_000.0, 55_000.0);
        assert!((short + pl).abs() < 0.001);
    }

    #[test]
    fn short_pl_positive_when_price_falls() {
        let pl = inverse_pl_sats(Direction::Short, 100.0, 50_000.0, 45_000.0);
        assert!(pl > 0.0);
        assert_eq!(inverse_pl_sats(Direction::Neutral, 100.0, 50_000.0, 45_000.0), 0.0);
    }

    #[test]
    fn fees_scale_with_notional_not_margin() {
        // $100 notional at $50k = 200,000 sats per side; 0.1% × 2 sides = 400 sats.
        // (The old margin-based estimate at 10x leverage would have said ~40.)
        let fees = estimated_fees_sats(100.0, 50_000.0, 50_000.0);
        assert!((fees - 400.0).abs() < 1.0, "fees = {}", fees);
    }
}
