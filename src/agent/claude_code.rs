use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout, Command};

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
    /// deliberately this simple per §fMSksqK — no `ksforge config` store.
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
            // Not `acceptEdits` — that name specifically pre-approves the
            // Edit/Write family, which `ExecuteOnly` must not have. `default`
            // plus the explicit `--allowedTools Bash,PowerShell` below is
            // the combination that pre-approves *only* shell execution.
            PermissionMode::ExecuteOnly => "default",
        };
        cmd.arg("--permission-mode")
            .arg(mode)
            // Never block on an interactive prompt nobody can answer; a
            // denied action surfaces as part of the agent's own summary
            // instead of hanging the process (§sTdJsQn of the base spec).
            .arg("--permission-prompts")
            .arg("none");

        if matches!(
            request.permission_mode,
            PermissionMode::AcceptEdits | PermissionMode::ExecuteOnly
        ) {
            // `acceptEdits` only pre-approves Edit/Write-family tools, not
            // the shell tool — confirmed against a real run: an `implement`
            // turn that needed `cargo build`/`cargo test` for
            // `prompts/phases/act.md`'s mandatory validation step had that
            // shell call auto-denied (no prompt answerable, see above), and
            // the agent correctly refused to claim `completed` without a
            // validation run it couldn't actually execute. Both `Bash` and
            // `PowerShell` are pre-approved — confirmed against a real run
            // on Windows that Claude Code invokes shell commands via a
            // distinct `PowerShell` tool there, not `Bash`, so allowing
            // only `Bash` left every shell call on Windows auto-denied the
            // same way — without going as far as `--permission-mode
            // bypassPermissions` (Claude Code's own docs: "recommended
            // only for sandboxes with no internet access" — too broad for
            // ksforge's typical CI runner, which does have internet
            // access).
            cmd.arg("--allowedTools").arg("Bash,PowerShell");
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
            .arg("stream-json")
            // Required by Claude Code whenever `-p`/`--output-format
            // stream-json` are combined (confirmed against a real run: it
            // refuses to start otherwise) — this is what lets `execute`
            // report live progress below instead of going silent until
            // the whole turn ends.
            .arg("--verbose")
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

        let mut child = cmd
            .spawn()
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;
        let stdout = child.stdout.take().expect("stdout was piped above");
        let stderr = child.stderr.take().expect("stderr was piped above");

        // Both pipes are drained concurrently, on their own tasks, so
        // neither can fill up and block the child while we wait on the
        // other — the same thing `Command::output()` did for us before,
        // now done by hand because we also want to react to each stdout
        // line as it arrives instead of only after the process exits.
        let stdout_task = tokio::spawn(stream_stdout(stdout));
        let stderr_task = tokio::spawn(collect_stderr(stderr));

        let status = child
            .wait()
            .await
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;
        let (raw_stdout, last_result) = stdout_task
            .await
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;
        let stderr_text = stderr_task
            .await
            .map_err(|e| AgentError::Io(std::io::Error::other(e)))?;

        if !status.success() {
            return Err(AgentError::NonZeroExit(error_detail(
                &raw_stdout,
                &stderr_text,
            )));
        }

        let envelope = last_result.ok_or_else(|| {
            AgentError::MalformedOutput(format!(
                "no `result` event in the stream-json output; raw: {}",
                truncate(&raw_stdout, 500)
            ))
        })?;

        if envelope.is_error {
            return Err(AgentError::NonZeroExit(
                envelope
                    .result
                    .unwrap_or_else(|| "Claude Code reported is_error=true".into()),
            ));
        }

        // With --json-schema set, `result` is itself the schema-constrained
        // JSON, serialized as a string. Try to parse it eagerly so callers
        // get a ready `structured` value; fall back to None (raw_text-only)
        // rather than failing the whole call, since some capabilities don't
        // request structured output at all.
        let raw_text = envelope.result.ok_or_else(|| {
            AgentError::MalformedOutput("result event had no `result` field".into())
        })?;
        let structured = serde_json::from_str(&raw_text).ok();

        Ok(AgentResult {
            raw_text,
            structured,
            session_id: envelope.session_id,
        })
    }
}

