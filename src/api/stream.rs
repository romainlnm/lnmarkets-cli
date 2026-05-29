//! WebSocket stream client for the LN Markets stream API.
//!
//! Subscribes to public + (when credentials are present) private JSON-RPC
//! channels at `wss://stream.lnmarkets.com/v1`, then forwards typed events
//! on a tokio channel for the TUI to consume in real time.
//!
//! REST polling stays as cold-start + fallback for anything not yet streamed.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::api::auth::generate_signature;
use crate::config::Credentials;
use crate::models::funding::{Deposit, Withdrawal};
use crate::models::futures::Trade;
use crate::models::market::Ticker;

const STREAM_URL: &str = "wss://stream.lnmarkets.com/v1";
const TICKER_TOPIC: &str = "futures/inverse/btc_usd/ticker";
const ISOLATED_TRADES_TOPIC: &str = "futures/inverse/btc_usd/isolated/trades";
const CROSS_ORDERS_TOPIC: &str = "futures/inverse/btc_usd/cross/orders";
const CROSS_POSITION_TOPIC: &str = "futures/inverse/btc_usd/cross/position";
const WALLET_DEPOSIT_TOPIC: &str = "wallet/deposit";
const WALLET_WITHDRAWAL_TOPIC: &str = "wallet/withdrawal";

const BACKOFF_INITIAL_MS: u64 = 500;
const BACKOFF_MAX_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamStatus {
    Connecting,
    Connected,
    Authenticated,
    Disconnected,
}

/// Lifecycle of an isolated trade — drives Positions / Orders / History updates.
#[derive(Debug, Clone)]
pub enum IsolatedEvent {
    /// Limit order placed, not yet filled — belongs in Orders.
    Open(Trade),
    /// Order filled into a running position — belongs in Positions.
    Filled(Trade),
    /// Position closed normally — exit_price, pl set; goes to History.
    Closed(ClosedTrade),
    /// Open order canceled — remove from Orders.
    Canceled { id: String },
    /// Liquidation / stoploss / takeprofit — closes the position with terminal state.
    Liquidation(ClosedTrade),
    Stoploss(ClosedTrade),
    Takeprofit(ClosedTrade),
    /// Funding settlement — updates margin + liquidation on the running trade.
    Funding {
        id: String,
        margin: i64,
        liquidation_price: f64,
    },
}

#[derive(Debug, Clone)]
pub struct ClosedTrade {
    pub id: String,
    pub exit_price: Option<f64>,
    pub pl: Option<i64>,
}

// Some fields here are emitted for future App-side consumers (e.g. richer
// History updates) — silence dead-code warnings while that's wired up later.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CrossPositionUpdate {
    pub event: String,
    pub quantity: f64,
    pub leverage: f64,
    pub margin: i64,
    pub entry_price: Option<f64>,
    pub liquidation: Option<f64>,
    pub delta_pl: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum CrossOrderEvent {
    New { id: String },
    Filled { id: String },
    Canceled { id: String },
}

pub enum StreamEvent {
    Status(StreamStatus),
    Ticker(Ticker),
    Isolated(IsolatedEvent),
    CrossOrder(CrossOrderEvent),
    CrossPosition(CrossPositionUpdate),
    Deposit(Deposit),
    Withdrawal(Withdrawal),
}

pub struct StreamHandle {
    pub events: mpsc::UnboundedReceiver<StreamEvent>,
}

#[derive(Clone)]
pub struct StreamCredentials {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
}

impl StreamCredentials {
    pub fn from_config(c: &Credentials) -> Option<Self> {
        Some(Self {
            api_key: c.api_key.clone()?,
            api_secret: c.api_secret.clone()?,
            passphrase: c.passphrase.clone()?,
        })
    }
}

/// Spawns the stream client task and returns the event receiver.
pub fn start(credentials: Option<StreamCredentials>) -> StreamHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(run_loop(tx, credentials));
    StreamHandle { events: rx }
}

async fn run_loop(tx: mpsc::UnboundedSender<StreamEvent>, creds: Option<StreamCredentials>) {
    let mut backoff_ms = BACKOFF_INITIAL_MS;
    loop {
        let _ = tx.send(StreamEvent::Status(StreamStatus::Connecting));
        match connect_and_run(&tx, creds.as_ref()).await {
            Ok(()) => backoff_ms = BACKOFF_INITIAL_MS,
            Err(_) => {}
        }
        let _ = tx.send(StreamEvent::Status(StreamStatus::Disconnected));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(BACKOFF_MAX_MS);
    }
}

