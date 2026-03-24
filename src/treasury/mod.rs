//! Treasury management for AI trading
//!
//! Manages funds between LN Markets exchange and claw-cash wallet:
//! - Auto-withdraw profits to secure wallet
//! - Auto-fund when margin runs low
//! - Keep minimal funds on exchange for security

pub mod claw;

use crate::api::LnmClient;
use anyhow::Result;
use reqwest::Method;
use std::sync::atomic::{AtomicU64, Ordering};

pub use claw::ClawClient;

/// Treasury configuration
#[derive(Debug, Clone)]
pub struct TreasuryConfig {
    /// Claw-cash daemon URL
    pub claw_url: String,
    /// Minimum balance to keep on LN Markets (sats)
    pub min_exchange_balance: u64,
    /// Maximum balance on LN Markets - withdraw above this (sats)
    pub max_exchange_balance: u64,
    /// Amount to request when funding (sats)
    pub fund_amount: u64,
    /// Enable auto-withdraw
    pub auto_withdraw: bool,
    /// Enable auto-fund
    pub auto_fund: bool,
    /// Mock mode for testing (simulates claw-cash without real daemon)
    pub mock: bool,
}

impl Default for TreasuryConfig {
    fn default() -> Self {
        Self {
            claw_url: "http://127.0.0.1:9137".to_string(),
            min_exchange_balance: 10_000,      // 10k sats minimum
            max_exchange_balance: 100_000,     // 100k sats maximum
            fund_amount: 50_000,               // Request 50k sats when funding
            auto_withdraw: true,
            auto_fund: true,
            mock: false,
        }
    }
}

/// Treasury manager
pub struct Treasury {
    config: TreasuryConfig,
    claw: ClawClient,
    /// Mock balance for testing (in sats)
    mock_balance: AtomicU64,
}

impl Treasury {
    pub fn new(config: TreasuryConfig) -> Self {
        let claw = ClawClient::new(&config.claw_url);
        // Start mock with 500k sats
        let mock_balance = AtomicU64::new(500_000);
        Self { config, claw, mock_balance }
    }

    /// Check if claw-cash daemon is available
    pub async fn is_available(&self) -> bool {
        if self.config.mock {
            return true;
        }
        self.claw.health_check().await.unwrap_or(false)
    }

    /// Get claw-cash wallet balance
    pub async fn get_wallet_balance(&self) -> Result<u64> {
        if self.config.mock {
            return Ok(self.mock_balance.load(Ordering::Relaxed));
        }
        let balance = self.claw.get_balance().await?;
        Ok(balance.total)
    }

    /// Check balances and perform treasury operations if needed
    /// Returns a description of any action taken
    pub async fn manage(&self, lnm_client: &LnmClient) -> Result<Option<TreasuryAction>> {
        // Get LN Markets balance
        let lnm_balance = self.get_lnm_balance(lnm_client).await?;

        // Check if we need to withdraw (too much on exchange)
        if self.config.auto_withdraw && lnm_balance > self.config.max_exchange_balance {
            let withdraw_amount = lnm_balance - self.config.min_exchange_balance;
            return self.withdraw_to_claw(lnm_client, withdraw_amount).await;
        }

        // Check if we need to fund (too little on exchange)
        if self.config.auto_fund && lnm_balance < self.config.min_exchange_balance {
            return self.fund_from_claw(lnm_client).await;
        }

        Ok(None)
    }

    /// Get LN Markets account balance
    async fn get_lnm_balance(&self, client: &LnmClient) -> Result<u64> {
        let user: serde_json::Value = client
            .request(Method::GET, "account", None::<&()>)
            .await?;

        user["balance"]
            .as_i64()
            .map(|b| b.max(0) as u64)
            .ok_or_else(|| anyhow::anyhow!("Could not get LN Markets balance"))
    }

