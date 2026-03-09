use anyhow::Result;
use clap::Subcommand;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tabled::Tabled;

use crate::api::LnmClient;
use crate::config::OutputFormat;
use crate::models::{Side, OrderType};
use super::output::{print_list, print_success, format_sats, format_price};

#[derive(Subcommand)]
pub enum FuturesCommands {
    /// List trades (open, running, or closed)
    List {
        /// Filter by status: open, running, closed
        #[arg(short, long, default_value = "running")]
        status: String,

        /// Maximum number of trades
        #[arg(short, long, default_value = "50")]
        limit: u32,
    },

    /// Open a new futures position
    Open {
        /// Position side (buy/sell)
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

    /// Update stop loss for a trade
    Stoploss {
        /// Trade ID
        id: String,

        /// New stop loss price
        #[arg(short, long)]
        price: f64,
    },

    /// Update take profit for a trade
    Takeprofit {
        /// Trade ID
        id: String,

        /// New take profit price
        #[arg(short, long)]
        price: f64,
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

    /// Cancel a pending (open) order
    Cancel {
        /// Trade ID
        id: String,
    },

    /// Cancel all pending orders
    CancelAll,
}

// v3 API trade response structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Trade {
    pub id: String,
    #[serde(default)]
    pub side: String,
    #[serde(rename = "type", default)]
    pub order_type: String,
    #[serde(default)]
    pub quantity: i64,
    #[serde(default)]
    pub leverage: f64,
    pub price: Option<f64>,
    #[serde(rename = "entryPrice")]
    pub entry_price: Option<f64>,
    #[serde(rename = "exitPrice")]
    pub exit_price: Option<f64>,
    pub margin: Option<i64>,
    pub pl: Option<i64>,
    pub stoploss: Option<f64>,
    pub takeprofit: Option<f64>,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: Option<f64>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
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
            id: if t.id.len() > 12 {
                t.id.chars().take(8).collect::<String>() + "..."
            } else {
                t.id.clone()
            },
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
                let path = match status.as_str() {
                    "open" => format!("futures/isolated/trades/open?limit={}", limit),
                    "running" => format!("futures/isolated/trades/running?limit={}", limit),
                    "closed" => format!("futures/isolated/trades/closed?limit={}", limit),
                    "canceled" => format!("futures/isolated/trades/canceled?limit={}", limit),
                    _ => anyhow::bail!("Invalid status. Use: open, running, closed, or canceled"),
                };

                let trades: Vec<Trade> = client.request(Method::GET, &path, None::<&()>).await?;
                let rows: Vec<TradeRow> = trades.into_iter().map(TradeRow::from).collect();
                print_list(rows, format)?;
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

                // Build request as serde_json::Value to control number formatting
                // (JavaScript drops .0 from whole numbers, so we must match)
                let mut request = serde_json::json!({
                    "side": side.as_str(),
                    "type": order_type.as_str(),
                    "quantity": *quantity,
                    "leverage": if leverage.fract() == 0.0 {
                        serde_json::Value::Number((*leverage as i64).into())
                    } else {
                        serde_json::json!(*leverage)
                    }
                });

                if let Some(p) = price {
                    request["price"] = if p.fract() == 0.0 {
                        serde_json::Value::Number((*p as i64).into())
                    } else {
                        serde_json::json!(*p)
                    };
                }
                if let Some(sl) = stoploss {
                    request["stoploss"] = if sl.fract() == 0.0 {
                        serde_json::Value::Number((*sl as i64).into())
                    } else {
                        serde_json::json!(*sl)
                    };
                }
                if let Some(tp) = takeprofit {
                    request["takeprofit"] = if tp.fract() == 0.0 {
                        serde_json::Value::Number((*tp as i64).into())
                    } else {
                        serde_json::json!(*tp)
                    };
                }

                let trade: Trade = client.request(Method::POST, "futures/isolated/trade", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&trade)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&trade)?),
                    OutputFormat::Table => {
                        print_success(&format!("Position opened: {}", trade.id));
                    }
                }
            }

            Self::Stoploss { id, price } => {
                #[derive(Serialize)]
                struct StoplossRequest {
                    id: String,
                    stoploss: f64,
                }

                let request = StoplossRequest {
                    id: id.clone(),
                    stoploss: *price,
                };

                let trade: Trade = client.request(Method::PUT, "futures/isolated/trade/stoploss", Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&trade)?);
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Stop loss updated to {} for trade {}", format_price(*price), id));
                    }
                }
            }

            Self::Takeprofit { id, price } => {
                #[derive(Serialize)]
                struct TakeprofitRequest {
                    id: String,
                    takeprofit: f64,
                }

                let request = TakeprofitRequest {
                    id: id.clone(),
                    takeprofit: *price,
                };

                let trade: Trade = client.request(Method::PUT, "futures/isolated/trade/takeprofit", Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&trade)?);
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Take profit updated to {} for trade {}", format_price(*price), id));
                    }
                }
            }

            Self::AddMargin { id, amount } => {
                #[derive(Serialize)]
                struct AddMarginRequest {
                    id: String,
                    amount: i64,
                }

                let request = AddMarginRequest {
                    id: id.clone(),
                    amount: *amount,
                };

                let trade: Trade = client.request(Method::POST, "futures/isolated/trade/add-margin", Some(&request)).await?;

                match format {
                    OutputFormat::Json | OutputFormat::JsonPretty => {
                        println!("{}", serde_json::to_string_pretty(&trade)?);
                    }
                    OutputFormat::Table => {
                        print_success(&format!("Added {} to trade {}", format_sats(*amount), id));
                    }
                }
            }

            Self::Cashin { id, amount } => {
                #[derive(Serialize)]
                struct CashInRequest {
                    id: String,
                    amount: i64,
                }

                let request = CashInRequest {
                    id: id.clone(),
                    amount: *amount,
                };

                let result: serde_json::Value = client.request(Method::POST, "futures/isolated/trade/cash-in", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Cashed in {} from trade {}", format_sats(*amount), id));
                    }
                }
            }

            Self::Close { id } => {
                let request = serde_json::json!({ "id": id });
                let result: serde_json::Value = client.request(Method::POST, "futures/isolated/trade/close", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Trade {} closed", id));
                    }
                }
            }

            Self::Cancel { id } => {
                #[derive(Serialize)]
                struct CancelRequest {
                    id: String,
                }

                let request = CancelRequest { id: id.clone() };
                let result: serde_json::Value = client.request(Method::POST, "futures/isolated/trade/cancel", Some(&request)).await?;

                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
                    OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&result)?),
                    OutputFormat::Table => {
                        print_success(&format!("Order {} canceled", id));
                    }
                }
            }

            Self::CancelAll => {
                let result: serde_json::Value = client.request(Method::POST, "futures/isolated/trades/cancel-all", None::<&()>).await?;

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
