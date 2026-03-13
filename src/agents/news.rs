//! News Agent - RSS & Sentiment Signals
//!
//! Aggregates crypto news and produces trading signals based on:
//! - Breaking news detection (spike in mentions)
//! - Keyword sentiment analysis
//! - Source credibility weighting

use super::{Agent, Direction, Signal};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

/// News source with credibility weight
#[derive(Debug, Clone)]
pub struct NewsSource {
    pub name: String,
    pub url: String,
    pub weight: f64, // 0.0 - 1.0 credibility
}

impl NewsSource {
    pub fn new(name: &str, url: &str, weight: f64) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            weight: weight.clamp(0.0, 1.0),
        }
    }
}

/// Default crypto news sources
fn default_sources() -> Vec<NewsSource> {
    vec![
        NewsSource::new(
            "CoinDesk",
            "https://www.coindesk.com/arc/outboundfeeds/rss/",
            0.9,
        ),
        NewsSource::new(
            "Cointelegraph",
            "https://cointelegraph.com/rss",
            0.8,
        ),
        NewsSource::new(
            "Bitcoin Magazine",
            "https://bitcoinmagazine.com/feed",
            0.9,
        ),
        NewsSource::new(
            "The Block",
            "https://www.theblock.co/rss.xml",
            0.85,
        ),
        NewsSource::new(
            "Decrypt",
            "https://decrypt.co/feed",
            0.75,
        ),
    ]
}

/// Parsed news item
#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub source: String,
    pub published: Option<DateTime<Utc>>,
    pub link: String,
    pub sentiment: Sentiment,
    pub relevance: f64,
}

/// Sentiment classification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sentiment {
    Bullish,
    Bearish,
    Neutral,
}

/// Keywords for sentiment analysis
struct SentimentKeywords {
    bullish: Vec<&'static str>,
    bearish: Vec<&'static str>,
    high_impact: Vec<&'static str>,
}

impl Default for SentimentKeywords {
    fn default() -> Self {
        Self {
            bullish: vec![
                "surge", "soar", "rally", "bullish", "breakout", "all-time high", "ath",
                "adoption", "institutional", "etf approved", "etf approval", "accumulate",
                "buy", "moon", "pump", "gains", "profit", "growth", "upgrade",
                "partnership", "integration", "mainstream", "milestone", "record",
                "inflow", "demand", "scarce", "halving", "bullrun",
            ],
            bearish: vec![
                "crash", "plunge", "dump", "bearish", "sell-off", "selloff", "collapse",
                "hack", "hacked", "exploit", "vulnerability", "ban", "banned", "crackdown",
                "regulation", "sec", "lawsuit", "fraud", "scam", "ponzi", "rug pull",
                "bankruptcy", "insolvent", "liquidation", "outflow", "fear", "panic",
                "correction", "decline", "drop", "fall", "tumble", "tank",
            ],
            high_impact: vec![
                "bitcoin", "btc", "lightning", "etf", "sec", "fed", "fomc",
                "halving", "institutional", "blackrock", "fidelity", "microstrategy",
                "el salvador", "regulation", "ban", "hack", "breaking",
            ],
        }
    }
}

/// Configuration for News Agent
#[derive(Debug, Clone)]
pub struct NewsConfig {
    /// Maximum age of news to consider (in hours)
    pub max_age_hours: i64,
    /// Minimum relevance score to include
    pub min_relevance: f64,
    /// Number of recent items to analyze
    pub max_items: usize,
}

impl Default for NewsConfig {
    fn default() -> Self {
        Self {
            max_age_hours: 4,
            min_relevance: 0.3,
            max_items: 20,
        }
    }
}

/// News Agent implementation
pub struct NewsAgent {
    config: NewsConfig,
    sources: Vec<NewsSource>,
    keywords: SentimentKeywords,
    http_client: reqwest::Client,
}

impl NewsAgent {
    pub fn new(config: NewsConfig) -> Self {
        Self {
            config,
            sources: default_sources(),
            keywords: SentimentKeywords::default(),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(NewsConfig::default())
    }

    /// Fetch and parse RSS feed
    async fn fetch_feed(&self, source: &NewsSource) -> Result<Vec<NewsItem>> {
        let response = self.http_client
            .get(&source.url)
            .header("User-Agent", "Mozilla/5.0 (compatible; LNMarketsBot/1.0)")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Feed {} returned {}", source.name, response.status());
        }

        let text = response.text().await?;
        self.parse_rss(&text, source)
    }

    /// Parse RSS XML into news items
    fn parse_rss(&self, xml: &str, source: &NewsSource) -> Result<Vec<NewsItem>> {
        let mut items = Vec::new();

        // Simple RSS parsing (avoid heavy XML dependencies)
        for item_match in xml.split("<item>").skip(1) {
            let end = item_match.find("</item>").unwrap_or(item_match.len());
            let item_xml = &item_match[..end];

            let title = self.extract_tag(item_xml, "title").unwrap_or_default();
            let link = self.extract_tag(item_xml, "link").unwrap_or_default();
            let pub_date = self.extract_tag(item_xml, "pubDate");

            if title.is_empty() {
                continue;
            }

            let published = pub_date.and_then(|d| self.parse_rss_date(&d));
            let (sentiment, relevance) = self.analyze_text(&title);

            items.push(NewsItem {
                title,
                source: source.name.clone(),
                published,
                link,
                sentiment,
                relevance: relevance * source.weight,
            });
        }

        Ok(items)
    }

