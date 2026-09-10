use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::DecisionOption;

use super::executor::{AgentError, AgentResult};

/// The structured shape ksforge asks Claude Code to fill via `--json-schema`
/// (see [`schema`]). One flat, discriminated-by-`status` object rather than
/// a `oneOf` — flatter shapes are more reliably honored by structured-output
/// enforcement than branching ones. Unverified against a live `claude -p`
/// run in this environment (no API credentials here) — see docs/06 for the
/// caveat; do a real smoke test before relying on this in CI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutcome {
    pub status: OutcomeStatus,
    pub summary: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub options: Vec<DecisionOption>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    /// Work done this turn, in the agent's own words (section 10/19).
    #[serde(default)]
    pub completed: Vec<String>,
    /// Work that remains and why — populated on `completed` when there are
    /// known follow-ups, and implicitly understood on `waiting_for_human`
    /// (the open item is the question itself).
    #[serde(default)]
    pub open_items: Vec<String>,
    /// Free-text rationale for `recommended_option`, or for a next step
    /// when there is no pending question. `None` only when there is
    /// genuinely no defensible preference (section 12) — never omitted to
    /// save space.
    #[serde(default)]
    pub recommendation: Option<String>,
    /// The `options[].id` the agent recommends when `waiting_for_human`.
    /// Must be one of `options`; unmatched values are ignored rather than
    /// rejected (section: never fully trust model-reported facts).
    #[serde(default)]
    pub recommended_option: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Completed,
    WaitingForHuman,
    Failed,
}

/// JSON Schema passed to `claude -p --json-schema`, constraining the final
/// turn's text to something `AgentOutcome` can deserialize.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "status": {
                "type": "string",
                "enum": ["completed", "waiting_for_human", "failed"]
            },
            "summary": { "type": "string" },
            "changed_files": {
                "type": "array",
                "items": { "type": "string" }
            },
            "question": { "type": "string" },
            "options": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string" },
                        "label": { "type": "string" }
                    },
                    "required": ["id", "label"]
                }
            },
            "failure_reason": { "type": "string" },
            "completed": {
                "type": "array",
                "items": { "type": "string" }
            },
            "open_items": {
                "type": "array",
                "items": { "type": "string" }
            },
            "recommendation": { "type": "string" },
            "recommended_option": { "type": "string" }
        },
        "required": ["status", "summary"]
    })
}

/// Extract an `AgentOutcome` from a raw agent result. Prefers the
/// pre-parsed `structured` payload the executor produced from
/// `--output-format json`'s envelope; falls back to parsing `raw_text`
/// directly (useful for a `MockAgentExecutor` in tests that skips the
/// envelope entirely).
pub fn parse(result: &AgentResult) -> Result<AgentOutcome, AgentError> {
    if let Some(value) = &result.structured {
        return serde_json::from_value(value.clone())
            .map_err(|e| AgentError::MalformedOutput(format!("structured payload: {e}")));
    }
    serde_json::from_str(&result.raw_text)
        .map_err(|e| AgentError::MalformedOutput(format!("raw text: {e}")))
}