async fn connect_and_run(
    tx: &mpsc::UnboundedSender<StreamEvent>,
    creds: Option<&StreamCredentials>,
) -> Result<()> {
    let (mut ws, _) = connect_async(STREAM_URL).await?;
    let _ = tx.send(StreamEvent::Status(StreamStatus::Connected));

    // Always subscribe to the public ticker — the Dashboard chart depends on it.
    let mut topics: Vec<&'static str> = vec![TICKER_TOPIC];

    // If credentials are present, authenticate first; on success add private topics.
    let mut authenticated = false;
    if let Some(c) = creds {
        match authenticate(&mut ws, c).await {
            Ok(()) => {
                authenticated = true;
                let _ = tx.send(StreamEvent::Status(StreamStatus::Authenticated));
                topics.extend_from_slice(&[
                    ISOLATED_TRADES_TOPIC,
                    CROSS_ORDERS_TOPIC,
                    CROSS_POSITION_TOPIC,
                    WALLET_DEPOSIT_TOPIC,
                    WALLET_WITHDRAWAL_TOPIC,
                ]);
            }
            Err(_) => {
                // Auth failed — keep the public ticker working, surface no private events.
            }
        }
    }

    let sub = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "subscribe",
        "params": { "topics": topics }
    });
    ws.send(Message::Text(sub.to_string())).await?;

    while let Some(msg) = ws.next().await {
        match msg? {
            Message::Text(text) => handle_text(&text, tx),
            Message::Ping(p) => ws.send(Message::Pong(p)).await?,
            Message::Close(_) => return Err(anyhow!("server closed connection")),
            _ => {}
        }
    }
    let _ = authenticated; // (kept for future per-channel re-subscribe logic)
    Err(anyhow!("stream ended"))
}

/// Performs the JSON-RPC `authenticate` handshake.
/// Signature payload per the server: `timestamp + nonce`, HMAC-SHA256 with the API secret.
async fn authenticate(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    creds: &StreamCredentials,
) -> Result<()> {
    let timestamp = now_ms();
    let nonce = make_nonce();
    // Reuse generate_signature with empty method+path so the message reduces to
    // `timestamp + nonce` — matches the WS auth payload format on the server.
    let signature = generate_signature(&creds.api_secret, timestamp, "", "", &nonce);

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "authenticate",
        "params": {
            "key": creds.api_key,
            "signature": signature,
            "timestamp": timestamp,
            "passphrase": creds.passphrase,
            "nonce": nonce,
        }
    });
    ws.send(Message::Text(req.to_string())).await?;

    // Wait briefly for the response; tolerate other frames arriving in the meantime.
    let auth_resp = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg) = ws.next().await {
            let text = match msg? {
                Message::Text(t) => t,
                Message::Ping(p) => {
                    ws.send(Message::Pong(p)).await?;
                    continue;
                }
                Message::Close(_) => return Err(anyhow!("closed during auth")),
                _ => continue,
            };
            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("id").and_then(|i| i.as_i64()) != Some(1) {
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(anyhow!("auth error: {}", err));
            }
            let ok = v
                .get("result")
                .and_then(|r| r.get("authenticated"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            if !ok {
                return Err(anyhow!("auth rejected"));
            }
            return Ok(());
        }
        Err(anyhow!("ws closed before auth response"))
    })
    .await
    .map_err(|_| anyhow!("auth timeout"))?;
    auth_resp
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn make_nonce() -> String {
    // 16+ ascii hex chars from nanos + pid — fits the server's 8..128 range and
    // is unpredictable enough for the per-request replay window.
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", ns, std::process::id())
}

#[derive(Deserialize)]
struct TickerPayload {
    time: i64,
    #[serde(rename = "lastPrice")]
    last_price: Option<f64>,
    index: Option<f64>,
    funding: TickerFunding,
}

#[derive(Deserialize)]
struct TickerFunding {
    rate: Option<f64>,
    time: Option<i64>,
}

