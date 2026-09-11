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
            return Err(AgentError::NotFound(format!(
                "{} (--claude-path). Install Claude Code or pass a valid --claude-path.",
                path.display()
            )));
        }
        match which::which("claude") {
            Ok(path) => Ok(Self {
                claude_path: resolve_windows_shim(path),
            }),
            Err(_) => Err(AgentError::NotFound(
                "claude (not on PATH). Install Claude Code or pass --claude-path.".into(),
            )),
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

        if request.permission_mode == PermissionMode::AcceptEdits {
            // `acceptEdits` only pre-approves Edit/Write-family tools, not
            // Bash — confirmed against a real run: an `implement` turn
            // that needed `cargo build`/`cargo test` for §21's mandatory
            // validation step had that Bash call auto-denied (no prompt
            // answerable, see above), and the agent correctly refused to
            // claim `completed` without a validation run it couldn't
            // actually execute. `--allowedTools Bash` pre-approves Bash
            // specifically, without going as far as `--permission-mode
            // bypassPermissions` (Claude Code's own docs: "recommended
            // only for sandboxes with no internet access" — too broad for
            // ksforge's typical CI runner, which does have internet
            // access).
            cmd.arg("--allowedTools").arg("Bash");
        }
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
            return Err(AgentError::NonZeroExit(error_detail(&stdout, &stderr)));
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

/// Picks the error text for a non-zero exit. Claude Code can exit non-zero
/// while still emitting a well-formed `--output-format json` envelope on
/// stdout (e.g. an API-level error such as "Credit balance is too low") —
/// surface just its `result` message in that case, the same field
/// `parse_envelope` reads on success, instead of dumping the raw envelope.
fn error_detail(stdout: &str, stderr: &str) -> String {
    let detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    if let Ok(envelope) = serde_json::from_str::<ResultEnvelope>(detail.trim())
        && let Some(result) = envelope.result
    {
        return result;
    }
    truncate(detail, 2000)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}... [truncated]", &s[..max])
    }
}

/// npm's global `claude` on Windows is a `.cmd` shim
/// (`%dp0%\node_modules\@anthropic-ai\claude-code\bin\claude.exe %*`) that
/// just forwards to a real `.exe` sitting next to it. Spawning the shim
/// directly makes Windows re-invoke it through `cmd.exe`'s own batch-file
/// argument parser, which mishandles a long, quote-heavy argument (ksforge's
/// system prompt easily exceeds what it tolerates) and fails with "batch
/// file arguments are invalid" — confirmed against a real global npm
/// install. Resolving the real `.exe` and spawning it directly avoids
/// `cmd.exe` entirely, the same workaround `tools/palau-test`'s
/// `runClaudeAgent.ts` uses. A no-op on other platforms and for any other
/// install layout (falls back to `path` unchanged), so an unusual setup
/// still works, just without this optimization.
#[cfg(windows)]
fn resolve_windows_shim(path: PathBuf) -> PathBuf {
    let Some(dir) = path.parent() else {
        return path;
    };
    let exe = dir
        .join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join("claude.exe");
    if exe.is_file() { exe } else { path }
}

#[cfg(not(windows))]
fn resolve_windows_shim(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
mod permission_args_tests {
    use super::*;

    fn request(permission_mode: PermissionMode) -> AgentRequest {
        AgentRequest {
            prompt: String::new(),
            system_prompt: None,
            working_directory: PathBuf::from("."),
            tools: Vec::new(),
            model: None,
            permission_mode,
            json_schema: None,
            max_budget_usd: None,
            resume_session_id: None,
            mcp_config: None,
        }
    }

    fn args_for(permission_mode: PermissionMode) -> Vec<String> {
        let mut cmd = Command::new("true");
        ClaudeCodeExecutor::permission_args(&request(permission_mode), &mut cmd);
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn accept_edits_also_pre_approves_bash() {
        let args = args_for(PermissionMode::AcceptEdits);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "acceptEdits",
                "--permission-prompts",
                "none",
                "--allowedTools",
                "Bash",
            ]
        );
    }

    #[test]
    fn read_only_does_not_get_bash_pre_approved() {
        let args = args_for(PermissionMode::ReadOnly);
        assert_eq!(
            args,
            vec!["--permission-mode", "plan", "--permission-prompts", "none"]
        );
    }
}

#[cfg(test)]
mod error_detail_tests {
    use super::error_detail;

    #[test]
    fn extracts_result_from_a_json_envelope_on_stdout() {
        let stdout = r#"{"type":"result","is_error":true,"result":"Credit balance is too low","session_id":"abc"}"#;
        assert_eq!(error_detail(stdout, ""), "Credit balance is too low");
    }

    #[test]
    fn falls_back_to_raw_text_when_not_a_json_envelope() {
        assert_eq!(
            error_detail("", "command not found: claude"),
            "command not found: claude"
        );
    }

    #[test]
    fn falls_back_to_raw_json_when_envelope_has_no_result_field() {
        let stdout = r#"{"type":"result","is_error":true,"session_id":"abc"}"#;
        assert_eq!(error_detail(stdout, ""), stdout);
    }
}

#[cfg(all(test, windows))]
mod windows_shim_tests {
    use super::resolve_windows_shim;

    #[test]
    fn resolves_the_real_exe_next_to_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir
            .path()
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let exe = bin_dir.join("claude.exe");
        std::fs::write(&exe, b"").unwrap();

        let shim = dir.path().join("claude.cmd");
        std::fs::write(&shim, b"").unwrap();

        assert_eq!(resolve_windows_shim(shim), exe);
    }

    #[test]
    fn falls_back_to_the_given_path_without_a_sibling_exe() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("claude");
        std::fs::write(&shim, b"").unwrap();

        assert_eq!(resolve_windows_shim(shim.clone()), shim);
    }
}
