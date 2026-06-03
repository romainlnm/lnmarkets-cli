//! Trading statistics — tracks daemon trade IDs.
//!
//! Stores only trade IDs locally. Full trade data is fetched from the LN
//! Markets API when `lnmarkets stats` runs.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn trades_file_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?;
    Ok(config_dir.join("lnmarkets").join("daemon_trades.txt"))
}

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

pub fn save_trade_id(trade_id: &str) -> Result<()> {
    let path = trades_file_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    writeln!(file, "{}", trade_id)?;
    Ok(())
}
