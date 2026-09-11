use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

/// How permissive the spawned agent session is. Mapped to Claude Code's
/// `--permission-mode` / `--tools` / `--permission-prompts` flags by
/// `ClaudeCodeExecutor` — see docs/03-architecture.md for the exact mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Never allowed to edit; unattended prompts are auto-denied.
    ReadOnly,
    /// Edits auto-accepted; unattended prompts are auto-denied (never
    /// hangs waiting for a human that CI cannot provide).
    AcceptEdits,
    /// Shell execution pre-approved, same as `AcceptEdits`, but Edit/Write
    /// are not offered as tools at all (see `AgentRequest.tools`) — for the
    /// VALIDATE turn: run whatever build/test/packaging the change needs to
    /// be checked, without being able to change code any further. Enforced
    /// twice over: `application::execute` also diffs the workspace after
    /// this turn and fails the run if anything changed regardless of what
    /// the executor actually permitted (never fully trust the executor
    /// alone for this).
    ExecuteOnly,
}

/// A controlled request to the execution engine. Deliberately small and
/// free of ksforge domain types (§nU95phP): this is the boundary Claude
/// Code sees, not the boundary the rest of ksforge reasons in.
#[derive(Debug, Clone)]
pub struct AgentRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub working_directory: PathBuf,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    pub json_schema: Option<Value>,
    pub max_budget_usd: Option<f64>,
    /// Resume an existing Claude Code conversation instead of starting a
    /// new one (see docs/06-human-in-the-loop.md).
    pub resume_session_id: Option<String>,
    /// Path to a Claude Code `--mcp-config` file (its own JSON format, not
    /// a ksforge one — see docs/11-integrations.md), passed through
    /// unmodified together with `--strict-mcp-config`. `None` by default:
    /// no MCP servers beyond Claude Code's own tools.
    pub mcp_config: Option<PathBuf>,
}

/// What came back from one agent turn.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// Raw final-turn text, always present.
    pub raw_text: String,
    /// Parsed `--json-schema`-constrained payload, when the executor could
    /// extract one.
    pub structured: Option<Value>,
    /// Claude Code session id, when reported, for later `--resume`.
    pub session_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    /// The executor's payload is the whole message, including any
    /// install/`--*-path` hint.
    #[error("agent executable not found: {0}")]
    NotFound(String),

    #[error("agent process exited with a non-zero status: {0}")]
    NonZeroExit(String),

    #[error("agent produced output that could not be parsed: {0}")]
    MalformedOutput(String),

    /// A request field this executor has no way to honor. Raised instead of
    /// silently dropping the field, per docs/09-security.md (least
    /// privilege): an unenforced restriction must fail loud, not quietly
    /// run with weaker guarantees than the caller asked for.
    #[error("not supported by this executor: {0}")]
    Unsupported(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl AgentError {
    /// Whether the very same request might succeed on a second try — a
    /// network blip or a provider-side rate limit/overload — as opposed to
    /// a failure retrying would only reproduce identically.
    /// `NotFound`/`Unsupported` are static misconfiguration; `MalformedOutput`
    /// reflects the model's own output, not infrastructure, and must not be
    /// retried (see `tests/execution_pipeline.rs`'s
    /// `a_malformed_phase_response_fails_the_execution_without_retrying`).
    /// Claude Code exposes no structured transient/permanent distinction on
    /// a non-zero exit, so `NonZeroExit` falls back to a keyword sniff over
    /// the CLI's own error text.
    pub fn is_transient(&self) -> bool {
        match self {
            AgentError::Io(_) => true,
            AgentError::NonZeroExit(detail) => is_transient_detail(detail),
            AgentError::NotFound(_)
            | AgentError::MalformedOutput(_)
            | AgentError::Unsupported(_) => false,
        }
    }
}

fn is_transient_detail(detail: &str) -> bool {
    const MARKERS: &[&str] = &[
        "rate limit",
        "rate_limit",
        "overloaded",
        "timed out",
        "timeout",
        "connection reset",
        "econnreset",
        "socket hang up",
        "temporarily unavailable",
        "service unavailable",
        "502",
        "503",
        "529",
    ];
    let lower = detail.to_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Port to the coding/agent execution engine. `ksforge` never implements a
/// competing agent loop behind this trait — the one production impl,
/// `ClaudeCodeExecutor`, spawns the real `claude` CLI (§CnK6mQd/§nLFQ2PQ).
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError>;
}

#[cfg(test)]
mod transient_tests {
    use super::*;

    #[test]
    fn io_errors_are_transient() {
        let err = AgentError::Io(std::io::Error::other("boom"));
        assert!(err.is_transient());
    }

    #[test]
    fn non_zero_exit_with_a_rate_limit_message_is_transient() {
        let err = AgentError::NonZeroExit("Error: rate_limit_error, please retry".into());
        assert!(err.is_transient());
    }

    #[test]
    fn non_zero_exit_with_a_billing_message_is_not_transient() {
        let err = AgentError::NonZeroExit("Credit balance is too low".into());
        assert!(!err.is_transient());
    }

    #[test]
    fn not_found_is_never_transient() {
        assert!(!AgentError::NotFound("claude (not on PATH)".into()).is_transient());
    }

    #[test]
    fn malformed_output_is_never_transient() {
        assert!(!AgentError::MalformedOutput("no result event".into()).is_transient());
    }

    #[test]
    fn unsupported_is_never_transient() {
        assert!(!AgentError::Unsupported("mcp_config".into()).is_transient());
    }
}
