use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "b",
            Side::Sell => "s",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Market => "m",
            OrderType::Limit => "l",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TradeStatus {
    Open,
    Running,
    Closed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    #[serde(rename = "uid")]
    pub user_id: Option<String>,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub quantity: i64,
    pub leverage: f64,
    #[serde(rename = "stoploss")]
    pub stop_loss: Option<f64>,
    #[serde(rename = "takeprofit")]
    pub take_profit: Option<f64>,
    pub price: Option<f64>,
    #[serde(rename = "entryPrice")]
    pub entry_price: Option<f64>,
    #[serde(rename = "exitPrice")]
    pub exit_price: Option<f64>,
    pub margin: Option<i64>,
    #[serde(rename = "marginWiCf")]
    pub margin_with_cf: Option<i64>,
    pub pl: Option<i64>,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: Option<f64>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "openAt")]
    pub open_at: Option<String>,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<String>,
    #[serde(rename = "lastUpdate")]
    pub last_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeList {
    #[serde(default)]
    pub trades: Vec<Trade>,
    #[serde(default)]
    pub total: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewTradeRequest {
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub quantity: i64,
    pub leverage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stoploss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeprofit: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateTradeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stoploss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeprofit: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddMarginRequest {
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseTradeResponse {
    pub id: String,
    pub pl: Option<i64>,
    #[serde(rename = "exitPrice")]
    pub exit_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashInRequest {
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashInResponse {
    pub id: String,
    pub amount: i64,
}
