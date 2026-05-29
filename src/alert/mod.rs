//! Alert rules for the LN Markets CLI.
//!
//! Lightweight rule system over the WS stream — declare conditions on the
//! ticker channel (price thresholds, funding bounds, funding sign flips) and
//! fire OS-native notifications when they're crossed.
//!
//! v1 deliberately limits the grammar to the few rules that are easy to parse
//! and don't require account state. Position-aware rules (P&L, liquidation
//! distance) and webhooks land in follow-up PRs.

pub mod parser;

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Rule {
    PriceAbove(f64),
    PriceBelow(f64),
    /// Funding stored as a decimal: 0.0005 = 0.05%
    FundingAbove(f64),
    FundingBelow(f64),
    FundingFlipsPositive,
    FundingFlipsNegative,
}

impl Rule {
    /// Human-readable rendering, used in `alert list` and notifications.
    pub fn display(&self) -> String {
        match self {
            Self::PriceAbove(v) => format!("price > ${:.0}", v),
            Self::PriceBelow(v) => format!("price < ${:.0}", v),
            Self::FundingAbove(v) => format!("funding > {:.4}%", v * 100.0),
            Self::FundingBelow(v) => format!("funding < {:.4}%", v * 100.0),
            Self::FundingFlipsPositive => "funding flips positive".into(),
            Self::FundingFlipsNegative => "funding flips negative".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: u32,
    /// The original user-typed rule string — kept so the TOML file stays
    /// human-editable. The Rule is re-parsed on load.
    pub rule: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AlertStore {
    #[serde(default)]
    pub alerts: Vec<Alert>,
}

impl AlertStore {
    pub fn path() -> Result<PathBuf> {
        Ok(Config::config_dir()?.join("alerts.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("read {:?}", path))?;
        toml::from_str(&text).with_context(|| "parse alerts.toml")
    }

    pub fn save(&self) -> Result<()> {
        let dir = Config::config_dir()?;
        fs::create_dir_all(&dir).with_context(|| format!("mkdir {:?}", dir))?;
        let path = Self::path()?;
        let text = toml::to_string_pretty(self).context("serialize alerts")?;
        fs::write(&path, text).with_context(|| format!("write {:?}", path))?;
        Ok(())
    }

    pub fn next_id(&self) -> u32 {
        self.alerts.iter().map(|a| a.id).max().unwrap_or(0) + 1
    }

    pub fn add(&mut self, rule_str: &str) -> Result<&Alert> {
        // Parse to validate before persisting.
        parser::parse(rule_str)?;
        let id = self.next_id();
        self.alerts.push(Alert {
            id,
            rule: rule_str.to_string(),
            enabled: true,
        });
        Ok(self.alerts.last().expect("just inserted"))
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.alerts.len();
        self.alerts.retain(|a| a.id != id);
        self.alerts.len() < before
    }
}

/// Tracks last observed values so rules fire only on threshold crossings, not
/// continuously while a condition holds. One transition = one notification.
#[derive(Default)]
pub struct EvalState {
    pub last_price: Option<f64>,
    pub last_funding: Option<f64>,
}

#[derive(Debug)]
pub struct Trigger {
    pub alert_id: u32,
    /// Exposed for richer future consumers (webhooks, structured logs);
    /// unused by the v1 stdout + OS-notification path.
    #[allow(dead_code)]
    pub rule: Rule,
    pub message: String,
}

/// Evaluate all enabled alerts against a new ticker push. Returns one Trigger
/// per rule that just transitioned into its match state.
pub fn evaluate(
    alerts: &[Alert],
    state: &mut EvalState,
    new_price: Option<f64>,
    new_funding: Option<f64>,
) -> Vec<Trigger> {
    let mut fired = Vec::new();
    for alert in alerts.iter().filter(|a| a.enabled) {
        let rule = match parser::parse(&alert.rule) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rule_just_fired(rule, state, new_price, new_funding) {
            fired.push(Trigger {
                alert_id: alert.id,
                rule,
                message: trigger_message(rule, new_price, new_funding),
            });
        }
    }
    // Commit observed values for the next call.
    if let Some(p) = new_price {
        state.last_price = Some(p);
    }
    if let Some(f) = new_funding {
        state.last_funding = Some(f);
    }
    fired
}

fn rule_just_fired(
    rule: Rule,
    state: &EvalState,
    new_price: Option<f64>,
    new_funding: Option<f64>,
) -> bool {
    match rule {
        Rule::PriceAbove(threshold) => match (state.last_price, new_price) {
            (Some(prev), Some(curr)) => prev <= threshold && curr > threshold,
            _ => false,
        },
        Rule::PriceBelow(threshold) => match (state.last_price, new_price) {
            (Some(prev), Some(curr)) => prev >= threshold && curr < threshold,
            _ => false,
        },
        Rule::FundingAbove(threshold) => match (state.last_funding, new_funding) {
            (Some(prev), Some(curr)) => prev <= threshold && curr > threshold,
            _ => false,
        },
        Rule::FundingBelow(threshold) => match (state.last_funding, new_funding) {
            (Some(prev), Some(curr)) => prev >= threshold && curr < threshold,
            _ => false,
        },
        Rule::FundingFlipsPositive => match (state.last_funding, new_funding) {
            (Some(prev), Some(curr)) => prev <= 0.0 && curr > 0.0,
            _ => false,
        },
        Rule::FundingFlipsNegative => match (state.last_funding, new_funding) {
            (Some(prev), Some(curr)) => prev >= 0.0 && curr < 0.0,
            _ => false,
        },
    }
}

fn trigger_message(rule: Rule, price: Option<f64>, funding: Option<f64>) -> String {
    match rule {
        Rule::PriceAbove(_) | Rule::PriceBelow(_) => {
            let p = price.map(|v| format!("${:.0}", v)).unwrap_or("?".into());
            format!("{} — at {}", rule.display(), p)
        }
        Rule::FundingAbove(_)
        | Rule::FundingBelow(_)
        | Rule::FundingFlipsPositive
        | Rule::FundingFlipsNegative => {
            let f = funding
                .map(|v| format!("{:.4}%", v * 100.0))
                .unwrap_or("?".into());
            format!("{} — at {}", rule.display(), f)
        }
    }
}

/// Fire an OS-native notification for a triggered rule.
pub fn notify(trigger: &Trigger) {
    let _ = notify_rust::Notification::new()
        .summary("LN Markets")
        .body(&trigger.message)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(id: u32, rule: &str) -> Alert {
        Alert {
            id,
            rule: rule.into(),
            enabled: true,
        }
    }

    #[test]
    fn price_above_fires_once_on_crossing() {
        let alerts = vec![alert(1, "price > 200000")];
        let mut state = EvalState::default();

        // First push seeds state — never fires on the very first tick.
        assert!(evaluate(&alerts, &mut state, Some(195000.0), None).is_empty());
        // Still below — no fire.
        assert!(evaluate(&alerts, &mut state, Some(199000.0), None).is_empty());
        // Crosses upward — fires.
        assert_eq!(evaluate(&alerts, &mut state, Some(201000.0), None).len(), 1);
        // Stays above — does NOT fire again.
        assert!(evaluate(&alerts, &mut state, Some(202000.0), None).is_empty());
        // Drops back below — no fire on the way down for an Above rule.
        assert!(evaluate(&alerts, &mut state, Some(198000.0), None).is_empty());
        // Crosses up again — fires once more.
        assert_eq!(evaluate(&alerts, &mut state, Some(201500.0), None).len(), 1);
    }

    #[test]
    fn funding_flip_positive() {
        let alerts = vec![alert(1, "funding flips positive")];
        let mut state = EvalState::default();
        evaluate(&alerts, &mut state, None, Some(-0.0001));
        let fired = evaluate(&alerts, &mut state, None, Some(0.0002));
        assert_eq!(fired.len(), 1);
        // Already positive, doesn't re-fire on further positive readings.
        assert!(evaluate(&alerts, &mut state, None, Some(0.0005)).is_empty());
    }
}
