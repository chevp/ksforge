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
}

/// A controlled request to the execution engine. Deliberately small and
/// free of ksforge domain types (section 11): this is the boundary Claude
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
    #[error(
        "Claude Code executable not found (looked for: {0}). Install Claude Code or pass --claude-path."
    )]
    NotFound(String),

    #[error("Claude Code exited with a non-zero status: {0}")]
    NonZeroExit(String),

    #[error("Claude Code produced output that could not be parsed: {0}")]
    MalformedOutput(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Port to the coding/agent execution engine. `ksforge` never implements a
/// competing agent loop behind this trait — the only production impl spawns
/// the real Claude Code CLI (section 2/3).
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError>;
}
