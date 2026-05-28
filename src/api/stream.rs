//! WebSocket stream client for the LN Markets stream API.
//!
//! Tracer slice (issue #6): connects to `wss://stream.lnmarkets.com/v1`, subscribes
//! to the public ticker channel via JSON-RPC, and emits `Ticker` updates on a
//! tokio channel. Auto-reconnects with exponential backoff and answers pings.
//!
//! REST polling stays as the cold-start + fallback path for everything else.

use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::models::market::Ticker;

const STREAM_URL: &str = "wss://stream.lnmarkets.com/v1";
const TICKER_TOPIC: &str = "futures/inverse/btc_usd/ticker";
const BACKOFF_INITIAL_MS: u64 = 500;
const BACKOFF_MAX_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamStatus {
    Connecting,
    Connected,
    Disconnected,
}

pub enum StreamEvent {
    Status(StreamStatus),
    Ticker(Ticker),
}

pub struct StreamHandle {
    pub events: mpsc::UnboundedReceiver<StreamEvent>,
}

/// Spawns the stream client task and returns a handle holding the event receiver.
pub fn start() -> StreamHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(run_loop(tx));
    StreamHandle { events: rx }
}

async fn run_loop(tx: mpsc::UnboundedSender<StreamEvent>) {
    let mut backoff_ms = BACKOFF_INITIAL_MS;
    loop {
        let _ = tx.send(StreamEvent::Status(StreamStatus::Connecting));
        match connect_and_run(&tx).await {
            Ok(()) => backoff_ms = BACKOFF_INITIAL_MS,
            Err(_) => {}
        }
        let _ = tx.send(StreamEvent::Status(StreamStatus::Disconnected));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(BACKOFF_MAX_MS);
    }
}

async fn connect_and_run(tx: &mpsc::UnboundedSender<StreamEvent>) -> Result<()> {
    let (mut ws, _) = connect_async(STREAM_URL).await?;
    let _ = tx.send(StreamEvent::Status(StreamStatus::Connected));

    // Subscribe to the public ticker channel
    let sub = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "subscribe",
        "params": { "topics": [TICKER_TOPIC] }
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
    Err(anyhow!("stream ended"))
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
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Push notification frame: { jsonrpc, method: "subscription", params: { topic, data } }
    if value.get("method").and_then(|m| m.as_str()) != Some("subscription") {
        return;
    }
    let Some(params) = value.get("params") else { return };
    let topic = params.get("topic").and_then(|t| t.as_str()).unwrap_or("");
    if topic != TICKER_TOPIC {
        return;
    }
    let Some(data) = params.get("data") else { return };
    let Ok(payload) = serde_json::from_value::<TickerPayload>(data.clone()) else { return };

    let ticker = Ticker {
        time: Some(payload.time.to_string()),
        index: payload.index.unwrap_or(0.0),
        last_price: payload.last_price,
        // The buckets channel feeds orderbook levels — leave empty here so the TUI
        // merges with whatever REST polling has already loaded.
        prices: Vec::new(),
        funding_rate: payload.funding.rate,
        funding_time: payload.funding.time.map(|t| t.to_string()),
    };
    let _ = tx.send(StreamEvent::Ticker(ticker));
}
