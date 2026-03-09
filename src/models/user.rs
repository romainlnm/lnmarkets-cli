use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "uid")]
    pub id: Option<String>,
    pub balance: Option<i64>,
    pub username: Option<String>,
    #[serde(rename = "linkingpublickey")]
    pub linking_public_key: Option<String>,
    #[serde(rename = "showLeaderboard")]
    pub show_leaderboard: Option<bool>,
    #[serde(rename = "showUsername")]
    pub show_username: Option<bool>,
    #[serde(rename = "lnurlAuth")]
    pub lnurl_auth: Option<bool>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "lastUpdate")]
    pub last_update: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "showLeaderboard")]
    pub show_leaderboard: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "showUsername")]
    pub show_username: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaderboard {
    #[serde(default)]
    pub entries: Vec<LeaderboardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: Option<i64>,
    pub username: Option<String>,
    pub pl: Option<i64>,
    pub quantity: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LeaderboardPeriod {
    Daily,
    Weekly,
    Monthly,
    #[default]
    AllTime,
}

impl LeaderboardPeriod {
    pub fn as_str(&self) -> &'static str {
        match self {
            LeaderboardPeriod::Daily => "daily",
            LeaderboardPeriod::Weekly => "weekly",
            LeaderboardPeriod::Monthly => "monthly",
            LeaderboardPeriod::AllTime => "all_time",
        }
    }
}
