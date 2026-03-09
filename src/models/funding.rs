use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinAddress {
    pub address: String,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub id: String,
    pub amount: Option<i64>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub deposit_type: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "confirmedAt")]
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositList {
    #[serde(default)]
    pub deposits: Vec<Deposit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightningInvoice {
    #[serde(rename = "paymentRequest")]
    pub payment_request: String,
    pub id: Option<String>,
    pub amount: Option<i64>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewDepositRequest {
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub id: String,
    pub amount: Option<i64>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub withdrawal_type: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "confirmedAt")]
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalList {
    #[serde(default)]
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewWithdrawalRequest {
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalResponse {
    pub id: String,
    pub amount: Option<i64>,
    pub status: Option<String>,
}
