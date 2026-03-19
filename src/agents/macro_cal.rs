//! Macro Agent - Economic Calendar Signals
//!
//! Fetches economic calendar from TradingView API.
//! Analyzes recent economic data releases and upcoming events:
//! - Surprise analysis: compares actual vs forecast values
//! - Pre-event warnings: reduce exposure before high-impact events
//! - BTC impact assessment based on Fed policy implications

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

/// Configuration for Macro Agent
#[derive(Debug, Clone)]
pub struct MacroConfig {
    /// Warning threshold in minutes before event
    pub pre_event_warning_mins: i64,
    /// Look back window for recent releases (hours)
    pub lookback_hours: i64,
    /// Minimum surprise percentage to generate signal
    pub min_surprise_pct: f64,
}

impl Default for MacroConfig {
    fn default() -> Self {
        Self {
            pre_event_warning_mins: 60,
            lookback_hours: 6,
            min_surprise_pct: 5.0,
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

    /// Fetch events from TradingView API
    async fn fetch_events(&self, from: &str, to: &str) -> Result<Vec<TradingViewEvent>> {
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
        Ok(data.result.unwrap_or_default())
    }

    /// Analyze recent releases for surprises
    async fn analyze_recent_releases(&self) -> Option<(Direction, f64, String)> {
        let now = Utc::now();
        let from = (now - chrono::Duration::hours(self.config.lookback_hours))
            .format("%Y-%m-%d")
            .to_string();
        let to = now.format("%Y-%m-%d").to_string();

        let events = self.fetch_events(&from, &to).await.ok()?;

        // Find significant surprises in recent releases
        let mut best_surprise: Option<(Direction, f64, String)> = None;

        for event in events {
            // Skip if no actual value or low importance
            let actual = event.actual?;
            let forecast = event.forecast?;
            let importance = event.importance;

            // Only consider medium+ importance
            if importance < 0 {
                continue;
            }

            // Check if event was within lookback window
            let event_time = chrono::DateTime::parse_from_rfc3339(&event.date).ok()?;
            let mins_ago = (now - event_time.with_timezone(&Utc)).num_minutes();
            if mins_ago < 0 || mins_ago > self.config.lookback_hours * 60 {
                continue;
            }

            // Calculate surprise percentage
            let surprise_pct = if forecast != 0.0 {
                ((actual - forecast) / forecast.abs()) * 100.0
            } else {
                continue;
            };

            // Skip small surprises
            if surprise_pct.abs() < self.config.min_surprise_pct {
                continue;
            }

            // Determine BTC impact based on event type and surprise direction
            let (direction, confidence) = self.assess_btc_impact(&event.title, surprise_pct, importance);

            // Keep the most significant surprise
            if direction != Direction::Neutral {
                let current_conf = best_surprise.as_ref().map(|(_, c, _)| *c).unwrap_or(0.0);
                if confidence > current_conf {
                    let reasoning = format!(
                        "{}: {:.1} vs {:.1} exp ({:+.1}%) {}",
                        event.title,
                        actual,
                        forecast,
                        surprise_pct,
                        if mins_ago < 60 {
                            format!("{}m ago", mins_ago)
                        } else {
                            format!("{}h ago", mins_ago / 60)
                        }
                    );
                    best_surprise = Some((direction, confidence, reasoning));
                }
            }
        }

        best_surprise
    }

    /// Assess BTC impact based on economic indicator and surprise
    fn assess_btc_impact(&self, title: &str, surprise_pct: f64, importance: i32) -> (Direction, f64) {
        let title_lower = title.to_lowercase();

        // Base confidence from importance
        let base_conf = match importance {
            1.. => 0.7,
            0 => 0.55,
            _ => 0.5,
        };

        // Scale confidence by surprise magnitude (cap at 2x)
        let surprise_factor = (surprise_pct.abs() / 10.0).min(2.0);
        let confidence = (base_conf + (surprise_factor * 0.15)).min(0.95);

        // Determine direction based on indicator type
        // For BTC: hawkish Fed = bearish, dovish Fed = bullish

        // INFLATION indicators - higher = hawkish = bearish for BTC
        if title_lower.contains("cpi") || title_lower.contains("ppi")
            || title_lower.contains("inflation") || title_lower.contains("pce") {
            return if surprise_pct > 0.0 {
                (Direction::Short, confidence) // Higher inflation = bearish
            } else {
                (Direction::Long, confidence) // Lower inflation = bullish
            };
        }

        // EMPLOYMENT indicators - stronger jobs = hawkish = bearish for BTC
        if title_lower.contains("payroll") || title_lower.contains("nfp")
            || title_lower.contains("employment") || title_lower.contains("jobs") {
            return if surprise_pct > 0.0 {
                (Direction::Short, confidence) // More jobs = bearish
            } else {
                (Direction::Long, confidence) // Fewer jobs = bullish
            };
        }

        // UNEMPLOYMENT - lower = hawkish = bearish for BTC
        if title_lower.contains("unemployment") || title_lower.contains("jobless") {
            return if surprise_pct < 0.0 {
                (Direction::Short, confidence) // Lower unemployment = bearish
            } else {
                (Direction::Long, confidence) // Higher unemployment = bullish
            };
        }

        // HOUSING indicators - weaker = dovish = bullish for BTC
        if title_lower.contains("home") || title_lower.contains("housing")
            || title_lower.contains("building") || title_lower.contains("mortgage") {
            return if surprise_pct < 0.0 {
                (Direction::Long, confidence * 0.8) // Weak housing = bullish (dovish)
            } else {
                (Direction::Short, confidence * 0.8) // Strong housing = bearish
            };
        }

        // GDP - weaker = dovish = bullish for BTC
        if title_lower.contains("gdp") {
            return if surprise_pct < 0.0 {
                (Direction::Long, confidence * 0.9) // Weak GDP = bullish
            } else {
                (Direction::Short, confidence * 0.9) // Strong GDP = bearish
            };
        }

        // RETAIL SALES - stronger = hawkish = bearish
        if title_lower.contains("retail") {
            return if surprise_pct > 0.0 {
                (Direction::Short, confidence * 0.7)
            } else {
                (Direction::Long, confidence * 0.7)
            };
        }

        // FED related - interpret based on title
        if title_lower.contains("fed") || title_lower.contains("fomc") {
            // Can't easily interpret from numbers, stay neutral
            return (Direction::Neutral, 0.5);
        }

        // Default: no clear signal
        (Direction::Neutral, 0.5)
    }

    /// Get upcoming high-impact events
    async fn get_upcoming_events(&self) -> Result<Option<(String, i64)>> {
        let now = Utc::now();
        let from = now.format("%Y-%m-%d").to_string();
        let to = (now + chrono::Duration::days(14)).format("%Y-%m-%d").to_string();

        let events = self.fetch_events(&from, &to).await?;

        // Find next high-importance event
        for event in events {
            if event.importance >= 1 {
                if let Ok(event_time) = chrono::DateTime::parse_from_rfc3339(&event.date) {
                    let mins = (event_time.with_timezone(&Utc) - now).num_minutes();
                    if mins > 0 {
                        return Ok(Some((event.title, mins)));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl Agent for MacroAgent {
    fn name(&self) -> &str {
        "macro"
    }

    async fn analyze(&self) -> Result<Signal> {
        // First, check for recent surprises
        if let Some((direction, confidence, reasoning)) = self.analyze_recent_releases().await {
            return Ok(Signal::new(direction, confidence, "macro", &reasoning));
        }

        // Check for upcoming events
        if let Ok(Some((event_name, mins))) = self.get_upcoming_events().await {
            // Event very soon - strong caution
            if mins <= 15 {
                return Ok(Signal::new(
                    Direction::Neutral,
                    0.9,
                    "macro",
                    &format!("CAUTION: {} in {} min", event_name, mins),
                ));
            }

            // Event coming soon
            if mins <= self.config.pre_event_warning_mins {
                return Ok(Signal::new(
                    Direction::Neutral,
                    0.7,
                    "macro",
                    &format!("WARNING: {} in {} min", event_name, mins),
                ));
            }

            // Show next event
            let time_str = if mins > 24 * 60 {
                format!("{}d", mins / (24 * 60))
            } else if mins > 60 {
                format!("{}h", mins / 60)
            } else {
                format!("{}m", mins)
            };

            return Ok(Signal::neutral("macro", &format!("Next: {} in {}", event_name, time_str)));
        }

        Ok(Signal::neutral("macro", "No major events"))
    }
}

/// TradingView API response
#[derive(Debug, Deserialize)]
struct TradingViewResponse {
    #[serde(default)]
    result: Option<Vec<TradingViewEvent>>,
}

/// TradingView event structure
#[derive(Debug, Deserialize)]
struct TradingViewEvent {
    title: String,
    date: String,
    importance: i32,
    #[serde(default)]
    actual: Option<f64>,
    #[serde(default)]
    forecast: Option<f64>,
    #[serde(default)]
    previous: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btc_impact_cpi() {
        let agent = MacroAgent::with_defaults();

        // Higher CPI = bearish
        let (dir, _) = agent.assess_btc_impact("CPI m/m", 10.0, 1);
        assert_eq!(dir, Direction::Short);

        // Lower CPI = bullish
        let (dir, _) = agent.assess_btc_impact("CPI m/m", -10.0, 1);
        assert_eq!(dir, Direction::Long);
    }

    #[test]
    fn test_btc_impact_housing() {
        let agent = MacroAgent::with_defaults();

        // Weak housing = bullish (dovish Fed)
        let (dir, _) = agent.assess_btc_impact("New Home Sales", -20.0, 1);
        assert_eq!(dir, Direction::Long);
    }
}
