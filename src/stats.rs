//! Trading statistics with SQLite storage
//!
//! Tracks trade history and calculates performance metrics.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use std::path::PathBuf;

/// Trade record for statistics
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub id: i64,
    pub direction: String,      // "long" or "short"
    pub quantity_usd: f64,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub pnl_sats: Option<i64>,
    pub confidence: f64,
    pub agents: String,         // comma-separated agent signals
    pub mode: String,           // "live" or "paper"
    pub closed: bool,
}

/// Trading statistics summary
#[derive(Debug, Clone)]
pub struct TradingStats {
    pub total_trades: u32,
    pub open_trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub win_rate: f64,
    pub total_pnl_sats: i64,
    pub best_trade_sats: i64,
    pub worst_trade_sats: i64,
    pub avg_pnl_sats: f64,
    pub current_streak: i32,    // positive = wins, negative = losses
    pub avg_hold_mins: f64,
}

/// Stats database manager
pub struct StatsDb {
    conn: Connection,
}

impl StatsDb {
    /// Open or create the stats database
    pub fn open() -> Result<Self> {
        let db_path = Self::db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Get database file path
    fn db_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?;
        Ok(config_dir.join("lnmarkets").join("stats.db"))
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                direction TEXT NOT NULL,
                quantity_usd REAL NOT NULL,
                entry_price REAL NOT NULL,
                exit_price REAL,
                entry_time TEXT NOT NULL,
                exit_time TEXT,
                pnl_sats INTEGER,
                confidence REAL NOT NULL,
                agents TEXT NOT NULL,
                mode TEXT NOT NULL,
                closed INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        // Index for faster queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trades_closed ON trades(closed)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trades_mode ON trades(mode)",
            [],
        )?;

