use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::Serialize;
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::{
    Trade, Side, OrderType, NewTradeRequest, UpdateTradeRequest,
    AddMarginRequest, CashInRequest,
};
use super::output::{print_single, print_list, print_success, format_sats, format_price};

#[derive(Subcommand)]
pub enum FuturesCommands {
    /// List trades (open, running, or closed)
    List {
        /// Filter by status: open, running, closed
        #[arg(short, long)]
        status: Option<String>,

        /// Maximum number of trades
        #[arg(short, long, default_value = "50")]
        limit: u32,
    },

    /// Get a specific trade by ID
    Get {
        /// Trade ID
        id: String,
    },

    /// Open a new futures position
    Open {
        /// Position side (buy/sell, or long/short)
        #[arg(short, long)]
        side: Side,

        /// Order type (market or limit)
        #[arg(short = 't', long, default_value = "market")]
        order_type: OrderType,

        /// Position size in satoshis
        #[arg(short, long)]
        quantity: i64,

        /// Leverage (1-100)
        #[arg(short, long, default_value = "1")]
        leverage: f64,

        /// Limit price (required for limit orders)
        #[arg(short, long)]
        price: Option<f64>,

        /// Stop loss price
        #[arg(long)]
        stoploss: Option<f64>,

        /// Take profit price
        #[arg(long)]
        takeprofit: Option<f64>,
    },

    /// Update an existing trade (stop loss, take profit)
    Update {
        /// Trade ID
        id: String,

        /// New stop loss price
        #[arg(long)]
        stoploss: Option<f64>,

        /// New take profit price
        #[arg(long)]
        takeprofit: Option<f64>,
    },

    /// Add margin to a running position
    AddMargin {
        /// Trade ID
        id: String,

        /// Amount in satoshis to add
        #[arg(short, long)]
        amount: i64,
    },

    /// Cash in (partial close) a profitable position
    Cashin {
        /// Trade ID
        id: String,

        /// Amount in satoshis to cash in
        #[arg(short, long)]
        amount: i64,
    },

    /// Close a running trade
    Close {
        /// Trade ID
        id: String,
    },

    /// Close all running trades
    CloseAll,

    /// Cancel a pending (open) order
    Cancel {
        /// Trade ID
        id: String,
    },

    /// Cancel all pending orders
    CancelAll,
}

#[derive(Debug, Tabled, Serialize)]
pub struct TradeRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Side")]
    pub side: String,
    #[tabled(rename = "Type")]
    pub order_type: String,
    #[tabled(rename = "Quantity")]
    pub quantity: String,
    #[tabled(rename = "Leverage")]
    pub leverage: String,
    #[tabled(rename = "Entry")]
    pub entry_price: String,
    #[tabled(rename = "P/L")]
    pub pl: String,
    #[tabled(rename = "Margin")]
    pub margin: String,
}

impl From<Trade> for TradeRow {
    fn from(t: Trade) -> Self {
        let side = match t.side.as_str() {
            "b" => "Long",
            "s" => "Short",
            other => other,
        };
        let order_type = match t.order_type.as_str() {
            "m" => "Market",
            "l" => "Limit",
            other => other,
        };

        Self {
            id: t.id.chars().take(8).collect::<String>() + "...",
            side: side.to_string(),
            order_type: order_type.to_string(),
            quantity: format_sats(t.quantity),
            leverage: format!("{}x", t.leverage),
            entry_price: t.entry_price.map(format_price).unwrap_or_else(|| "-".to_string()),
            pl: t.pl.map(format_sats).unwrap_or_else(|| "-".to_string()),
            margin: t.margin.map(format_sats).unwrap_or_else(|| "-".to_string()),
        }
    }
}

impl FuturesCommands {
    pub async fn execute(&self, client: &LnmClient, format: OutputFormat) -> Result<()> {
        match self {
            Self::List { status, limit } => {
                let mut path = format!("futures/trades?limit={}", limit);
                if let Some(s) = status {
                    path.push_str(&format!("&type={}", s));
                }

                let trades: Vec<Trade> = client.request(Method::GET, &path, None::<&()>).await?;
                let rows: Vec<TradeRow> = trades.into_iter().map(TradeRow::from).collect();
                print_list(rows, format)?;
            }

            Self::Get { id } => {
                let path = format!("futures/trades/{}", id);
                let trade: Trade = client.request(Method::GET, &path, None::<&()>).await?;
                print_single(TradeRow::from(trade), format)?;
            }

            Self::Open {
                side,
                order_type,
                quantity,
                leverage,
                price,
                stoploss,
                takeprofit,
            } => {
                if *order_type == OrderType::Limit && price.is_none() {
                    anyhow::bail!("Price is required for limit orders");
                }

                let request = NewTradeRequest {
                    side: side.as_str().to_string(),
                    order_type: order_type.as_str().to_string(),
                    quantity: *quantity,
                    leverage: *leverage,
                    price: *price,
                    stoploss: *stoploss,
                    takeprofit: *takeprofit,
                };

                let trade: Trade = client.request(Method::POST, "futures/trades", Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        print_single(TradeRow::from(trade), format)?;
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Position opened: {}", trade.id));
                        print_single(TradeRow::from(trade), format)?;
                    }
                }
            }

            Self::Update { id, stoploss, takeprofit } => {
                if stoploss.is_none() && takeprofit.is_none() {
                    anyhow::bail!("At least one of --stoploss or --takeprofit must be provided");
                }

                let path = format!("futures/trades/{}", id);
                let request = UpdateTradeRequest {
                    stoploss: *stoploss,
                    takeprofit: *takeprofit,
                };

                let trade: Trade = client.request(Method::PUT, &path, Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        print_single(TradeRow::from(trade), format)?;
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Trade {} updated", id));
                    }
                }
            }

            Self::AddMargin { id, amount } => {
                let path = format!("futures/trades/{}/margin", id);
                let request = AddMarginRequest { amount: *amount };

                let trade: Trade = client.request(Method::POST, &path, Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        print_single(TradeRow::from(trade), format)?;
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Added {} to trade {}", format_sats(*amount), id));
                    }
                }
            }

            Self::Cashin { id, amount } => {
                let path = format!("futures/trades/{}/cash-in", id);
                let request = CashInRequest { amount: *amount };

                let result: serde_json::Value = client.request(Method::POST, &path, Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Cashed in {} from trade {}", format_sats(*amount), id));
                    }
                }
            }

            Self::Close { id } => {
                let path = format!("futures/trades/{}", id);
                let result: serde_json::Value = client.request(Method::DELETE, &path, None::<&()>).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Trade {} closed", id));
                    }
                }
            }

            Self::CloseAll => {
                let result: serde_json::Value = client.request(Method::DELETE, "futures/trades/all", None::<&()>).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success("All running trades closed");
                    }
                }
            }

            Self::Cancel { id } => {
                let path = format!("futures/trades/{}/cancel", id);
                let result: serde_json::Value = client.request(Method::POST, &path, None::<&()>).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Order {} canceled", id));
                    }
                }
            }

            Self::CancelAll => {
                let result: serde_json::Value = client.request(Method::DELETE, "futures/trades/all/cancel", None::<&()>).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success("All pending orders canceled");
                    }
                }
            }
        }

        Ok(())
    }
}
