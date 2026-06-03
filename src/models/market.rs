use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub time: Option<String>,
    pub index: f64,
    #[serde(rename = "lastPrice")]
    pub last_price: Option<f64>,
    #[serde(default)]
    pub prices: Vec<PriceLevel>,
    #[serde(rename = "fundingRate")]
    pub funding_rate: Option<f64>,
    #[serde(rename = "fundingTime")]
    pub funding_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    #[serde(rename = "askPrice")]
    pub ask_price: f64,
    #[serde(rename = "bidPrice")]
    pub bid_price: f64,
    #[serde(rename = "minSize")]
    pub min_size: i64,
    #[serde(rename = "maxSize")]
    pub max_size: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    #[serde(rename = "1")]
    M1,
    #[serde(rename = "5")]
    M5,
    #[serde(rename = "15")]
    M15,
    #[serde(rename = "30")]
    M30,
    #[serde(rename = "60")]
    H1,
    #[serde(rename = "240")]
    H4,
    #[serde(rename = "1440")]
    #[default]
    D1,
    #[serde(rename = "10080")]
    W1,
}