        Ok(())
    }

    /// Record a new trade opening
    pub fn record_open(
        &self,
        direction: &str,
        quantity_usd: f64,
        entry_price: f64,
        confidence: f64,
        agents: &str,
        mode: &str,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO trades (direction, quantity_usd, entry_price, entry_time, confidence, agents, mode, closed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![direction, quantity_usd, entry_price, now, confidence, agents, mode],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Record a trade closing
    pub fn record_close(
        &self,
        trade_id: i64,
        exit_price: f64,
        pnl_sats: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "UPDATE trades SET exit_price = ?1, exit_time = ?2, pnl_sats = ?3, closed = 1
             WHERE id = ?4",
            params![exit_price, now, pnl_sats, trade_id],
        )?;

        Ok(())
    }

    /// Close all open trades (e.g., when position is closed externally)
    pub fn close_all_open(&self, exit_price: f64, mode: &str) -> Result<u32> {
        let now = Utc::now().to_rfc3339();

        // First get open trades to calculate P&L
        let mut stmt = self.conn.prepare(
            "SELECT id, direction, quantity_usd, entry_price FROM trades WHERE closed = 0 AND mode = ?1"
        )?;

        let trades: Vec<(i64, String, f64, f64)> = stmt.query_map([mode], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?.filter_map(|r| r.ok()).collect();

        let mut closed = 0;
        for (id, direction, quantity, entry_price) in trades {
            // Calculate P&L using inverse perpetual formula
            let inv_diff = (1.0 / exit_price) - (1.0 / entry_price);
            let pnl_sats = if direction == "long" {
                (-quantity * inv_diff * 100_000_000.0) as i64
            } else {
                (quantity * inv_diff * 100_000_000.0) as i64
            };

            self.conn.execute(
                "UPDATE trades SET exit_price = ?1, exit_time = ?2, pnl_sats = ?3, closed = 1 WHERE id = ?4",
                params![exit_price, now, pnl_sats, id],
            )?;
            closed += 1;
        }

        Ok(closed)
    }

    /// Get the most recent open trade
    pub fn get_open_trade(&self, mode: &str) -> Result<Option<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, direction FROM trades WHERE closed = 0 AND mode = ?1 ORDER BY id DESC LIMIT 1"
        )?;

        let result = stmt.query_row([mode], |row| {
            Ok((row.get(0)?, row.get(1)?))
        });

        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Calculate trading statistics
    pub fn get_stats(&self, mode: Option<&str>) -> Result<TradingStats> {
        let mode_filter = mode.unwrap_or("%");

        // Basic counts
        let total_trades: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let open_trades: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE closed = 0 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let wins: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE closed = 1 AND pnl_sats > 0 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let losses: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE closed = 1 AND pnl_sats <= 0 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let closed_trades = wins + losses;
        let win_rate = if closed_trades > 0 {
            wins as f64 / closed_trades as f64 * 100.0
        } else {
            0.0
        };

        // P&L stats
        let total_pnl_sats: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(pnl_sats), 0) FROM trades WHERE closed = 1 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let best_trade_sats: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(pnl_sats), 0) FROM trades WHERE closed = 1 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let worst_trade_sats: i64 = self.conn.query_row(
            "SELECT COALESCE(MIN(pnl_sats), 0) FROM trades WHERE closed = 1 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        let avg_pnl_sats = if closed_trades > 0 {
            total_pnl_sats as f64 / closed_trades as f64
        } else {
            0.0
        };

        // Current streak
        let current_streak = self.calculate_streak(mode)?;

        // Average hold time
        let avg_hold_mins: f64 = self.conn.query_row(
            "SELECT COALESCE(AVG(
                (julianday(exit_time) - julianday(entry_time)) * 24 * 60
             ), 0) FROM trades WHERE closed = 1 AND mode LIKE ?1",
            [mode_filter],
            |row| row.get(0),
        )?;

        Ok(TradingStats {
            total_trades,
            open_trades,
            wins,
            losses,
            win_rate,
            total_pnl_sats,
            best_trade_sats,
            worst_trade_sats,
            avg_pnl_sats,
            current_streak,
            avg_hold_mins,
        })
    }

    /// Calculate current win/loss streak
    fn calculate_streak(&self, mode: Option<&str>) -> Result<i32> {
        let mode_filter = mode.unwrap_or("%");

        let mut stmt = self.conn.prepare(
            "SELECT pnl_sats FROM trades WHERE closed = 1 AND mode LIKE ?1 ORDER BY exit_time DESC LIMIT 20"
        )?;

        let pnls: Vec<i64> = stmt.query_map([mode_filter], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if pnls.is_empty() {
            return Ok(0);
        }

        let first_positive = pnls[0] > 0;
        let mut streak = 0i32;

        for pnl in pnls {
            if (pnl > 0) == first_positive {
                streak += 1;
            } else {
                break;
            }
        }

        Ok(if first_positive { streak } else { -streak })
    }

    /// Get recent trades
    pub fn get_recent_trades(&self, limit: u32, mode: Option<&str>) -> Result<Vec<TradeRecord>> {
        let mode_filter = mode.unwrap_or("%");

        let mut stmt = self.conn.prepare(
            "SELECT id, direction, quantity_usd, entry_price, exit_price, entry_time, exit_time,
                    pnl_sats, confidence, agents, mode, closed
             FROM trades WHERE mode LIKE ?1 ORDER BY id DESC LIMIT ?2"
        )?;

        let trades = stmt.query_map(params![mode_filter, limit], |row| {
            let entry_time: String = row.get(5)?;
            let exit_time: Option<String> = row.get(6)?;

            Ok(TradeRecord {
                id: row.get(0)?,
                direction: row.get(1)?,
                quantity_usd: row.get(2)?,
                entry_price: row.get(3)?,
                exit_price: row.get(4)?,
                entry_time: DateTime::parse_from_rfc3339(&entry_time)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                exit_time: exit_time.and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                pnl_sats: row.get(7)?,
                confidence: row.get(8)?,
                agents: row.get(9)?,
                mode: row.get(10)?,
                closed: row.get(11)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        Ok(trades)
    }
}

/// Format stats for display
pub fn format_stats(stats: &TradingStats, mode: &str) -> String {
    let streak_str = if stats.current_streak > 0 {
        format!("{}W \x1b[32m{}\x1b[0m", stats.current_streak, "🔥")
    } else if stats.current_streak < 0 {
        format!("{}L \x1b[31m{}\x1b[0m", -stats.current_streak, "💀")
    } else {
        "0".to_string()
    };

    let pnl_color = if stats.total_pnl_sats >= 0 { "\x1b[32m" } else { "\x1b[31m" };

    format!(
        r#"
{} Trading Stats
{}
Trades:      {} total ({} open)
Win/Loss:    {} / {} ({:.1}%)
Total P&L:   {}{:+} sats\x1b[0m
Best trade:  {:+} sats
Worst trade: {:+} sats
Avg P&L:     {:+.0} sats
Streak:      {}
Avg hold:    {:.0} min
"#,
        if mode == "live" { "\x1b[31mLIVE\x1b[0m" } else { "\x1b[36mPAPER\x1b[0m" },
        "─".repeat(30),
        stats.total_trades,
        stats.open_trades,
        stats.wins,
        stats.losses,
        stats.win_rate,
        pnl_color,
        stats.total_pnl_sats,
        stats.best_trade_sats,
        stats.worst_trade_sats,
        stats.avg_pnl_sats,
        streak_str,
        stats.avg_hold_mins,
    )
}
