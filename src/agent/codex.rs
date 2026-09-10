use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

use super::executor::{AgentError, AgentExecutor, AgentRequest, AgentResult, PermissionMode};

/// Spawns the OpenAI Codex CLI (`codex`) as a subprocess — the GPT-backed
/// counterpart to [`super::ClaudeCodeExecutor`], behind the same
/// [`AgentExecutor`] port. Flags and the `--json` event shape below come
/// from developers.openai.com/codex/cli/reference and
/// developers.openai.com/codex/noninteractive (checked 2026-09-11) plus one
/// community sample of raw `--json` output; unlike `claude_code.rs`, none
/// of this has been verified against a real `codex --help` /
/// `codex exec --json` run on the build machine (no local Codex CLI
/// install). Treat exact flag/field names as best-effort until verified.
pub struct CodexExecutor {
    codex_path: PathBuf,
}

impl CodexExecutor {
    /// Resolve the Codex CLI executable: explicit path > PATH lookup.
    /// Mirrors `ClaudeCodeExecutor::discover`, minus its Windows `.cmd`-shim
    /// workaround — that workaround was confirmed against a real npm
    /// install of Claude Code; the Codex npm package's install layout has
    /// not been checked, so it is not reproduced here speculatively. If a
    /// shim causes the same "batch file arguments are invalid" failure,
    /// pass the real `.exe` explicitly.
    pub fn discover(explicit: Option<&Path>) -> Result<Self, AgentError> {
        if let Some(path) = explicit {
            if path.is_file() {
                return Ok(Self {
                    codex_path: path.to_path_buf(),
                });
            }
            return Err(AgentError::NotFound(format!(
                "{} (--codex-path). Install the Codex CLI or pass a valid --codex-path.",
                path.display()
            )));
        }
        match which::which("codex") {
            Ok(path) => Ok(Self { codex_path: path }),
            Err(_) => Err(AgentError::NotFound(
                "codex (not on PATH). Install the Codex CLI or pass --codex-path.".into(),
            )),
        }
    }
}

#[async_trait]
impl AgentExecutor for CodexExecutor {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError> {
        if request.mcp_config.is_some() {
            return Err(AgentError::Unsupported(
                "mcp_config: the Codex CLI configures MCP servers through its own \
                 $CODEX_HOME/config.toml, not a single pass-through file with a strict flag \
                 like Claude Code's --mcp-config/--strict-mcp-config"
                    .into(),
            ));
        }
        if request.max_budget_usd.is_some() {
            return Err(AgentError::Unsupported(
                "max_budget_usd: the Codex CLI has no documented spend-cap flag".into(),
            ));
        }

        let mut cmd = Command::new(&self.codex_path);
        cmd.current_dir(&request.working_directory)
            .arg("exec")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(session_id) = &request.resume_session_id {
            cmd.arg("resume").arg(session_id);
        }

        cmd.arg("--json")
            // Codex refuses to run outside a git repository by default; a
            // ksforge `--dry-run` isolated copy of the workspace need not be
            // one, and ksforge (not Codex) already owns whether the target
            // directory is a trustworthy place to run in.
            .arg("--skip-git-repo-check")
            .arg("--cd")
            .arg(&request.working_directory);

        let (sandbox, approval) = match request.permission_mode {
            // Coarser than Claude Code's Read/Grep/Glob-only allowlist:
            // Codex's `read-only` sandbox blocks filesystem writes but, per
            // the docs consulted, does not name individual permitted tools
            // the way `request.tools` does. `request.tools` itself has no
            // Codex equivalent and is intentionally not translated below.
            PermissionMode::ReadOnly => ("read-only", "never"),
            PermissionMode::AcceptEdits => ("workspace-write", "never"),
        };
        cmd.arg("--sandbox").arg(sandbox);
        cmd.arg("--ask-for-approval").arg(approval);

        if let Some(model) = &request.model {
            cmd.arg("--model").arg(model);
        }

        // No documented equivalent of Claude Code's `--append-system-prompt`;
        // folding it into the prompt text is the always-correct fallback.
        let prompt = match &request.system_prompt {
            Some(system_prompt) => format!("{system_prompt}\n\n{}", request.prompt),
            None => request.prompt.clone(),
        };

        let schema_file = request
            .json_schema
            .as_ref()
            .map(write_schema_file)
            .transpose()?;
        if let Some(file) = &schema_file {
            cmd.arg("--output-schema").arg(file.path());
        }

        let output_file = tempfile::Builder::new()
            .prefix("ksforge-codex-output-")
            .suffix(".txt")
            .tempfile()
            .map_err(AgentError::Io)?;
        cmd.arg("--output-last-message").arg(output_file.path());

        cmd.arg(&prompt);

        let output = cmd
            .output()
            .await
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(AgentError::NonZeroExit(error_detail(&stdout, &stderr)));
        }

        let raw_text = std::fs::read_to_string(output_file.path()).map_err(|e| {
            AgentError::MalformedOutput(format!(
                "reading --output-last-message file: {e}; stdout: {}",
                truncate(&stdout, 500)
            ))
        })?;

        let structured = serde_json::from_str(raw_text.trim()).ok();
        let session_id = extract_thread_id(&stdout);

        Ok(AgentResult {
            raw_text,
            structured,
            session_id,
        })
    }
}

fn write_schema_file(schema: &serde_json::Value) -> Result<tempfile::NamedTempFile, AgentError> {
    let mut file = tempfile::Builder::new()
        .prefix("ksforge-codex-schema-")
        .suffix(".json")
        .tempfile()
        .map_err(AgentError::Io)?;
    file.write_all(schema.to_string().as_bytes())
        .map_err(AgentError::Io)?;
    Ok(file)
}

/// One line of `codex exec --json`'s newline-delimited event stream.
/// `thread_id` is only present on the `thread.started` event; every other
/// event kind is ignored here.
#[derive(Debug, Deserialize)]
struct JsonlEvent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    thread_id: Option<String>,
}

fn extract_thread_id(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let event: JsonlEvent = serde_json::from_str(line).ok()?;
        (event.kind == "thread.started")
            .then_some(event.thread_id)
            .flatten()
    })
}

/// No error event type is documented for the `--json` stream; Codex streams
/// progress/failures to stderr instead, so prefer that over stdout.
fn error_detail(stdout: &str, stderr: &str) -> String {
    let detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    truncate(detail, 2000)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}... [truncated]", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_thread_id_from_the_started_event_among_other_lines() {
        let stdout = "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\"}}\n\
             {\"type\":\"thread.started\",\"thread_id\":\"019ce6ce-65fd-7530-8e6b-9ccce0436091\"}\n\
             {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1}}\n";
        assert_eq!(
            extract_thread_id(stdout),
            Some("019ce6ce-65fd-7530-8e6b-9ccce0436091".to_string())
        );
    }

    #[test]
    fn no_thread_id_when_the_stream_has_no_started_event() {
        assert_eq!(extract_thread_id("{\"type\":\"turn.completed\"}\n"), None);
    }

    #[test]
    fn error_detail_prefers_stderr_over_stdout() {
        assert_eq!(error_detail("stdout text", "stderr text"), "stderr text");
    }

    #[test]
    fn error_detail_falls_back_to_stdout_when_stderr_is_empty() {
        assert_eq!(error_detail("stdout text", ""), "stdout text");
    }
}
