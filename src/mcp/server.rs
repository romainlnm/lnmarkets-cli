use anyhow::Result;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::api::LnmClient;
use crate::cli::mcp::{ServiceGroup, parse_services};
use crate::models::Ticker;

use super::tools::{
    AddMarginRequest, CloseTradeRequest, DepositRequest, ListTradesRequest, OpenTradeRequest,
    UpdateStoplossRequest, UpdateTakeprofitRequest, WithdrawRequest,
};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC request
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// MCP Tool definition
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
}

/// Tool metadata for filtering and safety
struct ToolMeta {
    name: &'static str,
    description: &'static str,
    service: ServiceGroup,
    dangerous: bool,
    schema: Value,
}

/// MCP Server implementation
pub struct LnMarketsServer {
    client: LnmClient,
    enabled_services: HashSet<ServiceGroup>,
    allow_dangerous: bool,
}

impl LnMarketsServer {
    pub fn new(client: LnmClient, services: &str, allow_dangerous: bool) -> Self {
        let service_list = parse_services(services);
        let enabled_services: HashSet<ServiceGroup> = service_list.into_iter().collect();

        Self {
            client,
            enabled_services,
            allow_dangerous,
        }
    }

    /// Get all tool definitions
    fn all_tools(&self) -> Vec<ToolMeta> {
        vec![
            // Market tools (public, read-only)
            ToolMeta {
                name: "get_ticker",
                description: "Get current BTC/USD price, bid/ask spread, and funding rate from LN Markets",
                service: ServiceGroup::Market,
                dangerous: false,
                schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            // Account tools (authenticated, read-only)
            ToolMeta {
                name: "get_balance",
                description: "Get your LN Markets account balance in satoshis",
                service: ServiceGroup::Account,
                dangerous: false,
                schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolMeta {
                name: "list_trades",
                description: "List your trades filtered by status. Use 'running' for active positions, 'open' for pending orders, 'closed' for completed trades.",
                service: ServiceGroup::Account,
                dangerous: false,
                schema: json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Trade status: 'open' (pending orders), 'running' (active positions), 'closed', or 'canceled'",
                            "enum": ["open", "running", "closed", "canceled"],
                            "default": "running"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of trades to return (default: 50)",
                            "default": 50
                        }
                    },
                    "required": []
                }),
            },
            // Trade tools (dangerous)
            ToolMeta {
                name: "open_trade",
                description: "[DANGEROUS: requires acknowledged=true] Open a new BTC/USD futures position. Specify side ('buy' for long, 'sell' for short), quantity in sats, and optionally leverage (1-100), order type, price, stoploss, and takeprofit.",
                service: ServiceGroup::Trade,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "side": {
                            "type": "string",
                            "description": "Trade side: 'buy' for long position, 'sell' for short position",
                            "enum": ["buy", "sell"]
                        },
                        "quantity": {
                            "type": "integer",
                            "description": "Position size in satoshis (1 BTC = 100,000,000 sats)"
                        },
                        "leverage": {
                            "type": "number",
                            "description": "Leverage multiplier (1-100, default: 1)",
                            "default": 1
                        },
                        "order_type": {
                            "type": "string",
                            "description": "Order type: 'market' (immediate execution) or 'limit' (execute at specified price)",
                            "enum": ["market", "limit"],
                            "default": "market"
                        },
                        "price": {
                            "type": "number",
                            "description": "Limit price in USD (required for limit orders, ignored for market orders)"
                        },
                        "stoploss": {
                            "type": "number",
                            "description": "Stop loss price in USD - position closes if price reaches this level"
                        },
                        "takeprofit": {
                            "type": "number",
                            "description": "Take profit price in USD - position closes if price reaches this level"
                        }
                    },
                    "required": ["side", "quantity"]
                })),
            },
            ToolMeta {
                name: "close_trade",
                description: "[DANGEROUS: requires acknowledged=true] Close a running futures position by its trade ID",
                service: ServiceGroup::Trade,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the trade to close"
                        }
                    },
                    "required": ["id"]
                })),
            },
            ToolMeta {
                name: "update_stoploss",
                description: "[DANGEROUS: requires acknowledged=true] Update the stop loss price for a running trade",
                service: ServiceGroup::Trade,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the trade"
                        },
                        "price": {
                            "type": "number",
                            "description": "New stop loss price in USD"
                        }
                    },
                    "required": ["id", "price"]
                })),
            },
            ToolMeta {
                name: "update_takeprofit",
                description: "[DANGEROUS: requires acknowledged=true] Update the take profit price for a running trade",
                service: ServiceGroup::Trade,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the trade"
                        },
                        "price": {
                            "type": "number",
                            "description": "New take profit price in USD"
                        }
                    },
                    "required": ["id", "price"]
                })),
            },
            ToolMeta {
                name: "add_margin",
                description: "[DANGEROUS: requires acknowledged=true] Add margin (in satoshis) to a running position to reduce liquidation risk",
                service: ServiceGroup::Trade,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The unique identifier of the trade"
                        },
                        "amount": {
                            "type": "integer",
                            "description": "Amount in satoshis to add as margin"
                        }
                    },
                    "required": ["id", "amount"]
                })),
            },
            // Funding tools (dangerous)
            ToolMeta {
                name: "deposit",
                description: "[DANGEROUS: requires acknowledged=true] Generate a Lightning invoice to deposit funds into your LN Markets account",
                service: ServiceGroup::Funding,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "amount": {
                            "type": "integer",
                            "description": "Amount in satoshis to deposit"
                        }
                    },
                    "required": ["amount"]
                })),
            },
            ToolMeta {
                name: "withdraw",
                description: "[DANGEROUS: requires acknowledged=true] Withdraw funds from your LN Markets account by paying a Lightning invoice",
                service: ServiceGroup::Funding,
                dangerous: true,
                schema: self.dangerous_schema(json!({
                    "type": "object",
                    "properties": {
                        "invoice": {
                            "type": "string",
                            "description": "BOLT11 Lightning invoice to pay for withdrawal"
                        }
                    },
                    "required": ["invoice"]
                })),
            },
        ]
    }

    /// Add acknowledged parameter to dangerous tool schemas
    fn dangerous_schema(&self, mut schema: Value) -> Value {
        if !self.allow_dangerous {
            if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
                props.insert("acknowledged".to_string(), json!({
                    "type": "boolean",
                    "description": "Must be set to true to confirm this dangerous operation"
                }));
            }
            if let Some(required) = schema.get_mut("required").and_then(|r| r.as_array_mut()) {
                required.push(json!("acknowledged"));
            }
        }
        schema
    }

    /// Check if a tool is enabled based on service groups
    fn is_tool_enabled(&self, tool: &ToolMeta) -> bool {
        self.enabled_services.contains(&tool.service)
    }

    /// Check safety gate for dangerous operations
    fn check_safety_gate(&self, tool_name: &str, args: &Value) -> Result<(), String> {
        // Find the tool metadata
        let tools = self.all_tools();
        let tool = tools.iter().find(|t| t.name == tool_name);

        if let Some(tool) = tool {
            if tool.dangerous && !self.allow_dangerous {
                let acknowledged = args.get("acknowledged").and_then(|v| v.as_bool()).unwrap_or(false);
                if !acknowledged {
                    return Err(format!(
                        "This operation requires confirmation. Set acknowledged=true to proceed, or start the server with --allow-dangerous flag."
                    ));
                }
            }
        }

        Ok(())
    }

    /// Run the MCP server on stdio
    pub async fn run(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            // EOF - client closed connection
            if bytes_read == 0 {
                break;
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(line) {
                Ok(req) => req,
                Err(e) => {
                    let response = JsonRpcResponse::error(None, -32700, format!("Parse error: {}", e));
                    self.send_response(&mut stdout, &response).await?;
                    continue;
                }
            };

            // Handle request - notifications return None (no response needed)
            if let Some(response) = self.handle_request(request).await {
                self.send_response(&mut stdout, &response).await?;
            }
        }

        Ok(())
    }

    async fn send_response(&self, stdout: &mut tokio::io::Stdout, response: &JsonRpcResponse) -> Result<()> {
        let json = serde_json::to_string(response)?;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        Ok(())
    }

    async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Notifications (methods starting with "notifications/") don't expect a response
        if request.method.starts_with("notifications/") {
            return None;
        }

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "ping" => JsonRpcResponse::success(request.id, json!({})),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        };

        Some(response)
    }

    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        let services: Vec<&str> = self.enabled_services.iter().map(|s| match s {
            ServiceGroup::Market => "market",
            ServiceGroup::Account => "account",
            ServiceGroup::Trade => "trade",
            ServiceGroup::Funding => "funding",
        }).collect();

        let mode = if self.allow_dangerous { "autonomous" } else { "confirmed" };

        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "lnmarkets",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": format!(
                    "LN Markets trading tools for Bitcoin futures. \
                    Enabled services: {}. Mode: {} (dangerous operations {}). \
                    All amounts are in satoshis (1 BTC = 100,000,000 sats). Prices are in USD.",
                    services.join(", "),
                    mode,
                    if self.allow_dangerous { "allowed without confirmation" } else { "require acknowledged=true" }
                )
            }),
        )
    }

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        let tools: Vec<Tool> = self.all_tools()
            .into_iter()
            .filter(|t| self.is_tool_enabled(t))
            .map(|t| Tool {
                name: t.name.to_string(),
                description: if t.dangerous && !self.allow_dangerous {
                    t.description.to_string()
                } else {
                    // Remove the [DANGEROUS: ...] prefix in autonomous mode
                    t.description
                        .strip_prefix("[DANGEROUS: requires acknowledged=true] ")
                        .unwrap_or(t.description)
                        .to_string()
                },
                input_schema: t.schema,
            })
            .collect();

        JsonRpcResponse::success(id, json!({ "tools": tools }))
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
            }
        };

        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        // Check if tool is enabled
        let tools = self.all_tools();
        let tool = tools.iter().find(|t| t.name == name);

        if let Some(tool) = tool {
            if !self.is_tool_enabled(tool) {
                return JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Tool '{}' is not enabled. Enable the '{}' service group with -s flag.",
                                name,
                                match tool.service {
                                    ServiceGroup::Market => "market",
                                    ServiceGroup::Account => "account",
                                    ServiceGroup::Trade => "trade",
                                    ServiceGroup::Funding => "funding",
                                }
                            )
                        }],
                        "isError": true
                    }),
                );
            }
        }

        // Check safety gate
        if let Err(e) = self.check_safety_gate(name, &arguments) {
            return JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": e
                    }],
                    "isError": true
                }),
            );
        }

        let result = match name {
            "get_ticker" => self.tool_get_ticker().await,
            "get_balance" => self.tool_get_balance().await,
            "list_trades" => self.tool_list_trades(arguments).await,
            "open_trade" => self.tool_open_trade(arguments).await,
            "close_trade" => self.tool_close_trade(arguments).await,
            "update_stoploss" => self.tool_update_stoploss(arguments).await,
            "update_takeprofit" => self.tool_update_takeprofit(arguments).await,
            "add_margin" => self.tool_add_margin(arguments).await,
            "deposit" => self.tool_deposit(arguments).await,
            "withdraw" => self.tool_withdraw(arguments).await,
            _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
        };

        match result {
            Ok(content) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": content
                    }]
                }),
            ),
            Err(e) => JsonRpcResponse::success(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Error: {}", e)
                    }],
                    "isError": true
                }),
            ),
        }
    }

    async fn tool_get_ticker(&self) -> Result<String> {
        let ticker: Ticker = self
            .client
            .public_request(Method::GET, "futures/ticker")
            .await?;

        let (bid, ask) = ticker
            .prices
            .first()
            .map(|p| (p.bid_price, p.ask_price))
            .unwrap_or((0.0, 0.0));

        let result = json!({
            "index": ticker.index,
            "bid": bid,
            "ask": ask,
            "lastPrice": ticker.last_price,
            "fundingRate": ticker.funding_rate,
            "fundingTime": ticker.funding_time,
        });

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_get_balance(&self) -> Result<String> {
        let user: Value = self
            .client
            .request(Method::GET, "account", None::<&()>)
            .await?;

        let balance = user.get("balance").and_then(|v| v.as_i64()).unwrap_or(0);

        let result = json!({
            "balance_sats": balance,
            "balance_btc": balance as f64 / 100_000_000.0,
        });

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_list_trades(&self, args: Value) -> Result<String> {
        let request: ListTradesRequest = serde_json::from_value(args)?;
        let status = request.status.as_deref().unwrap_or("running");
        let limit = request.limit.unwrap_or(50);

        let path = match status {
            "open" => format!("futures/isolated/trades/open?limit={}", limit),
            "running" => format!("futures/isolated/trades/running?limit={}", limit),
            "closed" => format!("futures/isolated/trades/closed?limit={}", limit),
            "canceled" => format!("futures/isolated/trades/canceled?limit={}", limit),
            _ => anyhow::bail!("Invalid status. Use: open, running, closed, or canceled"),
        };

        let trades: Vec<Value> = self
            .client
            .request(Method::GET, &path, None::<&()>)
            .await?;

        Ok(serde_json::to_string_pretty(&trades)?)
    }

    async fn tool_open_trade(&self, args: Value) -> Result<String> {
        let request: OpenTradeRequest = serde_json::from_value(args)?;

        // Validate side
        let side = match request.side.to_lowercase().as_str() {
            "buy" | "b" | "long" => "buy",
            "sell" | "s" | "short" => "sell",
            _ => anyhow::bail!("Invalid side. Use 'buy' (long) or 'sell' (short)"),
        };

        // Validate order type
        let order_type = match request.order_type.as_deref().unwrap_or("market") {
            "market" | "m" => "market",
            "limit" | "l" => "limit",
            _ => anyhow::bail!("Invalid order type. Use 'market' or 'limit'"),
        };

        // Validate limit orders have a price
        if order_type == "limit" && request.price.is_none() {
            anyhow::bail!("Price is required for limit orders");
        }

        let leverage = request.leverage.unwrap_or(1.0);

        // Build request body with proper number formatting
        let mut body = json!({
            "side": side,
            "type": order_type,
            "quantity": request.quantity,
            "leverage": if leverage.fract() == 0.0 {
                Value::Number((leverage as i64).into())
            } else {
                json!(leverage)
            }
        });

        if let Some(p) = request.price {
            body["price"] = if p.fract() == 0.0 {
                Value::Number((p as i64).into())
            } else {
                json!(p)
            };
        }
        if let Some(sl) = request.stoploss {
            body["stoploss"] = if sl.fract() == 0.0 {
                Value::Number((sl as i64).into())
            } else {
                json!(sl)
            };
        }
        if let Some(tp) = request.takeprofit {
            body["takeprofit"] = if tp.fract() == 0.0 {
                Value::Number((tp as i64).into())
            } else {
                json!(tp)
            };
        }

        let trade: Value = self
            .client
            .request(Method::POST, "futures/isolated/trade", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&trade)?)
    }

    async fn tool_close_trade(&self, args: Value) -> Result<String> {
        let request: CloseTradeRequest = serde_json::from_value(args)?;
        let body = json!({ "id": request.id });

        let result: Value = self
            .client
            .request(Method::POST, "futures/isolated/trade/close", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_update_stoploss(&self, args: Value) -> Result<String> {
        let request: UpdateStoplossRequest = serde_json::from_value(args)?;
        let price = request.price;
        let body = json!({
            "id": request.id,
            "value": if price.fract() == 0.0 {
                serde_json::Value::Number((price as i64).into())
            } else {
                serde_json::json!(price)
            }
        });

        let result: Value = self
            .client
            .request(Method::PUT, "futures/isolated/trade/stoploss", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_update_takeprofit(&self, args: Value) -> Result<String> {
        let request: UpdateTakeprofitRequest = serde_json::from_value(args)?;
        let price = request.price;
        let body = json!({
            "id": request.id,
            "value": if price.fract() == 0.0 {
                serde_json::Value::Number((price as i64).into())
            } else {
                serde_json::json!(price)
            }
        });

        let result: Value = self
            .client
            .request(Method::PUT, "futures/isolated/trade/takeprofit", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_add_margin(&self, args: Value) -> Result<String> {
        let request: AddMarginRequest = serde_json::from_value(args)?;
        let body = json!({
            "id": request.id,
            "amount": request.amount
        });

        let result: Value = self
            .client
            .request(Method::POST, "futures/isolated/trade/add-margin", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&result)?)
    }

    async fn tool_deposit(&self, args: Value) -> Result<String> {
        let request: DepositRequest = serde_json::from_value(args)?;
        let body = json!({ "amount": request.amount });

        let invoice: Value = self
            .client
            .request(Method::POST, "account/deposit/lightning", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&invoice)?)
    }

    async fn tool_withdraw(&self, args: Value) -> Result<String> {
        let request: WithdrawRequest = serde_json::from_value(args)?;
        let body = json!({ "invoice": request.invoice });

        let result: Value = self
            .client
            .request(Method::POST, "account/withdraw/lightning", Some(&body))
            .await?;

        Ok(serde_json::to_string_pretty(&result)?)
    }
}
