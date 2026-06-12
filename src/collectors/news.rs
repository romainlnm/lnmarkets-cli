//! News collector — pulls headlines from a fixed set of RSS feeds.
//! No sentiment scoring — keyword bags were the weakest signal in the old
//! system. The LLM reads the actual headlines and reasons about them.

use super::DataCollector;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct NewsSource {
    pub name: String,
    pub url: String,
}

impl NewsSource {
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
        }
    }
}

fn default_sources() -> Vec<NewsSource> {
    vec![
        NewsSource::new(
            "CNBC Top News",
            "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=100003114",
        ),
        NewsSource::new("ZeroHedge", "https://feeds.feedburner.com/zerohedge/feed"),
        NewsSource::new(
            "MarketWatch",
            "https://feeds.marketwatch.com/marketwatch/topstories/",
        ),
        NewsSource::new("Yahoo Finance", "https://finance.yahoo.com/news/rssindex"),
        NewsSource::new("CoinDesk", "https://www.coindesk.com/arc/outboundfeeds/rss/"),
        NewsSource::new("Cointelegraph", "https://cointelegraph.com/rss"),
        NewsSource::new("Bitcoin Magazine", "https://bitcoinmagazine.com/feed"),
        NewsSource::new("Decrypt", "https://decrypt.co/feed"),
        NewsSource::new("CryptoSlate", "https://cryptoslate.com/feed/"),
        NewsSource::new("The Block", "https://www.theblock.co/rss.xml"),
    ]
}

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub source: String,
    pub published: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewsConfig {
    pub max_age_hours: i64,
    pub max_items: usize,
    pub cache_ttl_mins: i64,
}

impl Default for NewsConfig {
    fn default() -> Self {
        Self {
            max_age_hours: 2,
            max_items: 30,
            cache_ttl_mins: 2,
        }
    }
}

struct NewsCache {
    items: Vec<NewsItem>,
    fetched_at: DateTime<Utc>,
}

pub struct NewsAgent {
    config: NewsConfig,
    sources: Vec<NewsSource>,
    http_client: reqwest::Client,
    cache: Arc<RwLock<Option<NewsCache>>>,
}

impl NewsAgent {
    pub fn new(config: NewsConfig) -> Self {
        Self {
            config,
            sources: default_sources(),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(NewsConfig::default())
    }

    async fn is_cache_valid(&self) -> bool {
        let cache = self.cache.read().await;
        if let Some(ref c) = *cache {
            (Utc::now() - c.fetched_at).num_minutes() < self.config.cache_ttl_mins
        } else {
            false
        }
    }

    async fn get_news(&self) -> Vec<NewsItem> {
        if self.is_cache_valid().await {
            if let Some(ref c) = *self.cache.read().await {
                return c.items.clone();
            }
        }
        let items = self.fetch_all_news().await;
        *self.cache.write().await = Some(NewsCache {
            items: items.clone(),
            fetched_at: Utc::now(),
        });
        items
    }

    async fn fetch_feed(&self, source: &NewsSource) -> Result<Vec<NewsItem>> {
        let response = self
            .http_client
            .get(&source.url)
            .header("User-Agent", "Mozilla/5.0 (compatible; LNMarketsBot/1.0)")
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("Feed {} returned {}", source.name, response.status());
        }
        let text = response.text().await?;
        Ok(self.parse_rss(&text, source))
    }

    fn parse_rss(&self, xml: &str, source: &NewsSource) -> Vec<NewsItem> {
        let mut items = Vec::new();
        for item_match in xml.split("<item>").skip(1) {
            let end = item_match.find("</item>").unwrap_or(item_match.len());
            let item_xml = &item_match[..end];
            let title = extract_tag(item_xml, "title").unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let published = extract_tag(item_xml, "pubDate").and_then(|d| parse_rss_date(&d));
            items.push(NewsItem {
                title,
                source: source.name.clone(),
                published,
            });
        }
        items
    }

    async fn fetch_all_news(&self) -> Vec<NewsItem> {
        let now = Utc::now();
        let max_age = chrono::Duration::hours(self.config.max_age_hours);

        // Fetch all feeds concurrently — serially, ten slow feeds at a 10s
        // timeout each could eat a whole daemon cycle.
        let results = futures_util::future::join_all(
            self.sources
                .iter()
                .map(|source| async move { (source.name.clone(), self.fetch_feed(source).await) }),
        )
        .await;

        let mut all_items = Vec::new();
        for (name, result) in results {
            match result {
                Ok(items) => {
                    for item in items {
                        if let Some(pub_date) = item.published {
                            if now - pub_date > max_age {
                                continue;
                            }
                        }
                        all_items.push(item);
                    }
                }
                Err(e) => {
                    eprintln!("[news] failed to fetch {}: {}", name, e);
                }
            }
        }
        all_items.sort_by(|a, b| b.published.cmp(&a.published));
        all_items.truncate(self.config.max_items);
        all_items
    }
}

#[async_trait]
impl DataCollector for NewsAgent {
    fn name(&self) -> &str {
        "news"
    }

    async fn collect(&self) -> Result<Value> {
        let items = self.get_news().await;
        let now = Utc::now();
        let headlines: Vec<Value> = items
            .iter()
            .map(|i| {
                let age_min = i
                    .published
                    .map(|p| (now - p).num_minutes())
                    .unwrap_or(-1);
                json!({
                    "title": i.title,
                    "source": i.source,
                    "minutes_ago": age_min,
                })
            })
            .collect();

        Ok(json!({
            "headlines": headlines,
            "count": items.len(),
            "lookback_hours": self.config.max_age_hours,
        }))
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let cdata_start = format!("<{}><![CDATA[", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = xml.find(&cdata_start) {
        let content_start = start + cdata_start.len();
        if let Some(end) = xml[content_start..].find("]]>") {
            return Some(xml[content_start..content_start + end].trim().to_string());
        }
    }

    if let Some(start) = xml.find(&start_tag) {
        let content_start = start + start_tag.len();
        if let Some(end) = xml[content_start..].find(&end_tag) {
            let content = &xml[content_start..content_start + end];
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

fn parse_rss_date(date_str: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Some(dt.with_timezone(&Utc));
    }
    None
}