fn handle_text(text: &str, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    if value.get("method").and_then(|m| m.as_str()) != Some("subscription") {
        return;
    }
    let Some(params) = value.get("params") else { return };
    let topic = params.get("topic").and_then(|t| t.as_str()).unwrap_or("");
    let Some(data) = params.get("data") else { return };

    match topic {
        TICKER_TOPIC => emit_ticker(data, tx),
        ISOLATED_TRADES_TOPIC => emit_isolated(data, tx),
        CROSS_ORDERS_TOPIC => emit_cross_order(data, tx),
        CROSS_POSITION_TOPIC => emit_cross_position(data, tx),
        WALLET_DEPOSIT_TOPIC => emit_deposit(data, tx),
        WALLET_WITHDRAWAL_TOPIC => emit_withdrawal(data, tx),
        _ => {}
    }
}

fn emit_ticker(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let Ok(payload) = serde_json::from_value::<TickerPayload>(data.clone()) else { return };
    let ticker = Ticker {
        time: Some(payload.time.to_string()),
        index: payload.index.unwrap_or(0.0),
        last_price: payload.last_price,
        prices: Vec::new(),
        funding_rate: payload.funding.rate,
        funding_time: payload.funding.time.map(|t| t.to_string()),
    };
    let _ = tx.send(StreamEvent::Ticker(ticker));
}

fn emit_isolated(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let event = data.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let Some(trade_val) = data.get("trade") else { return };

    // Build a Trade from the WS payload. Many events carry only the close-side
    // fields; we map them onto our existing Trade model leaving the open-side
    // fields for whatever was already loaded by REST.
    let id = trade_val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let side = trade_val.get("side").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let order_type = trade_val
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("market")
        .to_string();
    let quantity = trade_val.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
    let leverage = trade_val.get("leverage").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let price = trade_val.get("price").and_then(|v| v.as_f64());
    let margin = trade_val.get("margin").and_then(|v| v.as_i64());

    let make_trade = || Trade {
        id: id.clone(),
        user_id: None,
        side: side.clone(),
        order_type: order_type.clone(),
        quantity,
        leverage,
        stop_loss: None,
        take_profit: None,
        price,
        entry_price: price,
        exit_price: None,
        margin,
        margin_with_cf: None,
        pl: None,
        liquidation_price: None,
        created_at: trade_val.get("createdAt").and_then(|v| v.as_i64()).map(|t| t.to_string()),
        open_at: None,
        closed_at: None,
        last_update: None,
        margin_type: crate::models::futures::MarginType::Isolated,
        opening_fee: trade_val.get("openingFee").and_then(|v| v.as_i64()),
        sum_carry_fees: None,
    };

    let closed = || ClosedTrade {
        id: id.clone(),
        exit_price: trade_val.get("exitPrice").and_then(|v| v.as_f64()),
        pl: trade_val.get("pl").and_then(|v| v.as_i64()),
    };

    let ev = match event {
        "open" => IsolatedEvent::Open(make_trade()),
        "filled" => IsolatedEvent::Filled(make_trade()),
        "closed" => IsolatedEvent::Closed(closed()),
        "canceled" => IsolatedEvent::Canceled { id: id.clone() },
        "liquidation" => IsolatedEvent::Liquidation(closed()),
        "stoploss" => IsolatedEvent::Stoploss(closed()),
        "takeprofit" => IsolatedEvent::Takeprofit(closed()),
        "funding" => IsolatedEvent::Funding {
            id: id.clone(),
            margin: margin.unwrap_or(0),
            liquidation_price: trade_val
                .get("liquidationPrice")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        },
        _ => return,
    };
    let _ = tx.send(StreamEvent::Isolated(ev));
}

fn emit_cross_order(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let event = data.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let Some(order) = data.get("order") else { return };
    let id = order.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let ev = match event {
        "new" => CrossOrderEvent::New { id },
        "limit" => CrossOrderEvent::Filled { id },
        "canceled" => CrossOrderEvent::Canceled { id },
        _ => return,
    };
    let _ = tx.send(StreamEvent::CrossOrder(ev));
}

fn emit_cross_position(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let event = data.get("event").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let Some(pos) = data.get("position") else { return };
    let update = CrossPositionUpdate {
        event,
        quantity: pos.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0),
        leverage: pos.get("leverage").and_then(|v| v.as_f64()).unwrap_or(1.0),
        margin: pos.get("margin").and_then(|v| v.as_i64()).unwrap_or(0),
        entry_price: pos.get("entryPrice").and_then(|v| v.as_f64()),
        liquidation: pos.get("liquidation").and_then(|v| v.as_f64()),
        delta_pl: pos.get("deltaPl").and_then(|v| v.as_i64()),
    };
    let _ = tx.send(StreamEvent::CrossPosition(update));
}

