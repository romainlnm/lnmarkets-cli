use serde::{Deserialize, Serialize};

/// Request to list trades by status
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ListTradesRequest {
    /// Trade status filter: 'open', 'running', 'closed', or 'canceled'
    pub status: Option<String>,

    /// Maximum number of trades to return (default: 50)
    pub limit: Option<u32>,
}

/// Request to open a new trade
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenTradeRequest {
    /// Trade side: 'buy' (long) or 'sell' (short)
    pub side: String,

    /// Position size in satoshis
    pub quantity: i64,

    /// Leverage multiplier (1-100)
    pub leverage: Option<f64>,

    /// Order type: 'market' or 'limit'
    pub order_type: Option<String>,

    /// Limit price in USD (required for limit orders)
    pub price: Option<f64>,

    /// Stop loss price in USD
    pub stoploss: Option<f64>,

    /// Take profit price in USD
    pub takeprofit: Option<f64>,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to close a trade
#[derive(Debug, Serialize, Deserialize)]
pub struct CloseTradeRequest {
    /// Trade ID to close
    pub id: String,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to update stop loss
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateStoplossRequest {
    /// Trade ID
    pub id: String,

    /// New stop loss price in USD
    pub price: f64,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to update take profit
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTakeprofitRequest {
    /// Trade ID
    pub id: String,

    /// New take profit price in USD
    pub price: f64,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to add margin to a position
#[derive(Debug, Serialize, Deserialize)]
pub struct AddMarginRequest {
    /// Trade ID
    pub id: String,

    /// Amount in satoshis to add as margin
    pub amount: i64,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to create a Lightning deposit invoice
#[derive(Debug, Serialize, Deserialize)]
pub struct DepositRequest {
    /// Amount in satoshis to deposit
    pub amount: i64,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}

/// Request to withdraw via Lightning
#[derive(Debug, Serialize, Deserialize)]
pub struct WithdrawRequest {
    /// Lightning invoice to pay
    pub invoice: String,

    /// Safety confirmation for dangerous operation
    #[serde(default)]
    pub acknowledged: Option<bool>,
}
