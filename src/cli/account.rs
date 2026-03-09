use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::Serialize;
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::{User, LeaderboardPeriod, LeaderboardEntry};
use super::output::{print_single, print_list, print_success, format_sats};

#[derive(Subcommand)]
pub enum AccountCommands {
    /// Get account information and balance
    Info,

    /// Get current balance
    Balance,

    /// Update account settings
    Update {
        /// Set username
        #[arg(long)]
        username: Option<String>,

        /// Show on leaderboard
        #[arg(long)]
        show_leaderboard: Option<bool>,

        /// Show username publicly
        #[arg(long)]
        show_username: Option<bool>,
    },

    /// View the leaderboard
    Leaderboard {
        /// Time period
        #[arg(short, long, default_value = "all-time")]
        period: LeaderboardPeriod,

        /// Maximum entries to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
}

#[derive(Debug, Tabled, Serialize)]
pub struct UserRow {
    #[tabled(rename = "Balance")]
    pub balance: String,
    #[tabled(rename = "Username")]
    pub username: String,
    #[tabled(rename = "Show Leaderboard")]
    pub show_leaderboard: String,
}

impl From<User> for UserRow {
    fn from(u: User) -> Self {
        Self {
            balance: u.balance.map(format_sats).unwrap_or_else(|| "-".to_string()),
            username: u.username.unwrap_or_else(|| "-".to_string()),
            show_leaderboard: u.show_leaderboard
                .map(|b| if b { "Yes" } else { "No" }.to_string())
                .unwrap_or_else(|| "-".to_string()),
        }
    }
}

#[derive(Debug, Tabled, Serialize)]
pub struct BalanceRow {
    #[tabled(rename = "Balance (sats)")]
    pub sats: i64,
    #[tabled(rename = "Balance (BTC)")]
    pub btc: String,
}

#[derive(Debug, Tabled, Serialize)]
pub struct LeaderboardRow {
    #[tabled(rename = "Rank")]
    pub rank: String,
    #[tabled(rename = "Username")]
    pub username: String,
    #[tabled(rename = "P/L")]
    pub pl: String,
    #[tabled(rename = "Volume")]
    pub quantity: String,
}

impl From<LeaderboardEntry> for LeaderboardRow {
    fn from(e: LeaderboardEntry) -> Self {
        Self {
            rank: e.rank.map(|r| format!("#{}", r)).unwrap_or_else(|| "-".to_string()),
            username: e.username.unwrap_or_else(|| "Anonymous".to_string()),
            pl: e.pl.map(format_sats).unwrap_or_else(|| "-".to_string()),
            quantity: e.quantity.map(format_sats).unwrap_or_else(|| "-".to_string()),
        }
    }
}

impl AccountCommands {
    pub async fn execute(&self, client: &LnmClient, format: OutputFormat) -> Result<()> {
        match self {
            Self::Info => {
                let user: User = client.request(Method::GET, "user", None::<&()>).await?;
                print_single(UserRow::from(user), format)?;
            }

            Self::Balance => {
                let user: User = client.request(Method::GET, "user", None::<&()>).await?;
                let balance = user.balance.unwrap_or(0);

                match format {
                    OutputFormat::Json => {
                        println!("{{\"balance\":{}}}", balance);
                    }
                    OutputFormat::JsonPretty => {
                        println!("{{\n  \"balance\": {}\n}}", balance);
                    }
                    OutputFormat::Table => {
                        let row = BalanceRow {
                            sats: balance,
                            btc: format!("{:.8}", balance as f64 / 100_000_000.0),
                        };
                        print_single(row, format)?;
                    }
                }
            }

            Self::Update { username, show_leaderboard, show_username } => {
                if username.is_none() && show_leaderboard.is_none() && show_username.is_none() {
                    anyhow::bail!("At least one update option must be provided");
                }

                #[derive(Serialize)]
                struct UpdateRequest {
                    #[serde(skip_serializing_if = "Option::is_none")]
                    username: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none", rename = "showLeaderboard")]
                    show_leaderboard: Option<bool>,
                    #[serde(skip_serializing_if = "Option::is_none", rename = "showUsername")]
                    show_username: Option<bool>,
                }

                let request = UpdateRequest {
                    username: username.clone(),
                    show_leaderboard: *show_leaderboard,
                    show_username: *show_username,
                };

                let _: serde_json::Value = client.request(Method::PUT, "user", Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        let user: User = client.request(Method::GET, "user", None::<&()>).await?;
                        print_single(UserRow::from(user), format)?;
                    }
                    OutputFormat::Table => {
                        print_success("Account updated");
                    }
                }
            }

            Self::Leaderboard { period, limit } => {
                let path = format!("futures/leaderboard?period={}&limit={}", period.as_str(), limit);
                let entries: Vec<LeaderboardEntry> = client.public_request(Method::GET, &path).await?;
                let rows: Vec<LeaderboardRow> = entries.into_iter().map(LeaderboardRow::from).collect();
                print_list(rows, format)?;
            }
        }

        Ok(())
    }
}
