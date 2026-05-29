//! LLM arbiter — calls the Claude Messages API with a structured market snapshot
//! and returns the final trade decision.
//!
//! Replaces the daemon's weighted-vote aggregator + anti-whipsaw guards in
//! `--llm` mode. The existing heuristic agents stay as data sources whose
//! signals feed into this prompt (v1 — see #14). v2 will drop the heuristic
//! signal logic entirely.

use std::env;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{Direction, Signal};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-4-7";
const TIMEOUT_SECS: u64 = 10;
const MAX_TOKENS: u32 = 512;

const SYSTEM_PROMPT: &str = "You are a Bitcoin perpetual futures trader on LN Markets. \
You trade with strict risk discipline.

Decision principles:
- Trade theses, not news reactions. Don't trade on a headline alone.
- Skip ambiguous markets — return action=\"hold\" when conviction is low.
- If you already hold a position aligned with the current thesis, prefer hold over re-entering.
- If you hold a position against the current thesis, return action=\"close\".
- position_pct scales with conviction: 0.3 marginal, 0.7 strong, 1.0 high-conviction only.

Respond with valid JSON only — no markdown fence, no prose around it:
{
  \"action\": \"open_long\" | \"open_short\" | \"close\" | \"hold\",
  \"confidence\": 0.0-1.0,
  \"position_pct\": 0.0-1.0,
  \"reasoning\": \"one to three sentences explaining the call\"
}";

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
    /// Map the LLM's symbolic action onto the daemon's Direction enum.
    /// Close + Hold both map to Neutral — explicit closes are handled
    /// in the daemon based on whether a position currently exists.
    pub fn direction(&self) -> Direction {
        match self.action {
            LlmAction::OpenLong => Direction::Long,
            LlmAction::OpenShort => Direction::Short,
            LlmAction::Close | LlmAction::Hold => Direction::Neutral,
        }
    }
}

/// Minimal description of an open position passed into the prompt.
pub struct PositionBrief {
    pub side: Direction,
    pub size_usd: f64,
    pub entry_price: f64,
    pub pl_pct: f64,
}

/// All inputs the LLM sees on each cycle.
pub struct MarketSnapshot<'a> {
    pub price: f64,
    pub change_24h_pct: Option<f64>,
    pub change_1h_pct: Option<f64>,
    pub funding_rate: Option<f64>,
    pub signals: &'a [Signal],
    pub current_position: Option<&'a PositionBrief>,
    pub max_position_usd: u64,
    pub leverage: u32,
    pub mode: &'a str,
}

pub struct LlmArbiter {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl LlmArbiter {
    /// Build an arbiter from environment. Requires ANTHROPIC_API_KEY.
    /// Model from ANTHROPIC_MODEL or the default.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow!("ANTHROPIC_API_KEY not set — required for --llm mode"))?;
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

        let json_str = extract_json(text)?;
        let mut decision: LlmDecision = serde_json::from_str(&json_str)
            .with_context(|| format!("parse decision JSON: {}", json_str))?;
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
        "Constraints: max position ${} USD, leverage {}x, mode={}\n\n",
        s.max_position_usd, s.leverage, s.mode
    ));

    out.push_str(&format!("Price: ${:.0}", s.price));
    if let Some(c) = s.change_24h_pct {
        out.push_str(&format!(" ({:+.2}% 24h", c));
        if let Some(c1) = s.change_1h_pct {
            out.push_str(&format!(", {:+.2}% 1h", c1));
        }
        out.push(')');
    }
    out.push('\n');
    if let Some(f) = s.funding_rate {
        out.push_str(&format!("Funding rate: {:+.4}%\n", f * 100.0));
    }
    out.push('\n');

    if let Some(p) = s.current_position {
        out.push_str(&format!(
            "Current position: {} ${:.0} entered at ${:.0}, P&L {:+.2}%\n\n",
            p.side, p.size_usd, p.entry_price, p.pl_pct
        ));
    } else {
        out.push_str("Current position: none\n\n");
    }

    if s.signals.is_empty() {
        out.push_str("Agent signals: none\n");
    } else {
        out.push_str("Agent signals (data only — your job is to decide):\n");
        for sig in s.signals {
            out.push_str(&format!(
                "  - {} ({} {:.0}%): {}\n",
                sig.source.to_uppercase(),
                sig.direction,
                sig.confidence * 100.0,
                sig.reasoning,
            ));
        }
    }

    out
}

/// Strip an optional markdown fence and slice to the outer-most JSON object.
/// Tolerates the model emitting `\`\`\`json … \`\`\`` despite the instruction.
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
        let s = extract_json(raw).unwrap();
        let d: LlmDecision = serde_json::from_str(&s).unwrap();
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
