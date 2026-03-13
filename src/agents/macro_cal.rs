//! Macro Agent - Economic Calendar Signals
//!
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
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
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

    /// Get upcoming economic events
    async fn get_events(&self) -> Result<Vec<EconomicEvent>> {
        // Try to fetch from API first
        if let Ok(events) = self.fetch_from_api().await {
            if !events.is_empty() {
                return Ok(events);
            }
        }

        // Fallback to calculated known events
        Ok(self.get_known_events())
    }

    /// Try to fetch from investing.com or similar API
    async fn fetch_from_api(&self) -> Result<Vec<EconomicEvent>> {
        // Investing.com calendar API (unofficial)
        let now = Utc::now();
        let from = now.format("%Y-%m-%d").to_string();
        let to = (now + chrono::Duration::days(7)).format("%Y-%m-%d").to_string();

        let url = format!(
            "https://nfs.faireconomy.media/ff_calendar_thisweek.json"
        );

        let response = self.http_client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("API returned {}", response.status());
        }

        let data: Vec<ForexFactoryEvent> = response.json().await?;

        let events: Vec<EconomicEvent> = data
            .into_iter()
            .filter(|e| e.country == "USD")
            .filter(|e| {
                let imp = match e.impact.as_str() {
                    "High" => Importance::High,
                    "Medium" => Importance::Medium,
                    _ => Importance::Low,
                };
                !self.config.high_importance_only || imp == Importance::High
            })
            .filter_map(|e| {
                let datetime = chrono::DateTime::parse_from_rfc3339(&e.date).ok()?;
                let importance = match e.impact.as_str() {
                    "High" => Importance::High,
                    "Medium" => Importance::Medium,
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

    /// Get known scheduled events (FOMC, CPI, NFP)
    fn get_known_events(&self) -> Vec<EconomicEvent> {
        let mut events = Vec::new();
        let now = Utc::now();
        let today = now.date_naive();

        // FOMC 2026 dates (announced by Fed)
        // Meetings typically end at 2:00 PM ET (18:00 UTC in winter, 19:00 UTC in summer)
        let fomc_dates_2026 = [
            "2026-01-29", "2026-03-19", "2026-05-07", "2026-06-18",
            "2026-07-30", "2026-09-17", "2026-11-05", "2026-12-17",
        ];

        for date_str in fomc_dates_2026 {
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                if date >= today && date <= today + chrono::Duration::days(14) {
                    let datetime = Utc.from_utc_datetime(
                        &date.and_time(NaiveTime::from_hms_opt(18, 0, 0).unwrap())
                    );
                    events.push(EconomicEvent::new("FOMC Rate Decision", datetime, Importance::High));
                }
            }
        }

        // CPI - usually released around 8:30 AM ET on ~10th-13th of each month
        let cpi_day = self.find_cpi_date(today);
        if let Some(cpi_date) = cpi_day {
            if cpi_date >= today && cpi_date <= today + chrono::Duration::days(14) {
                let datetime = Utc.from_utc_datetime(
                    &cpi_date.and_time(NaiveTime::from_hms_opt(12, 30, 0).unwrap()) // 8:30 ET = 12:30 UTC
                );
                events.push(EconomicEvent::new("CPI Release", datetime, Importance::High));
            }
        }

        // NFP - first Friday of month, 8:30 AM ET
        let nfp_date = self.find_first_friday(today);
        if nfp_date >= today && nfp_date <= today + chrono::Duration::days(14) {
            let datetime = Utc.from_utc_datetime(
                &nfp_date.and_time(NaiveTime::from_hms_opt(12, 30, 0).unwrap())
            );
            events.push(EconomicEvent::new("Non-Farm Payrolls", datetime, Importance::High));
        }

        // Sort by datetime
        events.sort_by_key(|e| e.datetime);
        events
    }

    /// Find CPI release date for current/next month (usually 10th-13th, not on weekend)
    fn find_cpi_date(&self, today: NaiveDate) -> Option<NaiveDate> {
        let year = today.year();
        let month = today.month();

        // Try current month first, then next month
        for m in [month, if month == 12 { 1 } else { month + 1 }] {
            let y = if m < month { year + 1 } else { year };

            // CPI is usually around 10th-13th
            for day in 10..=15 {
                if let Some(date) = NaiveDate::from_ymd_opt(y, m, day) {
                    // Skip weekends
                    if date.weekday() != Weekday::Sat && date.weekday() != Weekday::Sun {
                        if date >= today {
                            return Some(date);
                        }
                    }
                }
            }
        }
        None
    }

    /// Find first Friday of current/next month
    fn find_first_friday(&self, today: NaiveDate) -> NaiveDate {
        let year = today.year();
        let month = today.month();

        // Try current month
        for day in 1..=7 {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                if date.weekday() == Weekday::Fri && date >= today {
                    return date;
                }
            }
        }

        // Next month
        let (y, m) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        for day in 1..=7 {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, day) {
                if date.weekday() == Weekday::Fri {
                    return date;
                }
            }
        }

        today // Fallback
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

/// ForexFactory event structure
#[derive(Debug, Deserialize)]
struct ForexFactoryEvent {
    title: String,
    country: String,
    date: String,
    impact: String,
    #[serde(default)]
    forecast: Option<String>,
    #[serde(default)]
    previous: Option<String>,
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

    #[test]
    fn test_find_first_friday() {
        let agent = MacroAgent::with_defaults();
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let friday = agent.find_first_friday(today);
        assert_eq!(friday.weekday(), Weekday::Fri);
        assert!(friday.day() <= 7);
    }
}
