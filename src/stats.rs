//! Trading statistics - tracks daemon trade IDs
//!
//! Stores only trade IDs locally. Full trade data fetched from LN Markets API.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Get the path to the daemon trades file
fn trades_file_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?;
    Ok(config_dir.join("lnmarkets").join("daemon_trades.txt"))
}

/// Load daemon trade IDs from file
pub fn load_trade_ids() -> Result<HashSet<String>> {
    let path = trades_file_path()?;

    if !path.exists() {
        return Ok(HashSet::new());
    }

    let content = fs::read_to_string(&path)?;
    let ids: HashSet<String> = content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(ids)
}

/// Save a new trade ID
pub fn save_trade_id(trade_id: &str) -> Result<()> {
    let path = trades_file_path()?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Append to file
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    writeln!(file, "{}", trade_id)?;
    Ok(())
}

/// Trading statistics calculated from API data
#[derive(Debug, Default)]
pub struct TradingStats {
    pub total_trades: u32,
    pub open_trades: u32,
    pub closed_trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub win_rate: f64,
    pub total_pnl_sats: i64,
    pub best_trade_sats: i64,
    pub worst_trade_sats: i64,
    pub avg_pnl_sats: f64,
    pub current_streak: i32,
}

/// Trade info from API
#[derive(Debug, Clone)]
pub struct TradeInfo {
    pub id: String,
    pub side: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub pl: i64,
    pub closed: bool,
    pub creation_ts: i64,
    pub last_update_ts: i64,
}

/// Calculate stats from a list of trades
pub fn calculate_stats(trades: &[TradeInfo]) -> TradingStats {
    if trades.is_empty() {
        return TradingStats::default();
    }

    let mut stats = TradingStats::default();
    stats.total_trades = trades.len() as u32;

    let mut pnls: Vec<i64> = Vec::new();

    for trade in trades {
        if trade.closed {
            stats.closed_trades += 1;
            pnls.push(trade.pl);
            stats.total_pnl_sats += trade.pl;

            if trade.pl > 0 {
                stats.wins += 1;
            } else {
                stats.losses += 1;
            }

            if trade.pl > stats.best_trade_sats {
                stats.best_trade_sats = trade.pl;
            }
            if trade.pl < stats.worst_trade_sats || stats.worst_trade_sats == 0 {
                stats.worst_trade_sats = trade.pl;
            }
        } else {
            stats.open_trades += 1;
        }
    }

    if stats.closed_trades > 0 {
        stats.win_rate = stats.wins as f64 / stats.closed_trades as f64 * 100.0;
        stats.avg_pnl_sats = stats.total_pnl_sats as f64 / stats.closed_trades as f64;
    }

    // Calculate streak (from most recent trades)
    stats.current_streak = calculate_streak(&pnls);

    stats
}

/// Calculate current win/loss streak
fn calculate_streak(pnls: &[i64]) -> i32 {
    if pnls.is_empty() {
        return 0;
    }

    // Sort by most recent first (assuming pnls are in order)
    let first_positive = pnls.last().map(|&p| p > 0).unwrap_or(false);
    let mut streak = 0i32;

    for &pnl in pnls.iter().rev() {
        if (pnl > 0) == first_positive {
            streak += 1;
        } else {
            break;
        }
    }

    if first_positive { streak } else { -streak }
}

/// Format stats for display
pub fn format_stats(stats: &TradingStats) -> String {
    let streak_str = if stats.current_streak > 0 {
        format!("{}W 🔥", stats.current_streak)
    } else if stats.current_streak < 0 {
        format!("{}L 💀", -stats.current_streak)
    } else {
        "—".to_string()
    };

    let reset = "\x1b[0m";
    let pnl_color = if stats.total_pnl_sats >= 0 { "\x1b[32m" } else { "\x1b[31m" };

    format!(
"
Trading Stats (daemon trades only)
{}
Trades:      {} total ({} open, {} closed)
Win/Loss:    {} / {} ({:.1}%)
Total P&L:   {}{:+} sats{}
Best trade:  {:+} sats
Worst trade: {:+} sats
Avg P&L:     {:+.0} sats
Streak:      {}
",
        "─".repeat(35),
        stats.total_trades,
        stats.open_trades,
        stats.closed_trades,
        stats.wins,
        stats.losses,
        stats.win_rate,
        pnl_color,
        stats.total_pnl_sats,
        reset,
        stats.best_trade_sats,
        stats.worst_trade_sats,
        stats.avg_pnl_sats,
        streak_str,
    )
}