/// Reads Claude Code's `stream-json` stdout one NDJSON line at a time,
/// printing a short progress line per line to stderr as it arrives (see
/// `print_stream_event`), and remembers the last `type: "result"` event —
/// there is normally exactly one, at the very end of the stream. Returns
/// the full raw stdout too, for `error_detail`'s fallback on failure.
async fn stream_stdout(stdout: ChildStdout) -> (String, Option<ResultEnvelope>) {
    let mut lines = BufReader::new(stdout).lines();
    let mut raw = String::new();
    let mut last_result = None;

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                raw.push_str(&line);
                raw.push('\n');

                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match value.get("type").and_then(Value::as_str) {
                    Some("assistant") => print_assistant_activity(&value),
                    Some("user") => print_tool_errors(&value),
                    Some("result") => {
                        if let Ok(envelope) = serde_json::from_value::<ResultEnvelope>(value) {
                            last_result = Some(envelope);
                        }
                    }
                    _ => {}
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    (raw, last_result)
}

async fn collect_stderr(mut stderr: ChildStderr) -> String {
    let mut buf = Vec::new();
    let _ = stderr.read_to_end(&mut buf).await;
    String::from_utf8_lossy(&buf).to_string()
}

/// One assistant turn's content blocks: free text and tool calls. Printed
/// live so a long-running phase (`Act` especially) shows what it is
/// actually doing instead of going silent until it finishes.
fn print_assistant_activity(event: &Value) {
    let Some(blocks) = event.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let detail = tool_use_detail(name, block.get("input"));
                eprintln!(
                    "  {}",
                    crate::color::paint(
                        &format!("[claude] -> {name}{detail}"),
                        crate::color::Color::BrightBlack,
                        false,
                    )
                );
            }
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    let preview = first_line_preview(text, 160);
                    if !preview.is_empty() {
                        eprintln!("  [claude] {preview}");
                    }
                }
            }
            _ => {}
        }
    }
}

/// A short `name: <argument>` suffix for a tool call, e.g. `Read:
/// src/lib.rs` or `Bash: cargo test`, using whichever input field that
/// tool actually names its main argument. Unrecognized tools print with
/// no detail rather than guessing a field name that isn't there.
fn tool_use_detail(name: &str, input: Option<&Value>) -> String {
    let key = match name {
        "Bash" | "PowerShell" => "command",
        "Read" | "Edit" | "Write" | "NotebookEdit" => "file_path",
        "Grep" | "Glob" => "pattern",
        "WebFetch" | "WebSearch" => "url",
        _ => return String::new(),
    };
    match input
        .and_then(|input| input.get(key))
        .and_then(Value::as_str)
    {
        Some(value) => format!(": {}", first_line_preview(value, 120)),
        None => String::new(),
    }
}

/// A failed tool call reported back as a `user`/`tool_result` event.
/// Successful tool results are not printed — the next assistant event
/// already shows that execution moved on.
fn print_tool_errors(event: &Value) {
    let Some(blocks) = event.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for block in blocks {
        let is_error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !is_error {
            continue;
        }
        let content = block
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("tool call failed");
        eprintln!(
            "  {}",
            crate::color::paint(
                &format!("[claude] !! {}", first_line_preview(content, 160)),
                crate::color::Color::Red,
                false,
            )
        );
    }
}

/// The first non-blank line of `text`, trimmed and capped at `max`
/// characters — enough to show what is happening without dumping a whole
/// multi-paragraph message to the terminal.
fn first_line_preview(text: &str, max: usize) -> String {
    let first_line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first_line.chars().count() > max {
        let truncated: String = first_line.chars().take(max).collect();
        format!("{truncated}\u{2026}")
    } else {
        first_line.to_string()
    }
}

