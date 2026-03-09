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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexHistory {
    pub time: i64,
    pub index: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    #[serde(rename = "quantityMin")]
    pub quantity_min: Option<i64>,
    #[serde(rename = "quantityMax")]
    pub quantity_max: Option<i64>,
    #[serde(rename = "leverageMin")]
    pub leverage_min: Option<f64>,
    #[serde(rename = "leverageMax")]
    pub leverage_max: Option<f64>,
    #[serde(rename = "maintenanceMargin")]
    pub maintenance_margin: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarryingFees {
    pub time: i64,
    pub rate: f64,
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

impl Resolution {
    pub fn to_minutes(&self) -> u32 {
        match self {
            Resolution::M1 => 1,
            Resolution::M5 => 5,
            Resolution::M15 => 15,
            Resolution::M30 => 30,
            Resolution::H1 => 60,
            Resolution::H4 => 240,
            Resolution::D1 => 1440,
            Resolution::W1 => 10080,
        }
    }
}
