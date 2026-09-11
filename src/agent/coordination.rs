use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::executor::{AgentError, AgentResult};

/// What the coordination agent (`prompts/coordinator/system-prompt.md`,
/// `application::coordinate`) reports about a new change request relative
/// to the workspace's other active executions — its §Bwuxxi7 "Coordination
/// Output", minus `execution_id` (this run never starts an `Execution` of
/// its own, so there is none to report). Advisory only: `proceed: false`
/// is a recommendation ksforge surfaces to the caller, not something this
/// command enforces itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationDecision {
    pub classification: Classification,
    #[serde(default)]
    pub affected_scope: Vec<String>,
    #[serde(default)]
    pub related_executions: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub recommendations: Vec<String>,
    pub proceed: bool,
}

/// §NB0Aevz of the coordinator prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Classification {
    Low,
    Medium,
    High,
    Blocked,
}

/// JSON Schema passed to `--json-schema`, constraining the coordination
/// agent's final turn to something `CoordinationDecision` can deserialize
/// — same mechanism as `agent::outcome::schema`, and for the same reason
/// (section: deterministic, schema-validated JSON, not free text).
pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "classification": {
                "type": "string",
                "enum": ["LOW", "MEDIUM", "HIGH", "BLOCKED"]
            },
            "affected_scope": {
                "type": "array",
                "items": { "type": "string" }
            },
            "related_executions": {
                "type": "array",
                "items": { "type": "string" }
            },
            "dependencies": {
                "type": "array",
                "items": { "type": "string" }
            },
            "conflicts": {
                "type": "array",
                "items": { "type": "string" }
            },
            "recommendations": {
                "type": "array",
                "items": { "type": "string" }
            },
            "proceed": { "type": "boolean" }
        },
        "required": ["classification", "proceed"]
    })
}

/// Mirrors `agent::outcome::parse` — prefers the executor's pre-parsed
/// `structured` payload, falls back to parsing `raw_text` directly (a
/// `MockAgentExecutor` in tests skips the envelope entirely).
pub fn parse(result: &AgentResult) -> Result<CoordinationDecision, AgentError> {
    if let Some(value) = &result.structured {
        return serde_json::from_value(value.clone())
            .map_err(|e| AgentError::MalformedOutput(format!("structured payload: {e}")));
    }
    serde_json::from_str(&result.raw_text)
        .map_err(|e| AgentError::MalformedOutput(format!("raw text: {e}")))
}