    /// Withdraw from LN Markets to claw-cash
    async fn withdraw_to_claw(&self, lnm_client: &LnmClient, amount: u64) -> Result<Option<TreasuryAction>> {
        if self.config.mock {
            // Mock mode: just update the simulated balance
            self.mock_balance.fetch_add(amount, Ordering::Relaxed);
            return Ok(Some(TreasuryAction::Withdraw {
                amount,
                destination: "claw-cash (mock)".to_string(),
            }));
        }

        // Create invoice from claw-cash to receive the funds
        let invoice = self.claw.create_invoice(amount).await?;

        // Pay the invoice from LN Markets
        let request = serde_json::json!({
            "invoice": invoice,
        });

        let resp: serde_json::Value = lnm_client
            .request(Method::POST, "account/withdraw", Some(&request))
            .await?;

        let withdrawn = resp["amount"].as_u64().unwrap_or(amount);

        Ok(Some(TreasuryAction::Withdraw {
            amount: withdrawn,
            destination: "claw-cash".to_string(),
        }))
    }

    /// Fund LN Markets from claw-cash
    async fn fund_from_claw(&self, lnm_client: &LnmClient) -> Result<Option<TreasuryAction>> {
        // Check claw-cash has enough funds
        let wallet_balance = self.get_wallet_balance().await?;
        if wallet_balance < self.config.fund_amount {
            return Ok(Some(TreasuryAction::InsufficientFunds {
                needed: self.config.fund_amount,
                available: wallet_balance,
            }));
        }

        if self.config.mock {
            // Mock mode: simulate the balance change but don't actually fund
            // (can't fund without real Lightning payment)
            self.mock_balance.fetch_sub(self.config.fund_amount, Ordering::Relaxed);
            return Ok(Some(TreasuryAction::Fund {
                amount: self.config.fund_amount,
                source: "claw-cash (mock - no actual deposit)".to_string(),
            }));
        }

        // Create deposit invoice from LN Markets
        let request = serde_json::json!({
            "amount": self.config.fund_amount,
        });

        let resp: serde_json::Value = lnm_client
            .request(Method::POST, "account/deposit/lightning", Some(&request))
            .await?;

        let invoice = resp["paymentRequest"]
            .as_str()
            .or_else(|| resp["invoice"].as_str())
            .ok_or_else(|| anyhow::anyhow!("No invoice in deposit response"))?;

        // Pay the invoice from claw-cash
        self.claw.pay_invoice(invoice).await?;

        Ok(Some(TreasuryAction::Fund {
            amount: self.config.fund_amount,
            source: "claw-cash".to_string(),
        }))
    }

    /// Print treasury status
    pub async fn print_status(&self, lnm_client: Option<&LnmClient>) {
        let claw_available = self.is_available().await;
        let mode_label = if self.config.mock { " (mock)" } else { "" };

        print!("  \x1b[35m[TREASURY]\x1b[0m claw-cash{}: ", mode_label);
        if claw_available {
            if let Ok(balance) = self.get_wallet_balance().await {
                print!("\x1b[32m{} sats\x1b[0m", balance);
            } else {
                print!("\x1b[32mconnected\x1b[0m");
            }
        } else {
            print!("\x1b[31moffline\x1b[0m");
        }

        if let Some(client) = lnm_client {
            if let Ok(lnm_bal) = self.get_lnm_balance(client).await {
                print!(" | exchange: {} sats", lnm_bal);

                // Show thresholds
                if lnm_bal < self.config.min_exchange_balance {
                    print!(" \x1b[33m(low)\x1b[0m");
                } else if lnm_bal > self.config.max_exchange_balance {
                    print!(" \x1b[33m(high)\x1b[0m");
                }
            }
        }
        println!();
    }
}

/// Treasury action taken
#[derive(Debug, Clone)]
pub enum TreasuryAction {
    /// Withdrew funds to claw-cash
    Withdraw {
        amount: u64,
        destination: String,
    },
    /// Funded from claw-cash
    Fund {
        amount: u64,
        source: String,
    },
    /// Not enough funds in claw-cash
    InsufficientFunds {
        needed: u64,
        available: u64,
    },
}

impl std::fmt::Display for TreasuryAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreasuryAction::Withdraw { amount, destination } => {
                write!(f, "Withdrew {} sats to {}", amount, destination)
            }
            TreasuryAction::Fund { amount, source } => {
                write!(f, "Funded {} sats from {}", amount, source)
            }
            TreasuryAction::InsufficientFunds { needed, available } => {
                write!(f, "Insufficient funds: need {} sats, have {}", needed, available)
            }
        }
    }
}
