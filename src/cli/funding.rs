use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::Serialize;
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::{
    BitcoinAddress, Deposit, LightningInvoice, Withdrawal,
    NewDepositRequest, NewWithdrawalRequest,
};
use super::output::{print_single, print_list, print_success, print_info, format_sats};

#[derive(Subcommand)]
pub enum FundingCommands {
    /// Generate a new Bitcoin deposit address
    NewAddress,

    /// List Bitcoin deposit addresses
    Addresses,

    /// Create a Lightning deposit invoice
    Deposit {
        /// Amount in satoshis
        #[arg(short, long)]
        amount: i64,
    },

    /// List deposit history
    Deposits {
        /// Maximum number of deposits
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },

    /// Withdraw via Lightning (pay invoice)
    Withdraw {
        /// Amount in satoshis
        #[arg(short, long)]
        amount: i64,

        /// Lightning invoice to pay
        #[arg(short, long)]
        invoice: String,
    },

    /// Withdraw to Bitcoin address (on-chain)
    WithdrawOnchain {
        /// Amount in satoshis
        #[arg(short, long)]
        amount: i64,

        /// Bitcoin address
        #[arg(long)]
        address: String,
    },

    /// List withdrawal history
    Withdrawals {
        /// Maximum number of withdrawals
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },
}

#[derive(Debug, Tabled, Serialize)]
pub struct AddressRow {
    #[tabled(rename = "Address")]
    pub address: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

impl From<BitcoinAddress> for AddressRow {
    fn from(a: BitcoinAddress) -> Self {
        Self {
            address: a.address,
            created_at: a.created_at.unwrap_or_else(|| "-".to_string()),
        }
    }
}

#[derive(Debug, Tabled, Serialize)]
pub struct DepositRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Amount")]
    pub amount: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Type")]
    pub deposit_type: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

impl From<Deposit> for DepositRow {
    fn from(d: Deposit) -> Self {
        Self {
            id: d.id.chars().take(8).collect::<String>() + "...",
            amount: d.amount.map(format_sats).unwrap_or_else(|| "-".to_string()),
            status: d.status.unwrap_or_else(|| "-".to_string()),
            deposit_type: d.deposit_type.unwrap_or_else(|| "-".to_string()),
            created_at: d.created_at.unwrap_or_else(|| "-".to_string()),
        }
    }
}

#[derive(Debug, Tabled, Serialize)]
pub struct WithdrawalRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Amount")]
    pub amount: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Type")]
    pub withdrawal_type: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

impl From<Withdrawal> for WithdrawalRow {
    fn from(w: Withdrawal) -> Self {
        Self {
            id: w.id.chars().take(8).collect::<String>() + "...",
            amount: w.amount.map(format_sats).unwrap_or_else(|| "-".to_string()),
            status: w.status.unwrap_or_else(|| "-".to_string()),
            withdrawal_type: w.withdrawal_type.unwrap_or_else(|| "-".to_string()),
            created_at: w.created_at.unwrap_or_else(|| "-".to_string()),
        }
    }
}

impl FundingCommands {
    pub async fn execute(&self, client: &LnmClient, format: OutputFormat) -> Result<()> {
        match self {
            Self::NewAddress => {
                let address: BitcoinAddress = client.request(Method::POST, "user/bitcoin-address", None::<&()>).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        print_single(AddressRow::from(address), format)?;
                    }
                    OutputFormat::Table => {
                        print_success("New Bitcoin address generated:");
                        println!("{}", address.address);
                    }
                }
            }

            Self::Addresses => {
                let addresses: Vec<BitcoinAddress> = client.request(Method::GET, "user/bitcoin-addresses", None::<&()>).await?;
                let rows: Vec<AddressRow> = addresses.into_iter().map(AddressRow::from).collect();
                print_list(rows, format)?;
            }

            Self::Deposit { amount } => {
                let request = NewDepositRequest { amount: *amount };
                let invoice: LightningInvoice = client.request(Method::POST, "user/deposit", Some(&request)).await?;

                match format {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string(&invoice)?);
                    }
                    OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&invoice)?);
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Lightning invoice created for {}", format_sats(*amount)));
                        print_info("Pay this invoice to deposit funds:");
                        println!("\n{}\n", invoice.payment_request);
                        if let Some(expires) = invoice.expires_at {
                            print_info(&format!("Expires: {}", expires));
                        }
                    }
                }
            }

            Self::Deposits { limit } => {
                let path = format!("user/deposits?limit={}", limit);
                let deposits: Vec<Deposit> = client.request(Method::GET, &path, None::<&()>).await?;
                let rows: Vec<DepositRow> = deposits.into_iter().map(DepositRow::from).collect();
                print_list(rows, format)?;
            }

            Self::Withdraw { amount, invoice } => {
                let request = NewWithdrawalRequest {
                    amount: *amount,
                    invoice: Some(invoice.clone()),
                    address: None,
                };

                let result: serde_json::Value = client.request(Method::POST, "user/withdraw", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Withdrawal of {} initiated", format_sats(*amount)));
                    }
                }
            }

            Self::WithdrawOnchain { amount, address } => {
                let request = NewWithdrawalRequest {
                    amount: *amount,
                    invoice: None,
                    address: Some(address.clone()),
                };

                let result: serde_json::Value = client.request(Method::POST, "user/withdraw", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("On-chain withdrawal of {} to {} initiated", format_sats(*amount), address));
                    }
                }
            }

            Self::Withdrawals { limit } => {
                let path = format!("user/withdrawals?limit={}", limit);
                let withdrawals: Vec<Withdrawal> = client.request(Method::GET, &path, None::<&()>).await?;
                let rows: Vec<WithdrawalRow> = withdrawals.into_iter().map(WithdrawalRow::from).collect();
                print_list(rows, format)?;
            }
        }

        Ok(())
    }
}