/// Claude Code's `--print --output-format stream-json` final `"result"`
/// event. Only the fields ksforge needs; unknown fields are ignored via
/// `#[serde(default)]` + no `deny_unknown_fields`, so a Claude Code
/// version bump that adds fields does not break parsing.
#[derive(Debug, Deserialize)]
struct ResultEnvelope {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    is_error: bool,
}

/// Picks the error text for a non-zero exit. Claude Code can exit non-zero
/// while still emitting a well-formed final `"result"` event on stdout
/// (e.g. an API-level error such as "Credit balance is too low") — surface
/// just its `result` message in that case, the same field `execute` reads
/// on success, instead of dumping the raw NDJSON. Scans from the last line
/// backward since, unlike the old single-envelope `--output-format json`,
/// `stdout`/`stderr` here can be several lines of stream-json, with the
/// result (if any) always the final one.
fn error_detail(stdout: &str, stderr: &str) -> String {
    let detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    if let Some(result) = last_result_message(detail) {
        return result;
    }
    truncate(detail, 2000)
}

fn last_result_message(text: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        serde_json::from_str::<ResultEnvelope>(line.trim())
            .ok()?
            .result
    })
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
    fn accept_edits_also_pre_approves_the_shell_tools() {
        let args = args_for(PermissionMode::AcceptEdits);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "acceptEdits",
                "--permission-prompts",
                "none",
                "--allowedTools",
                "Bash,PowerShell",
            ]
        );
    }

    #[test]
    fn read_only_does_not_get_shell_tools_pre_approved() {
        let args = args_for(PermissionMode::ReadOnly);
        assert_eq!(
            args,
            vec!["--permission-mode", "plan", "--permission-prompts", "none"]
        );
    }

    #[test]
    fn execute_only_pre_approves_shell_but_not_via_accept_edits() {
        let args = args_for(PermissionMode::ExecuteOnly);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "default",
                "--permission-prompts",
                "none",
                "--allowedTools",
                "Bash,PowerShell",
            ]
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

    /// The realistic case now: `stdout` is several NDJSON lines (a
    /// stream-json turn), not one JSON blob — the `result` event is the
    /// last line, preceded by ordinary assistant/tool activity.
    #[test]
    fn extracts_result_from_the_last_line_of_a_multi_line_stream() {
        let stdout = "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\n\
            {\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"checking budget\"}]}}\n\
            {\"type\":\"result\",\"is_error\":true,\"result\":\"Credit balance is too low\",\"session_id\":\"abc\"}\n";
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

#[cfg(test)]
mod stream_progress_tests {
    use super::{first_line_preview, tool_use_detail};
    use serde_json::json;

    #[test]
    fn tool_use_detail_reads_the_field_that_tool_actually_names() {
        assert_eq!(
            tool_use_detail("Bash", Some(&json!({"command": "cargo test"}))),
            ": cargo test"
        );
        assert_eq!(
            tool_use_detail("Read", Some(&json!({"file_path": "src/lib.rs"}))),
            ": src/lib.rs"
        );
    }

    #[test]
    fn tool_use_detail_is_empty_for_an_unrecognized_tool() {
        assert_eq!(
            tool_use_detail("SomeCustomTool", Some(&json!({"x": 1}))),
            ""
        );
    }

    #[test]
    fn tool_use_detail_is_empty_without_input() {
        assert_eq!(tool_use_detail("Bash", None), "");
    }

    #[test]
    fn first_line_preview_skips_leading_blank_lines() {
        assert_eq!(
            first_line_preview("\n\n  hello world  \nmore", 160),
            "hello world"
        );
    }

    #[test]
    fn first_line_preview_truncates_long_lines() {
        let long = "a".repeat(200);
        let preview = first_line_preview(&long, 10);
        assert_eq!(preview.chars().count(), 11); // 10 chars + the ellipsis
        assert!(preview.ends_with('\u{2026}'));
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
