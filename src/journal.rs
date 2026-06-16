//! Offline evaluation of the daemon journal (`daemon_journal.jsonl`).
//!
//! The daemon appends one JSON line per decision, open, and close. This module
//! reads that log back and computes trade-level performance so you can judge
//! whether the strategy has an edge — independent of the daemon process.
//!
//! All P&L here comes from `close` events (the `net_pl_sats` field, already net
//! of estimated fees). Peak-capture (max favorable excursion) is reconstructed
//! from the `decision` prices recorded while each position was open, so it is
//! approximate — the exit price is taken as the last decision price before the
//! close. It's directional guidance, not exchange-accurate fills.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::path::PathBuf;

/// Default journal location: <config>/lnmarkets/daemon_journal.jsonl.
fn default_journal_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?
        .join("lnmarkets");
    Ok(dir.join("daemon_journal.jsonl"))
}

/// A completed round-trip reconstructed from an open/close pair.
struct Trade {
    side: String,
    net_pl_sats: f64,
    /// Hold duration in minutes (None if the open wasn't in the log).
    hold_mins: Option<i64>,
    /// Confidence on the opening decision, if captured.
    entry_confidence: Option<f64>,
    /// Max favorable price excursion as a fraction (e.g. 0.012 = +1.2%),
    /// approximate, from decision prices during the hold.
    peak_favorable: Option<f64>,
    /// Realized price move at exit as a fraction, side-adjusted, approximate.
    realized_move: Option<f64>,
    reason: String,
}

/// Open position being tracked while walking the log.
struct OpenState {
    side: String,
    entry_price: Option<f64>,
    entry_ts: Option<DateTime<Utc>>,
    entry_confidence: Option<f64>,
    peak_favorable: Option<f64>,
    last_price: Option<f64>,
}

