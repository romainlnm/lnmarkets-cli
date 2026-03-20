//! Market Recap Module
//!
//! Provides 24-48h BTC derivatives market overview including:
//! - Price action from Binance
//! - Derivatives metrics (funding, OI, L/S ratio)
//! - Fear & Greed sentiment index
//! - Economic calendar events

pub mod calendar;
pub mod derivatives;
pub mod price;
pub mod sentiment;
pub mod types;

pub use types::MarketRecap;

/// Fetch all market recap data in parallel
pub async fn fetch_market_recap() -> MarketRecap {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut recap = MarketRecap::default();

    // Fetch all data sources in parallel
    let (price_result, derivatives_result, sentiment_result, calendar_result) = tokio::join!(
        price::fetch_price_action(&client),
        derivatives::fetch_derivatives_metrics(&client),
        sentiment::fetch_fear_greed(&client),
        calendar::fetch_calendar_events(&client),
    );

    // Handle price data
    match price_result {
        Ok(price) => recap.price = Some(price),
        Err(e) => recap.errors.push(format!("Price: {}", e)),
    }

    // Handle derivatives data
    match derivatives_result {
        Ok(derivatives) => recap.derivatives = Some(derivatives),
        Err(e) => recap.errors.push(format!("Derivatives: {}", e)),
    }

    // Handle sentiment data
    match sentiment_result {
        Ok(sentiment) => recap.sentiment = Some(sentiment),
        Err(e) => recap.errors.push(format!("Sentiment: {}", e)),
    }

    // Handle calendar data
    match calendar_result {
        Ok((recent, upcoming)) => {
            recap.recent_events = recent;
            recap.upcoming_events = upcoming;
        }
        Err(e) => recap.errors.push(format!("Calendar: {}", e)),
    }

    recap
}
