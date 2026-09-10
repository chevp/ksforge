use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use super::executor::{AgentError, AgentExecutor, AgentRequest, AgentResult, PermissionMode};

/// Spawns the real Claude Code CLI (`claude`) as a subprocess. This is the
/// only place in ksforge that knows Claude Code's command-line interface —
/// see docs/03-architecture.md for which flags were verified against a real
/// `claude --help` on the build machine vs. inferred from documentation.
pub struct ClaudeCodeExecutor {
    claude_path: PathBuf,
}

impl ClaudeCodeExecutor {
    /// Resolve the Claude Code executable: explicit path > `KSFORGE_CLAUDE_PATH`
    /// env (handled by the caller via clap's `env`) > PATH lookup. Kept
    /// deliberately this simple per section 32 — no `ksforge config` store.
    pub fn discover(explicit: Option<&Path>) -> Result<Self, AgentError> {
        if let Some(path) = explicit {
            if path.is_file() {
                return Ok(Self {
                    claude_path: path.to_path_buf(),
                });
            }
            return Err(AgentError::NotFound(path.display().to_string()));
        }
        match which::which("claude") {
            Ok(path) => Ok(Self { claude_path: path }),
            Err(_) => Err(AgentError::NotFound("claude (not on PATH)".into())),
        }
    }

    fn tools_args(request: &AgentRequest, cmd: &mut Command) {
        if !request.tools.is_empty() {
            cmd.arg("--tools").arg(request.tools.join(","));
        }
    }

    fn permission_args(request: &AgentRequest, cmd: &mut Command) {
        let mode = match request.permission_mode {
            PermissionMode::ReadOnly => "plan",
            PermissionMode::AcceptEdits => "acceptEdits",
        };
        cmd.arg("--permission-mode")
            .arg(mode)
            // Never block on an interactive prompt nobody can answer; a
            // denied action surfaces as part of the agent's own summary
            // instead of hanging the process (section 15 of the base spec).
            .arg("--permission-prompts")
            .arg("none");
    }
}

#[async_trait]
impl AgentExecutor for ClaudeCodeExecutor {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError> {
        let mut cmd = Command::new(&self.claude_path);
        cmd.current_dir(&request.working_directory)
            .arg("-p")
            .arg("--output-format")
            .arg("json")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Self::tools_args(&request, &mut cmd);
        Self::permission_args(&request, &mut cmd);

        if let Some(model) = &request.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(system_prompt) = &request.system_prompt {
            cmd.arg("--append-system-prompt").arg(system_prompt);
        }
        if let Some(schema) = &request.json_schema {
            cmd.arg("--json-schema").arg(schema.to_string());
        }
        if let Some(budget) = request.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(budget.to_string());
        }
        if let Some(session_id) = &request.resume_session_id {
            cmd.arg("--resume").arg(session_id);
        }
        if let Some(mcp_config) = &request.mcp_config {
            // `--strict-mcp-config` always accompanies it: only the servers
            // named in that file are available, never anything picked up
            // from a user/global Claude Code config (section: least
            // privilege, docs/09-security.md).
            cmd.arg("--mcp-config")
                .arg(mcp_config)
                .arg("--strict-mcp-config");
        }

        cmd.arg(&request.prompt);

        let output = cmd
            .output()
            .await
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let detail = if stderr.trim().is_empty() {
                stdout.clone()
            } else {
                stderr
            };
            return Err(AgentError::NonZeroExit(truncate(&detail, 2000)));
        }

        parse_envelope(&stdout)
    }
}

/// Claude Code's `--print --output-format json` envelope. Only the fields
/// ksforge needs; unknown fields are ignored via `#[serde(default)]` +
/// no `deny_unknown_fields`, so a Claude Code version bump that adds fields
/// does not break parsing.
#[derive(Debug, Deserialize)]
struct ResultEnvelope {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    is_error: bool,
}

fn parse_envelope(stdout: &str) -> Result<AgentResult, AgentError> {
    let envelope: ResultEnvelope = serde_json::from_str(stdout.trim()).map_err(|e| {
        AgentError::MalformedOutput(format!("envelope: {e}; raw: {}", truncate(stdout, 500)))
    })?;

    if envelope.is_error {
        return Err(AgentError::NonZeroExit(
            envelope
                .result
                .unwrap_or_else(|| "Claude Code reported is_error=true".into()),
        ));
    }

    let raw_text = envelope
        .result
        .ok_or_else(|| AgentError::MalformedOutput("envelope had no `result` field".into()))?;

    // With --json-schema set, `result` is itself the schema-constrained
    // JSON, serialized as a string. Try to parse it eagerly so callers get
    // a ready `structured` value; fall back to None (raw_text-only) rather
    // than failing the whole call, since some capabilities don't request
    // structured output at all.
    let structured = serde_json::from_str(&raw_text).ok();

    Ok(AgentResult {
        raw_text,
        structured,
        session_id: envelope.session_id,
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}... [truncated]", &s[..max])
    }
}
