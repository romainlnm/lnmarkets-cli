//! Economic calendar fetching from TradingView
//! Reuses logic from macro_cal.rs agent

use super::types::{BtcImpact, EconomicEvent, EventImportance};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

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

/// Fetch economic events from TradingView
async fn fetch_events(
    client: &reqwest::Client,
    from: &str,
    to: &str,
) -> Result<Vec<TradingViewEvent>> {
    let url = format!(
        "https://economic-calendar.tradingview.com/events?from={}&to={}&countries=US",
        from, to
    );

    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        )
        .header("Accept", "application/json")
        .header("Origin", "https://www.tradingview.com")
        .header("Referer", "https://www.tradingview.com/")
        .send()
        .await
        .context("Failed to fetch TradingView calendar")?;

    if !response.status().is_success() {
        anyhow::bail!("TradingView API returned {}", response.status());
    }

    let data: TradingViewResponse = response
        .json()
        .await
        .context("Failed to parse TradingView response")?;

    Ok(data.result.unwrap_or_default())
}

/// Assess BTC impact based on economic indicator and surprise
fn assess_btc_impact(title: &str, surprise_pct: f64) -> BtcImpact {
    let title_lower = title.to_lowercase();

    // INFLATION indicators - higher = hawkish = bearish for BTC
    if title_lower.contains("cpi")
        || title_lower.contains("ppi")
        || title_lower.contains("inflation")
        || title_lower.contains("pce")
    {
        return if surprise_pct > 0.0 {
            BtcImpact::Bearish
        } else {
            BtcImpact::Bullish
        };
    }

    // EMPLOYMENT indicators - stronger jobs = hawkish = bearish for BTC
    if title_lower.contains("payroll")
        || title_lower.contains("nfp")
        || title_lower.contains("employment")
        || title_lower.contains("jobs")
    {
        return if surprise_pct > 0.0 {
            BtcImpact::Bearish
        } else {
            BtcImpact::Bullish
        };
    }

    // UNEMPLOYMENT - lower = hawkish = bearish for BTC
    if title_lower.contains("unemployment") || title_lower.contains("jobless") {
        return if surprise_pct < 0.0 {
            BtcImpact::Bearish
        } else {
            BtcImpact::Bullish
        };
    }

    // HOUSING indicators - weaker = dovish = bullish for BTC
    if title_lower.contains("home")
        || title_lower.contains("housing")
        || title_lower.contains("building")
        || title_lower.contains("mortgage")
    {
        return if surprise_pct < 0.0 {
            BtcImpact::Bullish
        } else {
            BtcImpact::Bearish
        };
    }

    // GDP - weaker = dovish = bullish for BTC
    if title_lower.contains("gdp") {
        return if surprise_pct < 0.0 {
            BtcImpact::Bullish
        } else {
            BtcImpact::Bearish
        };
    }

    // RETAIL SALES - stronger = hawkish = bearish
    if title_lower.contains("retail") {
        return if surprise_pct > 0.0 {
            BtcImpact::Bearish
        } else {
            BtcImpact::Bullish
        };
    }

    BtcImpact::Neutral
}

/// Convert TradingView event to our format
fn convert_event(event: TradingViewEvent, now: DateTime<Utc>) -> Option<EconomicEvent> {
    let event_time = DateTime::parse_from_rfc3339(&event.date).ok()?;
    let event_utc = event_time.with_timezone(&Utc);
    let minutes_until = (event_utc - now).num_minutes();

    // Calculate surprise percentage
    let surprise_pct = match (event.actual, event.forecast) {
        (Some(actual), Some(forecast)) if forecast != 0.0 => {
            Some(((actual - forecast) / forecast.abs()) * 100.0)
        }
        _ => None,
    };

    // Determine BTC impact for past events with actual data
    let btc_impact = match surprise_pct {
        Some(pct) if pct.abs() > 3.0 => Some(assess_btc_impact(&event.title, pct)),
        _ => None,
    };

    // Format time string
    let time = if minutes_until.abs() < 60 {
        format!("{}m", minutes_until.abs())
    } else if minutes_until.abs() < 24 * 60 {
        format!("{}h", minutes_until.abs() / 60)
    } else {
        format!("{}d", minutes_until.abs() / (24 * 60))
    };

    Some(EconomicEvent {
        title: event.title,
        time,
        importance: EventImportance::from_int(event.importance),
        minutes_until,
        actual: event.actual,
        forecast: event.forecast,
        previous: event.previous,
        surprise_pct,
        btc_impact,
    })
}

/// Fetch economic calendar events for recap
/// Returns (recent_events, upcoming_events)
pub async fn fetch_calendar_events(
    client: &reqwest::Client,
) -> Result<(Vec<EconomicEvent>, Vec<EconomicEvent>)> {
    let now = Utc::now();

    // Fetch events from 2 days ago to 3 days ahead
    let from = (now - chrono::Duration::days(2)).format("%Y-%m-%d").to_string();
    let to = (now + chrono::Duration::days(3)).format("%Y-%m-%d").to_string();

    let events = fetch_events(client, &from, &to).await?;

    let mut recent_events = Vec::new();
    let mut upcoming_events = Vec::new();

    for event in events {
        // Only consider medium+ importance
        if event.importance < 0 {
            continue;
        }

        if let Some(converted) = convert_event(event, now) {
            if converted.minutes_until < 0 {
                // Past event (within 24h)
                if converted.minutes_until > -24 * 60 && converted.actual.is_some() {
                    recent_events.push(converted);
                }
            } else if converted.minutes_until < 48 * 60 {
                // Upcoming event (within 48h)
                upcoming_events.push(converted);
            }
        }
    }

    // Sort recent by most recent first
    recent_events.sort_by(|a, b| b.minutes_until.cmp(&a.minutes_until));

    // Sort upcoming by soonest first
    upcoming_events.sort_by(|a, b| a.minutes_until.cmp(&b.minutes_until));

    // Limit to top events
    recent_events.truncate(5);
    upcoming_events.truncate(5);

    Ok((recent_events, upcoming_events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assess_btc_impact() {
        // Higher CPI = bearish
        assert_eq!(assess_btc_impact("CPI m/m", 10.0), BtcImpact::Bearish);

        // Lower CPI = bullish
        assert_eq!(assess_btc_impact("CPI m/m", -10.0), BtcImpact::Bullish);

        // Weak housing = bullish
        assert_eq!(assess_btc_impact("New Home Sales", -20.0), BtcImpact::Bullish);
    }
}
