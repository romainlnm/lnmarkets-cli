//! `lnmarkets alert` subcommand — declare conditions, fire OS notifications
//! when they're crossed.

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::Value;
use tabled::{Table, Tabled};

use crate::alert::{self, AlertStore, EvalState};
use crate::api::stream::{self, RawStreamMsg, StreamStatus};

#[derive(Subcommand)]
pub enum AlertCommands {
    /// Add a new rule (e.g. "price > 200000")
    Add(AddArgs),
    /// List all configured rules
    List,
    /// Remove a rule by ID
    Remove(RemoveArgs),
    /// Run the alert watcher in the foreground (Ctrl+C to exit)
    Watch,
}

#[derive(Args)]
pub struct AddArgs {
    /// The rule to add — quote it. Examples:
    /// "price > 200000", "funding > 0.05%", "funding flips positive"
    pub rule: String,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Rule ID, as shown in `alert list`
    pub id: u32,
}

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "ID")]
    id: u32,
    #[tabled(rename = "Rule")]
    rule: String,
    #[tabled(rename = "Enabled")]
    enabled: &'static str,
}

impl AlertCommands {
    pub async fn execute(self) -> Result<()> {
        match self {
            Self::Add(args) => add(args),
            Self::List => list(),
            Self::Remove(args) => remove(args),
            Self::Watch => watch().await,
        }
    }
}

fn add(args: AddArgs) -> Result<()> {
    let mut store = AlertStore::load()?;
    let inserted = store.add(&args.rule)?;
    let id = inserted.id;
    store.save()?;
    println!("Added alert #{}: {}", id, args.rule);
    Ok(())
}

fn list() -> Result<()> {
    let store = AlertStore::load()?;
    if store.alerts.is_empty() {
        println!("No alerts configured. Try: lnmarkets alert add \"price > 200000\"");
        return Ok(());
    }
    let rows: Vec<Row> = store
        .alerts
        .iter()
        .map(|a| Row {
            id: a.id,
            rule: a.rule.clone(),
            enabled: if a.enabled { "yes" } else { "no" },
        })
        .collect();
    println!("{}", Table::new(rows));
    Ok(())
}

fn remove(args: RemoveArgs) -> Result<()> {
    let mut store = AlertStore::load()?;
    if !store.remove(args.id) {
        anyhow::bail!("No alert with ID {}", args.id);
    }
    store.save()?;
    println!("Removed alert #{}", args.id);
    Ok(())
}

async fn watch() -> Result<()> {
    let store = AlertStore::load()?;
    if store.alerts.is_empty() {
        anyhow::bail!(
            "No alerts configured. Add one with: lnmarkets alert add \"price > 200000\""
        );
    }
    eprintln!("[alert] watching {} rule(s) — Ctrl+C to exit", store.alerts.len());
    for a in &store.alerts {
        eprintln!("  #{}: {}", a.id, a.rule);
    }

    // Public ticker covers all v1 rule types (price + funding). Authentication
    // not needed.
    let mut rx = stream::start_raw(
        vec!["futures/inverse/btc_usd/ticker".to_string()],
        None,
    );
    let mut state = EvalState::default();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\n[alert] exiting");
                break;
            }
            msg = rx.recv() => {
                match msg {
                    Some(RawStreamMsg::Status(s)) => {
                        let label = match s {
                            StreamStatus::Connecting => "connecting",
                            StreamStatus::Connected => "connected",
                            StreamStatus::Authenticated => "authenticated",
                            StreamStatus::Disconnected => "disconnected, reconnecting",
                        };
                        eprintln!("[alert] {}", label);
                    }
                    Some(RawStreamMsg::Data { data, .. }) => {
                        let (price, funding) = extract_ticker_fields(&data);
                        let triggers = alert::evaluate(
                            &store.alerts,
                            &mut state,
                            price,
                            funding,
                        );
                        for t in triggers {
                            // Stdout for scripting; native notification for humans.
                            println!("[#{}] {}", t.alert_id, t.message);
                            alert::notify(&t);
                        }
                    }
                    None => break,
                }
            }
        }
    }
    Ok(())
}

fn extract_ticker_fields(data: &Value) -> (Option<f64>, Option<f64>) {
    let price = data.get("lastPrice").and_then(|v| v.as_f64());
    let funding = data
        .get("funding")
        .and_then(|f| f.get("rate"))
        .and_then(|v| v.as_f64());
    (price, funding)
}
