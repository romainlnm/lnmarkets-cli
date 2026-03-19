//! Macro Agent - Economic Calendar Signals
//!
//! Fetches economic calendar from TradingView API.
//! Tracks major economic events and produces signals:
//! - Pre-event: reduce exposure before high-impact events
//! - Post-event: volatility opportunities after releases
//!
//! Key events tracked:
//! - FOMC (Federal Reserve decisions)
//! - CPI/PPI (inflation data)
//! - NFP (Non-Farm Payrolls)
//! - GDP releases

use super::{Agent, Direction, Signal};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;

/// Economic event importance
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Importance {
    Low = 1,
    Medium = 2,
    High = 3,
}

/// Economic event
#[derive(Debug, Clone)]
pub struct EconomicEvent {
    pub name: String,
    pub datetime: chrono::DateTime<Utc>,
    pub importance: Importance,
    pub country: String,
}

impl EconomicEvent {
    pub fn new(name: &str, datetime: chrono::DateTime<Utc>, importance: Importance) -> Self {
        Self {
            name: name.to_string(),
            datetime,
            importance,
            country: "US".to_string(),
        }
    }

    /// Minutes until this event
    pub fn minutes_until(&self) -> i64 {
        let now = Utc::now();
        (self.datetime - now).num_minutes()
    }

    /// Is this event imminent (within threshold)?
    pub fn is_imminent(&self, minutes: i64) -> bool {
        let mins = self.minutes_until();
        mins > 0 && mins <= minutes
    }

    /// Has this event just passed (within threshold)?
    pub fn just_passed(&self, minutes: i64) -> bool {
        let mins = self.minutes_until();
        mins < 0 && mins >= -minutes
    }
}

/// Configuration for Macro Agent
#[derive(Debug, Clone)]
pub struct MacroConfig {
    /// Warning threshold in minutes before event
    pub pre_event_warning_mins: i64,
    /// Post-event volatility window in minutes
    pub post_event_window_mins: i64,
    /// Only track high importance events
    pub high_importance_only: bool,
}

impl Default for MacroConfig {
    fn default() -> Self {
        Self {
            pre_event_warning_mins: 60,
            post_event_window_mins: 30,
            high_importance_only: true,
        }
    }
}

/// Macro Agent implementation
pub struct MacroAgent {
    config: MacroConfig,
    http_client: reqwest::Client,
}

impl MacroAgent {
    pub fn new(config: MacroConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(MacroConfig::default())
    }

    /// Get upcoming economic events from TradingView API
    async fn get_events(&self) -> Result<Vec<EconomicEvent>> {
        let now = Utc::now();
        let from = now.format("%Y-%m-%d").to_string();
        let to = (now + chrono::Duration::days(14)).format("%Y-%m-%d").to_string();

        let url = format!(
            "https://economic-calendar.tradingview.com/events?from={}&to={}&countries=US",
            from, to
        );

        let response = self.http_client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .header("Accept", "application/json")
            .header("Origin", "https://www.tradingview.com")
            .header("Referer", "https://www.tradingview.com/")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("TradingView API returned {}", response.status());
        }

        let data: TradingViewResponse = response.json().await?;

        let events: Vec<EconomicEvent> = data.result
            .into_iter()
            .filter(|e| {
                // importance: 1 = high, 0 = medium, -1 = low
                let dominated = e.importance >= 1;
                !self.config.high_importance_only || dominated
            })
            .filter_map(|e| {
                let datetime = chrono::DateTime::parse_from_rfc3339(&e.date).ok()?;
                let importance = match e.importance {
                    1.. => Importance::High,
                    0 => Importance::Medium,
                    _ => Importance::Low,
                };
                Some(EconomicEvent {
                    name: e.title,
                    datetime: datetime.with_timezone(&Utc),
                    importance,
                    country: "US".to_string(),
                })
            })
            .collect();

        Ok(events)
    }

    /// Analyze events and produce signal
    fn analyze_events(&self, events: &[EconomicEvent]) -> Signal {
        let now = Utc::now();

        // Check for imminent high-impact events
        for event in events {
            if event.importance == Importance::High {
                let mins = event.minutes_until();

                // Event very soon (< 15 min) - strong caution
                if mins > 0 && mins <= 15 {
                    return Signal::new(
                        Direction::Neutral,
                        0.9,
                        "macro",
                        &format!(
                            "CAUTION: {} in {} min - reduce exposure",
                            event.name, mins
                        ),
                    );
                }

                // Event coming (15-60 min) - moderate caution
                if mins > 15 && mins <= self.config.pre_event_warning_mins {
                    return Signal::new(
                        Direction::Neutral,
                        0.7,
                        "macro",
                        &format!(
                            "WARNING: {} in {} min - consider reducing positions",
                            event.name, mins
                        ),
                    );
                }

                // Event just passed - volatility window
                if event.just_passed(self.config.post_event_window_mins) {
                    return Signal::new(
                        Direction::Neutral,
                        0.6,
                        "macro",
                        &format!(
                            "POST-EVENT: {} released {} min ago - high volatility",
                            event.name, -mins
                        ),
                    );
                }
            }
        }

        // Find next event for status
        let next_event = events.iter()
            .filter(|e| e.minutes_until() > 0)
            .min_by_key(|e| e.minutes_until());

        let status = if let Some(event) = next_event {
            let mins = event.minutes_until();
            let hours = mins / 60;
            let time_str = if hours > 24 {
                format!("{}d", hours / 24)
            } else if hours > 0 {
                format!("{}h", hours)
            } else {
                format!("{}m", mins)
            };
            format!("Next: {} in {}", event.name, time_str)
        } else {
            "No major events in next 14 days".to_string()
        };

        Signal::neutral("macro", &status)
    }
}

#[async_trait]
impl Agent for MacroAgent {
    fn name(&self) -> &str {
        "macro"
    }

    async fn analyze(&self) -> Result<Signal> {
        let events = self.get_events().await?;
        Ok(self.analyze_events(&events))
    }
}

/// TradingView API response
#[derive(Debug, Deserialize)]
struct TradingViewResponse {
    result: Vec<TradingViewEvent>,
}

/// TradingView event structure
#[derive(Debug, Deserialize)]
struct TradingViewEvent {
    title: String,
    date: String,
    importance: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_imminent() {
        let event = EconomicEvent::new(
            "Test",
            Utc::now() + chrono::Duration::minutes(30),
            Importance::High,
        );
        assert!(event.is_imminent(60));
        assert!(!event.is_imminent(15));
    }
}