fn ts_of(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    v.get("ts")
        .and_then(|t| t.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Side-adjusted favorable fraction of a price vs entry.
fn favorable(side: &str, entry: f64, price: f64) -> f64 {
    if side.eq_ignore_ascii_case("SHORT") {
        (entry - price) / entry
    } else {
        (price - entry) / entry
    }
}

pub fn run(file: Option<String>, mode_filter: Option<String>) -> Result<()> {
    let path = match file {
        Some(f) => PathBuf::from(f),
        None => default_journal_path()?,
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read journal at {}", path.display()))?;

    // Parse + sort by timestamp (restarts can interleave append order).
    let mut events: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| {
            mode_filter
                .as_ref()
                .map(|m| v.get("mode").and_then(|x| x.as_str()) == Some(m.as_str()))
                .unwrap_or(true)
        })
        .collect();
    events.sort_by_key(|v| ts_of(v));

    if events.is_empty() {
        println!("No journal events found at {}", path.display());
        return Ok(());
    }

    // Decision distribution + walk for trades.
    let (mut n_hold, mut n_long, mut n_short, mut n_close, mut n_decisions) = (0, 0, 0, 0, 0u64);
    let mut conf_sum = 0.0;
    let mut trades: Vec<Trade> = Vec::new();
    let mut open: Option<OpenState> = None;
    let first_ts = events.first().and_then(ts_of);
    let last_ts = events.last().and_then(ts_of);
    let mut first_price: Option<f64> = None;
    let mut last_price: Option<f64> = None;
    // The open is journaled the same cycle as its decision; carry that
    // decision's confidence onto the trade.
    let mut last_decision_conf: Option<f64> = None;

    for ev in &events {
        let kind = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "decision" => {
                n_decisions += 1;
                if let Some(c) = ev.get("confidence").and_then(|c| c.as_f64()) {
                    conf_sum += c;
                    last_decision_conf = Some(c);
                }
                match ev.get("action").and_then(|a| a.as_str()) {
                    Some("OPEN_LONG") => n_long += 1,
                    Some("OPEN_SHORT") => n_short += 1,
                    Some("CLOSE") => n_close += 1,
                    _ => n_hold += 1,
                }
                if let Some(p) = ev.get("price").and_then(|p| p.as_f64()) {
                    if first_price.is_none() {
                        first_price = Some(p);
                    }
                    last_price = Some(p);
                    // Update MFE on the open position.
                    if let Some(st) = open.as_mut() {
                        st.last_price = Some(p);
                        if let Some(entry) = st.entry_price {
                            let fav = favorable(&st.side, entry, p);
                            st.peak_favorable =
                                Some(st.peak_favorable.map_or(fav, |m: f64| m.max(fav)));
                        }
                    }
                }
            }
            "open" => {
                open = Some(OpenState {
                    side: ev.get("side").and_then(|s| s.as_str()).unwrap_or("?").to_string(),
                    entry_price: ev.get("entry_price").and_then(|p| p.as_f64()),
                    entry_ts: ts_of(ev),
                    entry_confidence: last_decision_conf,
                    peak_favorable: None,
                    last_price: None,
                });
            }
            "close" => {
                let net = ev.get("net_pl_sats").and_then(|p| p.as_f64()).unwrap_or(0.0);
                let side = ev
                    .get("side")
                    .and_then(|s| s.as_str())
                    .or(open.as_ref().map(|o| o.side.as_str()))
                    .unwrap_or("?")
                    .to_string();
                let reason = ev
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_string();
                let (hold_mins, peak, realized, conf) = match open.take() {
                    Some(st) => {
                        let hold = match (st.entry_ts, ts_of(ev)) {
                            (Some(a), Some(b)) => Some((b - a).num_minutes()),
                            _ => None,
                        };
                        let realized = match (st.entry_price, st.last_price) {
                            (Some(e), Some(x)) => Some(favorable(&side, e, x)),
                            _ => None,
                        };
                        (hold, st.peak_favorable, realized, st.entry_confidence)
                    }
                    None => (None, None, None, None),
                };
                trades.push(Trade {
                    side,
                    net_pl_sats: net,
                    hold_mins,
                    entry_confidence: conf,
                    peak_favorable: peak,
                    realized_move: realized,
                    reason,
                });
            }
            _ => {}
        }
    }

    print_report(
        &path, first_ts, last_ts, n_decisions, n_hold, n_long, n_short, n_close, conf_sum,
        first_price, last_price, &trades,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_report(
    path: &std::path::Path,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    n_decisions: u64,
    n_hold: u64,
    n_long: u64,
    n_short: u64,
    n_close: u64,
    conf_sum: f64,
    first_price: Option<f64>,
    last_price: Option<f64>,
    trades: &[Trade],
) {
    let span = match (first_ts, last_ts) {
        (Some(a), Some(b)) => {
            let h = (b - a).num_minutes() as f64 / 60.0;
            format!("{} → {}  ({:.1}h)", a.format("%Y-%m-%d %H:%M"), b.format("%Y-%m-%d %H:%M"), h)
        }
        _ => "unknown".to_string(),
    };

    println!("\n{}", "Daemon journal evaluation".bold());
    println!("  file:   {}", path.display());
    println!("  window: {}", span);
    println!();

    // ── Decisions ──
    println!("{}", "Decisions".bold());
    println!("  total cycles:   {}", n_decisions);
    let pct = |n: u64| if n_decisions > 0 { 100.0 * n as f64 / n_decisions as f64 } else { 0.0 };
    println!("  hold:           {} ({:.0}%)", n_hold, pct(n_hold));
    println!("  open_long:      {} ({:.0}%)", n_long, pct(n_long));
    println!("  open_short:     {} ({:.0}%)", n_short, pct(n_short));
    println!("  close:          {} ({:.0}%)", n_close, pct(n_close));
    if n_decisions > 0 {
        println!("  avg confidence: {:.0}%", 100.0 * conf_sum / n_decisions as f64);
    }
    if n_long + n_short > 0 && n_short == 0 {
        println!("  {}", "note: long-only so far — no shorts taken".dimmed());
    }
    println!();

    // ── Trades ──
    println!("{}", "Closed trades".bold());
    if trades.is_empty() {
        println!("  none yet — no positions have been closed in this journal.");
        if let (Some(a), Some(b)) = (first_price, last_price) {
            println!("  {}", format!("(BTC spot moved {:+.2}% over the window)", 100.0 * (b - a) / a).dimmed());
        }
        println!();
        return;
    }

    let n = trades.len();
    let wins: Vec<&Trade> = trades.iter().filter(|t| t.net_pl_sats > 0.0).collect();
    let losses: Vec<&Trade> = trades.iter().filter(|t| t.net_pl_sats <= 0.0).collect();
    let total: f64 = trades.iter().map(|t| t.net_pl_sats).sum();
    let gross_win: f64 = wins.iter().map(|t| t.net_pl_sats).sum();
    let gross_loss: f64 = losses.iter().map(|t| t.net_pl_sats).sum(); // <= 0
    let win_rate = 100.0 * wins.len() as f64 / n as f64;
    let avg_win = if wins.is_empty() { 0.0 } else { gross_win / wins.len() as f64 };
    let avg_loss = if losses.is_empty() { 0.0 } else { gross_loss / losses.len() as f64 };
    let profit_factor = if gross_loss.abs() > 0.0 {
        Some(gross_win / gross_loss.abs())
    } else {
        None
    };
    let largest_win = trades.iter().map(|t| t.net_pl_sats).fold(f64::MIN, f64::max);
    let largest_loss = trades.iter().map(|t| t.net_pl_sats).fold(f64::MAX, f64::min);
    let n_longs = trades.iter().filter(|t| t.side.eq_ignore_ascii_case("LONG")).count();

    let color_pnl = |v: f64| {
        if v >= 0.0 { format!("{:+.0}", v).green() } else { format!("{:+.0}", v).red() }
    };

    println!("  trades:         {}  ({} long / {} short)", n, n_longs, n - n_longs);
    println!("  win rate:       {:.0}%  ({}W / {}L)", win_rate, wins.len(), losses.len());
    println!("  total net P&L:  {} sats", color_pnl(total));
    println!("  avg win:        {} sats", color_pnl(avg_win));
    println!("  avg loss:       {} sats", color_pnl(avg_loss));
    match profit_factor {
        Some(pf) => println!("  profit factor:  {:.2}  {}", pf, if pf >= 1.0 { "(>1 = net profitable)".dimmed() } else { "(<1 = net losing)".dimmed() }),
        None => println!("  profit factor:  n/a (no losing trades yet)"),
    }
    println!("  largest win:    {} sats", color_pnl(largest_win));
    println!("  largest loss:   {} sats", color_pnl(largest_loss));
    let holds: Vec<i64> = trades.iter().filter_map(|t| t.hold_mins).collect();
    if !holds.is_empty() {
        let avg = holds.iter().sum::<i64>() as f64 / holds.len() as f64;
        println!("  avg hold:       {:.0} min", avg);
    }
    // Exit-reason breakdown — which mechanism is closing trades?
    let bucket = |t: &Trade| -> &'static str {
        let r = t.reason.to_lowercase();
        if r.contains("trailing") {
            "trailing stop"
        } else if r.contains("take profit") {
            "take profit"
        } else if r.contains("stop loss") {
            "stop loss"
        } else if r.contains("reversal") {
            "llm reversal"
        } else if r.contains("llm") {
            "llm close"
        } else {
            "other"
        }
    };
    let mut reasons: std::collections::BTreeMap<&str, (u64, f64)> = std::collections::BTreeMap::new();
    for t in trades {
        let e = reasons.entry(bucket(t)).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += t.net_pl_sats;
    }
    if !reasons.is_empty() {
        println!("  exit reasons:");
        for (r, (cnt, pnl)) in &reasons {
            println!("    {:<14} {}× ({} sats)", r, cnt, color_pnl(*pnl));
        }
    }
    println!();

    // ── Peak capture (approximate) ──
    let captured: Vec<&Trade> = trades
        .iter()
        .filter(|t| t.peak_favorable.is_some() && t.realized_move.is_some())
        .collect();
    if !captured.is_empty() {
        println!("{}", "Peak capture (approx — answers the trailing-stop question)".bold());
        let avg_peak = captured.iter().filter_map(|t| t.peak_favorable).sum::<f64>() / captured.len() as f64;
        let avg_real = captured.iter().filter_map(|t| t.realized_move).sum::<f64>() / captured.len() as f64;
        println!("  avg peak move:    {:+.2}%  (best unrealized, gross)", 100.0 * avg_peak);
        println!("  avg exit move:    {:+.2}%  (where it actually closed)", 100.0 * avg_real);
        // Capture ratio only meaningful on trades that went favorable.
        let ratios: Vec<f64> = captured
            .iter()
            .filter(|t| t.peak_favorable.unwrap_or(0.0) > 0.001)
            .map(|t| (t.realized_move.unwrap() / t.peak_favorable.unwrap()).clamp(-1.0, 1.0))
            .collect();
        if !ratios.is_empty() {
            let avg_cap = 100.0 * ratios.iter().sum::<f64>() / ratios.len() as f64;
            println!("  avg capture:      {:.0}%  of the peak favorable move was kept", avg_cap);
            println!("  {}", "low capture % ⇒ trailing stop too wide / exits too late".dimmed());
        }
        println!();
    }

    // ── Confidence vs outcome (thin until more trades) ──
    let with_conf: Vec<&Trade> = trades.iter().filter(|t| t.entry_confidence.is_some()).collect();
    if with_conf.len() >= 4 {
        let wc: f64 = with_conf.iter().filter(|t| t.net_pl_sats > 0.0).filter_map(|t| t.entry_confidence).sum();
        let wn = with_conf.iter().filter(|t| t.net_pl_sats > 0.0).count();
        let lc: f64 = with_conf.iter().filter(|t| t.net_pl_sats <= 0.0).filter_map(|t| t.entry_confidence).sum();
        let ln = with_conf.iter().filter(|t| t.net_pl_sats <= 0.0).count();
        println!("{}", "Confidence vs outcome".bold());
        if wn > 0 { println!("  avg entry conf, winners: {:.0}%", 100.0 * wc / wn as f64); }
        if ln > 0 { println!("  avg entry conf, losers:  {:.0}%", 100.0 * lc / ln as f64); }
        println!();
    }

    // ── Context ──
    if let (Some(a), Some(b)) = (first_price, last_price) {
        println!("{}", "Context".bold());
        println!("  BTC spot over window: {:+.2}%  (buy-and-hold benchmark)", 100.0 * (b - a) / a);
        println!();
    }

    // Honesty about sample size.
    if n < 20 {
        println!(
            "{}",
            format!(
                "⚠ {} closed trade(s) — far too few to infer an edge. Treat as a sanity check, not a verdict.",
                n
            )
            .yellow()
        );
    }
}