    /// Extract content from XML tag
    fn extract_tag(&self, xml: &str, tag: &str) -> Option<String> {
        let start_tag = format!("<{}>", tag);
        let cdata_start = format!("<{}><![CDATA[", tag);
        let end_tag = format!("</{}>", tag);

        // Try CDATA format first
        if let Some(start) = xml.find(&cdata_start) {
            let content_start = start + cdata_start.len();
            if let Some(end) = xml[content_start..].find("]]>") {
                return Some(xml[content_start..content_start + end].trim().to_string());
            }
        }

        // Regular tag
        if let Some(start) = xml.find(&start_tag) {
            let content_start = start + start_tag.len();
            if let Some(end) = xml[content_start..].find(&end_tag) {
                let content = &xml[content_start..content_start + end];
                // Strip CDATA if present
                let clean = content
                    .trim()
                    .trim_start_matches("<![CDATA[")
                    .trim_end_matches("]]>")
                    .trim();
                return Some(clean.to_string());
            }
        }

        None
    }

    /// Parse RSS date formats
    fn parse_rss_date(&self, date_str: &str) -> Option<DateTime<Utc>> {
        // RFC 2822 format (common in RSS)
        if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
            return Some(dt.with_timezone(&Utc));
        }

        // RFC 3339 format
        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Some(dt.with_timezone(&Utc));
        }

        None
    }

    /// Analyze text for sentiment and relevance
    fn analyze_text(&self, text: &str) -> (Sentiment, f64) {
        let lower = text.to_lowercase();

        let mut bullish_score: f64 = 0.0;
        let mut bearish_score: f64 = 0.0;
        let mut relevance: f64 = 0.0;

        // Check bullish keywords
        for keyword in &self.keywords.bullish {
            if lower.contains(keyword) {
                bullish_score += 1.0;
            }
        }

        // Check bearish keywords
        for keyword in &self.keywords.bearish {
            if lower.contains(keyword) {
                bearish_score += 1.0;
            }
        }

        // Check high-impact keywords for relevance
        for keyword in &self.keywords.high_impact {
            if lower.contains(keyword) {
                relevance += 0.2;
            }
        }

        relevance = relevance.min(1.0);

        let sentiment = if bullish_score > bearish_score + 0.5 {
            Sentiment::Bullish
        } else if bearish_score > bullish_score + 0.5 {
            Sentiment::Bearish
        } else {
            Sentiment::Neutral
        };

        (sentiment, relevance)
    }

    /// Fetch all feeds and aggregate news
    async fn fetch_all_news(&self) -> Vec<NewsItem> {
        let mut all_items = Vec::new();
        let now = Utc::now();
        let max_age = chrono::Duration::hours(self.config.max_age_hours);

        for source in &self.sources {
            match self.fetch_feed(source).await {
                Ok(items) => {
                    for item in items {
                        // Filter by age
                        if let Some(pub_date) = item.published {
                            if now - pub_date > max_age {
                                continue;
                            }
                        }

                        // Filter by relevance
                        if item.relevance >= self.config.min_relevance {
                            all_items.push(item);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[news] Failed to fetch {}: {}", source.name, e);
                }
            }
        }

        // Sort by publication date (newest first)
        all_items.sort_by(|a, b| {
            b.published.cmp(&a.published)
        });

        // Limit items
        all_items.truncate(self.config.max_items);
        all_items
    }

    /// Analyze aggregated news and produce signal
    fn analyze_news(&self, items: &[NewsItem]) -> Signal {
        if items.is_empty() {
            return Signal::neutral("news", "No relevant news in last few hours");
        }

        let mut bullish_count = 0;
        let mut bearish_count = 0;
        let mut total_relevance = 0.0;

        for item in items {
            match item.sentiment {
                Sentiment::Bullish => bullish_count += 1,
                Sentiment::Bearish => bearish_count += 1,
                Sentiment::Neutral => {}
            }
            total_relevance += item.relevance;
        }

        let total = items.len() as f64;
        let avg_relevance = total_relevance / total;

        // Build summary
        let top_headlines: Vec<String> = items
            .iter()
            .take(3)
            .map(|i| format!("[{}] {}", i.source, truncate(&i.title, 50)))
            .collect();

        let summary = format!(
            "{} articles | {}B/{}N/{}b | {}",
            items.len(),
            bullish_count,
            items.len() - bullish_count - bearish_count,
            bearish_count,
            top_headlines.first().unwrap_or(&"".to_string())
        );

        // Determine direction
        let bullish_ratio = bullish_count as f64 / total;
        let bearish_ratio = bearish_count as f64 / total;

        let (direction, confidence) = if bullish_ratio > 0.5 && bullish_ratio > bearish_ratio * 2.0 {
            (Direction::Long, 0.5 + avg_relevance * 0.3)
        } else if bearish_ratio > 0.5 && bearish_ratio > bullish_ratio * 2.0 {
            (Direction::Short, 0.5 + avg_relevance * 0.3)
        } else {
            (Direction::Neutral, 0.5)
        };

        Signal::new(direction, confidence, "news", &summary)
    }
}

/// Truncate string to max length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[async_trait]
impl Agent for NewsAgent {
    fn name(&self) -> &str {
        "news"
    }

    async fn analyze(&self) -> Result<Signal> {
        let items = self.fetch_all_news().await;
        Ok(self.analyze_news(&items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_bullish() {
        let agent = NewsAgent::with_defaults();
        let (sentiment, relevance) = agent.analyze_text("Bitcoin ETF approved, market surges to all-time high");
        assert_eq!(sentiment, Sentiment::Bullish);
        assert!(relevance > 0.3);
    }

    #[test]
    fn test_sentiment_bearish() {
        let agent = NewsAgent::with_defaults();
        let (sentiment, _) = agent.analyze_text("Crypto exchange hacked, Bitcoin crashes in panic sell-off");
        assert_eq!(sentiment, Sentiment::Bearish);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("Hello World", 20), "Hello World");
        assert_eq!(truncate("Hello World This Is Long", 15), "Hello World ...");
    }
}
