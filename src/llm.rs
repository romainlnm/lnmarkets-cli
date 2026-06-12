//! LLM arbiter — single Claude call per cycle. Takes the structured
//! observations from every data collector and the current account state,
//! returns the trade decision.
//!
//! This is the only decision path. No weighted voting, no anti-whipsaw
//! guards in code — Claude handles all of that contextually.

use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::collectors::Direction;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-4-8";
const TIMEOUT_SECS: u64 = 30;
const MAX_TOKENS: u32 = 768;

const SYSTEM_PROMPT: &str = "You are a Bitcoin perpetual futures trader on LN Markets.
You trade with strict risk discipline and operate on a fixed budget.

You receive a structured snapshot of the market each cycle: multi-timeframe
price action with technical indicators, exchange order flow, the economic
calendar, recent news headlines, and whale positions on Hyperliquid. You also
receive your own recent decisions and trade outcomes. The numbers are facts;
your job is to interpret them in context.

Decision principles:
- Form a trade thesis. Don't react to a single headline or a single indicator.
- Weigh higher timeframes (1h) for trend direction; use 1m/5m for entry
  timing only. Don't fight the higher-timeframe trend without strong evidence.
- Review your recent decisions before acting. Don't flip-flop: reversing a
  position you opened minutes ago pays the spread and fees twice — reverse
  only when the thesis has genuinely broken, not on noise.
- Skip ambiguous markets — return action=\"hold\" when conviction is low.
- If you already hold a position aligned with the thesis, prefer hold over
  re-entering — don't churn through fees.
- If you hold a position against the current thesis, return action=\"close\".
- The snapshot states the round-trip fee cost as % of margin. Only open when
  your expected net-ROE move clearly exceeds that cost.
- position_pct scales with conviction: 0.3 marginal, 0.6 strong, 1.0 only for
  high-conviction setups with multiple confirming inputs.

Respond with the decision JSON only:
{
  \"action\": \"open_long\" | \"open_short\" | \"close\" | \"hold\",
  \"confidence\": 0.0-1.0,
  \"position_pct\": 0.0-1.0,
  \"reasoning\": \"one to three sentences explaining the thesis\"
}";

/// JSON Schema enforced via structured outputs — the API guarantees the
/// response text is valid JSON matching this shape. (Range clamps stay in
/// code: structured outputs don't support numeric min/max constraints.)
fn decision_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["open_long", "open_short", "close", "hold"]
            },
            "confidence": { "type": "number", "description": "0.0 to 1.0" },
            "position_pct": {
                "type": "number",
                "description": "0.0 to 1.0, fraction of max position"
            },
            "reasoning": {
                "type": "string",
                "description": "one to three sentences explaining the thesis"
            }
        },
        "required": ["action", "confidence", "position_pct", "reasoning"],
        "additionalProperties": false
    })
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmAction {
    OpenLong,
    OpenShort,
    Close,
    Hold,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmDecision {
    pub action: LlmAction,
    pub confidence: f64,
    #[serde(default)]
    pub position_pct: f64,
    pub reasoning: String,
}

impl LlmDecision {
    pub fn direction(&self) -> Direction {
        match self.action {
            LlmAction::OpenLong => Direction::Long,
            LlmAction::OpenShort => Direction::Short,
            LlmAction::Close | LlmAction::Hold => Direction::Neutral,
        }
    }
}

pub struct PositionBrief {
    pub side: Direction,
    pub size_usd: f64,
    pub entry_price: f64,
    pub pl_pct: f64,
}

pub struct MarketSnapshot<'a> {
    pub price: f64,
    pub collector_data: &'a BTreeMap<String, Value>,
    pub current_position: Option<&'a PositionBrief>,
    pub max_position_usd: u64,
    pub leverage: u32,
    pub mode: &'a str,
    /// Round-trip fees expressed as % of margin (net ROE) at this leverage.
    pub round_trip_fee_pct_of_margin: f64,
    /// Recent decisions and trade outcomes, oldest first (e.g. "12m ago: …").
    pub recent_history: &'a [String],
}