fn emit_deposit(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let d = Deposit {
        id: data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        amount: data.get("amount").and_then(|v| v.as_i64()),
        status: data.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        deposit_type: data.get("network").and_then(|v| v.as_str()).map(|s| s.to_string()),
        created_at: None,
        confirmed_at: None,
    };
    let _ = tx.send(StreamEvent::Deposit(d));
}

fn emit_withdrawal(data: &Value, tx: &mpsc::UnboundedSender<StreamEvent>) {
    let w = Withdrawal {
        id: data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        amount: data.get("amount").and_then(|v| v.as_i64()),
        status: data.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()),
        withdrawal_type: data.get("network").and_then(|v| v.as_str()).map(|s| s.to_string()),
        created_at: None,
        confirmed_at: None,
    };
    let _ = tx.send(StreamEvent::Withdrawal(w));
}

// ---------------------------------------------------------------------------
// Raw passthrough mode — used by the `lnmarkets stream watch` CLI subcommand
// to emit one JSON line per subscription event without the TUI's typed-decode
// path. Shares connection / auth logic with the typed flow above.
// ---------------------------------------------------------------------------

/// Raw subscription messages, one per WS push, plus connection status changes.
pub enum RawStreamMsg {
    Status(StreamStatus),
    Data { topic: String, data: Value },
}

/// Subscribe to an explicit list of topics and emit every push as a
/// `RawStreamMsg::Data { topic, data }`. Authenticates first if any of the
/// requested topics is private and credentials are provided.
pub fn start_raw(
    topics: Vec<String>,
    credentials: Option<StreamCredentials>,
) -> mpsc::UnboundedReceiver<RawStreamMsg> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(run_raw_loop(tx, topics, credentials));
    rx
}

fn topic_is_private(topic: &str) -> bool {
    topic.starts_with("wallet/") || topic.contains("/isolated/") || topic.contains("/cross/")
}

async fn run_raw_loop(
    tx: mpsc::UnboundedSender<RawStreamMsg>,
    topics: Vec<String>,
    creds: Option<StreamCredentials>,
) {
    let mut backoff_ms = BACKOFF_INITIAL_MS;
    loop {
        let _ = tx.send(RawStreamMsg::Status(StreamStatus::Connecting));
        match connect_and_run_raw(&tx, &topics, creds.as_ref()).await {
            Ok(()) => backoff_ms = BACKOFF_INITIAL_MS,
            Err(_) => {}
        }
        let _ = tx.send(RawStreamMsg::Status(StreamStatus::Disconnected));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(BACKOFF_MAX_MS);
    }
}

async fn connect_and_run_raw(
    tx: &mpsc::UnboundedSender<RawStreamMsg>,
    topics: &[String],
    creds: Option<&StreamCredentials>,
) -> Result<()> {
    let (mut ws, _) = connect_async(STREAM_URL).await?;
    let _ = tx.send(RawStreamMsg::Status(StreamStatus::Connected));

    let needs_auth = topics.iter().any(|t| topic_is_private(t));

    if needs_auth {
        let c = creds.ok_or_else(|| {
            anyhow!("authentication required for private channels — configure your API key first")
        })?;
        authenticate(&mut ws, c).await?;
        let _ = tx.send(RawStreamMsg::Status(StreamStatus::Authenticated));
    } else if let Some(c) = creds {
        // Public-only topic list but creds available — auth opportunistically
        // (e.g. "all" mode). Degrade gracefully on failure.
        if authenticate(&mut ws, c).await.is_ok() {
            let _ = tx.send(RawStreamMsg::Status(StreamStatus::Authenticated));
        }
    }

    let sub = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "subscribe",
        "params": { "topics": topics }
    });
    ws.send(Message::Text(sub.to_string())).await?;

    while let Some(msg) = ws.next().await {
        match msg? {
            Message::Text(text) => {
                let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                if value.get("method").and_then(|m| m.as_str()) != Some("subscription") {
                    continue;
                }
                let Some(params) = value.get("params") else { continue };
                let topic = params
                    .get("topic")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let Some(data) = params.get("data") else { continue };
                let _ = tx.send(RawStreamMsg::Data { topic, data: data.clone() });
            }
            Message::Ping(p) => ws.send(Message::Pong(p)).await?,
            Message::Close(_) => return Err(anyhow!("server closed connection")),
            _ => {}
        }
    }
    Err(anyhow!("stream ended"))
}
