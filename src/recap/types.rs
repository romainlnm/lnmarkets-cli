//! Data structures for market recap

use serde::{Deserialize, Serialize};

/// Price action metrics for BTC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceAction {
    pub current: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub change_24h_pct: f64,
    pub high_pct: f64,  // % from current to high
    pub low_pct: f64,   // % from current to low
}

/// Derivatives market metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivativesMetrics {
    /// Funding rate as percentage (e.g., 0.01 = 0.01%)
    pub funding_rate: f64,
    /// Funding rate interpretation
    pub funding_sentiment: FundingSentiment,
    /// Open interest in USD
    pub open_interest: f64,
    /// Long/Short account ratio
    pub long_short_ratio: f64,
    /// Interpretation of L/S ratio
    pub ls_sentiment: LongShortSentiment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FundingSentiment {
    Bearish,
    Neutral,
    Bullish,
}

impl FundingSentiment {
    pub fn from_rate(rate: f64) -> Self {
        if rate > 0.03 {
            Self::Bullish // High positive funding = longs paying
        } else if rate < -0.01 {
            Self::Bearish // Negative funding = shorts paying
        } else {
            Self::Neutral
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bearish => "bearish",
            Self::Neutral => "neutral",
            Self::Bullish => "bullish",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LongShortSentiment {
    ShortsDominant,
    Balanced,
    LongsDominant,
}

impl LongShortSentiment {
    pub fn from_ratio(ratio: f64) -> Self {
        if ratio > 1.1 {
            Self::LongsDominant
        } else if ratio < 0.9 {
            Self::ShortsDominant
        } else {
            Self::Balanced
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::ShortsDominant => "shorts dominant",
            Self::Balanced => "balanced",
            Self::LongsDominant => "longs dominant",
        }
    }
}

/// Fear & Greed Index data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentData {
    pub value: u32,
    pub label: String,
    pub previous_value: Option<u32>,
}

impl SentimentData {
    pub fn change_indicator(&self) -> &'static str {
        match self.previous_value {
            Some(prev) if self.value > prev => "^",
            Some(prev) if self.value < prev => "v",
            _ => "=",
        }
    }
}

/// Economic calendar event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicEvent {
    pub title: String,
    pub time: String,
    pub importance: EventImportance,
    /// Minutes until event (negative = past)
    pub minutes_until: i64,
    /// For past events: actual vs forecast
    pub actual: Option<f64>,
    pub forecast: Option<f64>,
    pub previous: Option<f64>,
    /// Surprise percentage if actual != forecast
    pub surprise_pct: Option<f64>,
    /// Interpretation for BTC
    pub btc_impact: Option<BtcImpact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventImportance {
    Low,
    Medium,
    High,
}

impl EventImportance {
    pub fn from_int(i: i32) -> Self {
        match i {
            i if i >= 1 => Self::High,
            0 => Self::Medium,
            _ => Self::Low,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::High => "[!]",
            Self::Medium => "[*]",
            Self::Low => "[-]",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BtcImpact {
    Bullish,
    Bearish,
    Neutral,
}

impl BtcImpact {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Bullish => "BULLISH",
            Self::Bearish => "BEARISH",
            Self::Neutral => "NEUTRAL",
        }
    }
}

/// Aggregated market recap data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketRecap {
    pub price: Option<PriceAction>,
    pub derivatives: Option<DerivativesMetrics>,
    pub sentiment: Option<SentimentData>,
    pub recent_events: Vec<EconomicEvent>,
    pub upcoming_events: Vec<EconomicEvent>,
    /// Errors encountered during data fetch
    pub errors: Vec<String>,
}

impl Default for MarketRecap {
    fn default() -> Self {
        Self {
            price: None,
            derivatives: None,
            sentiment: None,
            recent_events: Vec::new(),
            upcoming_events: Vec::new(),
            errors: Vec::new(),
        }
    }
}