pub struct LlmArbiter {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl LlmArbiter {
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow!("ANTHROPIC_API_KEY not set — required for the daemon"))?;
        let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .context("build HTTP client")?;
        Ok(Self { api_key, model, client })
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub async fn decide(&self, snapshot: &MarketSnapshot<'_>) -> Result<LlmDecision> {
        let body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": SYSTEM_PROMPT,
            "output_config": { "format": {
                "type": "json_schema",
                "schema": decision_schema(),
            }},
            "messages": [{ "role": "user", "content": render_user_prompt(snapshot) }]
        });

        let resp = self
            .client
            .post(ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Claude API request failed")?;

        let status = resp.status();
        let body_text = resp.text().await.context("read response body")?;
        if !status.is_success() {
            return Err(anyhow!("Claude API {}: {}", status, body_text));
        }

        let resp_json: Value =
            serde_json::from_str(&body_text).context("parse JSON response")?;
        let text = resp_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("no content[0].text in response: {}", body_text))?;

        // Structured outputs guarantee the text is schema-valid JSON; the
        // extract_json fallback covers models without that support (e.g. a
        // custom ANTHROPIC_MODEL override).
        let mut decision: LlmDecision = match serde_json::from_str(text) {
            Ok(d) => d,
            Err(_) => {
                let json_str = extract_json(text)?;
                serde_json::from_str(&json_str)
                    .with_context(|| format!("parse decision JSON: {}", json_str))?
            }
        };
        decision.confidence = decision.confidence.clamp(0.0, 1.0);
        decision.position_pct = decision.position_pct.clamp(0.0, 1.0);
        Ok(decision)
    }
}

fn render_user_prompt(s: &MarketSnapshot<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "BTC/USD perpetual on LN Markets. Time: {}\n\n",
        Utc::now().to_rfc3339()
    ));
    out.push_str(&format!(
        "Constraints: max position ${} USD, leverage {}x, mode={}\n",
        s.max_position_usd, s.leverage, s.mode
    ));
    out.push_str(&format!(
        "Round-trip fees ≈ {:.1}% of margin (net ROE) at this leverage — an \
         expected move must clearly exceed this to be worth taking.\n\n",
        s.round_trip_fee_pct_of_margin
    ));
    out.push_str(&format!("Current BTC price: ${:.0}\n\n", s.price));

    if let Some(p) = s.current_position {
        out.push_str(&format!(
            "Current position: {} ${:.0} entered at ${:.0}, unrealized P&L {:+.2}%\n\n",
            p.side, p.size_usd, p.entry_price, p.pl_pct
        ));
    } else {
        out.push_str("Current position: none\n\n");
    }

    if !s.recent_history.is_empty() {
        out.push_str("YOUR RECENT DECISIONS AND TRADES (oldest first):\n");
        for line in s.recent_history {
            out.push_str(&format!("- {}\n", line));
        }
        out.push('\n');
    }

    if s.collector_data.is_empty() {
        out.push_str("No collector data available.\n");
    } else {
        out.push_str("DATA FROM COLLECTORS (raw observations — interpret in context):\n\n");
        for (name, data) in s.collector_data {
            out.push_str(&format!(
                "--- {} ---\n{}\n\n",
                name.to_uppercase(),
                serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
            ));
        }
    }
    out
}

fn extract_json(text: &str) -> Result<String> {
    let t = text.trim();
    let start = t
        .find('{')
        .ok_or_else(|| anyhow!("no JSON object in response: {}", t))?;
    let end = t
        .rfind('}')
        .ok_or_else(|| anyhow!("no JSON close brace in response: {}", t))?;
    if end <= start {
        return Err(anyhow!("malformed JSON in response"));
    }
    Ok(t[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_handles_fences() {
        let raw = "```json\n{\"action\":\"hold\",\"confidence\":0.5,\"position_pct\":0.0,\"reasoning\":\"x\"}\n```";
        let d: LlmDecision = serde_json::from_str(&extract_json(raw).unwrap()).unwrap();
        assert_eq!(d.action, LlmAction::Hold);
    }

    #[test]
    fn extract_json_handles_bare() {
        let raw = "{\"action\":\"open_long\",\"confidence\":0.8,\"position_pct\":0.7,\"reasoning\":\"strong setup\"}";
        let d: LlmDecision = serde_json::from_str(&extract_json(raw).unwrap()).unwrap();
        assert_eq!(d.action, LlmAction::OpenLong);
        assert_eq!(d.direction(), Direction::Long);
    }

    #[test]
    fn extract_json_with_surrounding_prose() {
        let raw = "Looking at the data...\n{\"action\":\"open_short\",\"confidence\":0.9,\"position_pct\":1.0,\"reasoning\":\"r\"}\nThat's my call.";
        let d: LlmDecision = serde_json::from_str(&extract_json(raw).unwrap()).unwrap();
        assert_eq!(d.action, LlmAction::OpenShort);
    }

    #[test]
    fn close_and_hold_both_neutral() {
        let close = LlmDecision {
            action: LlmAction::Close,
            confidence: 0.9,
            position_pct: 0.0,
            reasoning: "exit".into(),
        };
        let hold = LlmDecision {
            action: LlmAction::Hold,
            confidence: 0.5,
            position_pct: 0.0,
            reasoning: "wait".into(),
        };
        assert_eq!(close.direction(), Direction::Neutral);
        assert_eq!(hold.direction(), Direction::Neutral);
    }
}
