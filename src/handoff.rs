//! Handing a paused execution to a Claude Code cloud session.
//!
//! When a run pauses `waiting_for_human` inside CI, the gate is durable but
//! the *conversation* is not reachable: answering means someone reading the
//! job log, then dispatching a second workflow run with `execution-id` +
//! `decision`. This module opens a second, human-facing channel for that
//! same gate by firing a claude.ai [routine] with an API trigger, which
//! returns a `claude.ai/code` session URL a human can simply open and talk
//! to — from a browser or the mobile app, days later if need be.
//!
//! It is deliberately *additive*: the gate stays open and `ksforge resume`
//! keeps working exactly as before (see `docs/06-human-in-the-loop.md`).
//! ksforge owns the workflow state; the cloud session is a place to hold
//! the conversation, never a second state machine. A handoff that fails
//! therefore never fails the run — the execution is still paused and still
//! resumable, which is the outcome that matters.
//!
//! [routine]: https://code.claude.com/docs/en/routines

use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::domain::{Execution, KsforgeError, Result};

/// Env var holding the routine's bearer token. Deliberately **not** a CLI
/// flag: a token passed as an argument is visible to every other process
/// on the runner via `ps` (§12 of CLAUDE.md — never expose credentials).
pub const TOKEN_ENV: &str = "KSFORGE_HANDOFF_TOKEN";

/// Env var overriding the beta header below. The `/fire` endpoint is in
/// research preview and Anthropic's own docs warn the header is versioned
/// and will change, so this is overridable without a ksforge release.
pub const BETA_ENV: &str = "KSFORGE_HANDOFF_BETA";

/// Beta header the routine `/fire` endpoint currently ships under.
pub const DEFAULT_BETA: &str = "experimental-cc-routine-2026-04-01";

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Everything needed to fire one routine. Built by [`Config::resolve`] so
/// that a missing token is a `Config` error (exit 2) at the point of use,
/// not a confusing HTTP 401 later.
#[derive(Debug, Clone)]
pub struct Config {
    pub routine_url: String,
    token: String,
    beta: String,
}

impl Config {
    /// `routine_url` comes from `--handoff-session`; the token only ever
    /// from the environment.
    pub fn resolve(routine_url: &str) -> Result<Self> {
        let url = routine_url.trim();
        if !url.starts_with("https://") {
            return Err(KsforgeError::Config(format!(
                "--handoff-session must be an https:// routine fire URL, got {url:?}"
            )));
        }
        let token = std::env::var(TOKEN_ENV)
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                KsforgeError::Config(format!(
                    "--handoff-session is set but {TOKEN_ENV} is empty; \
                     generate a routine token at claude.ai/code/routines \
                     and pass it as a secret"
                ))
            })?;
        let beta = std::env::var(BETA_ENV)
            .ok()
            .filter(|b| !b.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BETA.to_string());
        Ok(Self {
            routine_url: url.to_string(),
            token,
            beta,
        })
    }
}

/// The cloud session a fired routine created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffSession {
    pub session_id: String,
    pub session_url: String,
}

/// The `text` field of the fire request.
///
/// This arrives in the session wrapped in a `<routine-fire-payload>` block
/// that labels it untrusted, so the routine's *saved prompt* has to opt in
/// to acting on it — see `docs/06-human-in-the-loop.md` for the prompt to
/// paste. Everything a human needs to answer the gate is therefore spelled
/// out here rather than assumed to be in the routine's context: it may be
/// read by a session that has only just cloned the repository.
///
/// Pure and side-effect free so the wire format is unit-testable without a
/// network, same contract as `github::report::render`.
pub fn payload_for(execution: &Execution) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "A ksforge execution paused because Claude Code needs a human decision.\n\n\
         Execution: {}\n\
         Capability: {}\n",
        execution.id, execution.capability
    ));
    if let Some(first) = execution.change_request.text.lines().next() {
        s.push_str(&format!("Change request: {first}\n"));
    }

    let Some(gate) = execution.open_gate() else {
        // Callers gate on `open_gate()` before firing; keep the payload
        // truthful rather than inventing a question if that ever changes.
        s.push_str("\nNo open gate was recorded on this execution.\n");
        return s;
    };

    s.push_str(&format!("Gate: {}\n\n", gate.id));
    s.push_str(&format!("Question:\n{}\n", gate.question));

    if !gate.options.is_empty() {
        s.push_str("\nOptions:\n");
        for option in &gate.options {
            s.push_str(&format!("- {} — {}\n", option.id, option.label));
        }
    }
    if let Some(recommended) = &gate.recommended_option {
        s.push_str(&format!("\nRecommended option: {recommended}\n"));
    }
    if !gate.context.is_empty() {
        s.push_str(&format!("Rationale: {}\n", gate.context));
    }
    if !gate.completed.is_empty() {
        s.push_str("\nAlready completed before the gate:\n");
        for item in &gate.completed {
            s.push_str(&format!("- {item}\n"));
        }
    }

    s.push_str(&format!(
        "\nTo apply a decision, run this from a checkout of the repository:\n\
         ksforge resume {} --decision <option-id>\n",
        execution.id
    ));
    s
}

/// Read a `/fire` response. A body without `claude_code_session_url` is an
/// error carrying the body itself — API errors come back as JSON too, and
/// their message is the only useful diagnostic the caller will get.
pub fn parse_fire_response(body: &str) -> Result<HandoffSession> {
    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| {
        KsforgeError::Config(format!(
            "routine fire returned a non-JSON response: {}",
            truncate(body, 300)
        ))
    })?;
    let url = parsed
        .get("claude_code_session_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            KsforgeError::Config(format!(
                "routine fire returned no session URL: {}",
                truncate(body, 300)
            ))
        })?;
    let id = parsed
        .get("claude_code_session_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    Ok(HandoffSession {
        session_id: id.to_string(),
        session_url: url.to_string(),
    })
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// POST the execution's gate to the routine's `/fire` endpoint.
///
/// Shells out to `curl` rather than pulling in an HTTP client + TLS stack:
/// ksforge already treats external CLIs as its transport (`git`, `gh`,
/// `claude`), and a routine fire is one request on a code path that only
/// runs when a gate opens. Unlike `github::process`, the spawn is done here
/// so failures land in the right error domain — a routine fire is not a
/// GitHub integration error.
///
/// Both the token and the payload are kept out of the argument list: the
/// token is written to curl's stdin as a header (`-H @-`, curl 7.55+, the
/// form Anthropic's own docs use) and the JSON body to a temp file, so
/// neither shows up in `ps` output or a runner's process listing.
pub async fn fire(cwd: &Path, config: &Config, execution: &Execution) -> Result<HandoffSession> {
    let body = serde_json::json!({ "text": payload_for(execution) });
    let mut payload_file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut payload_file, serde_json::to_string(&body)?.as_bytes())?;
    std::io::Write::flush(&mut payload_file)?;
    let payload_arg = format!("@{}", payload_file.path().display());

    let mut child = Command::new("curl")
        .arg("-sS")
        .arg("-X")
        .arg("POST")
        .arg("-H")
        .arg("@-")
        .arg("-H")
        .arg(format!("anthropic-beta: {}", config.beta))
        .arg("-H")
        .arg(format!("anthropic-version: {ANTHROPIC_VERSION}"))
        .arg("-H")
        .arg("content-type: application/json")
        .arg("--data-binary")
        .arg(&payload_arg)
        .arg(&config.routine_url)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            KsforgeError::Config(format!("failed to run curl for the routine fire: {e}"))
        })?;

    {
        let mut stdin = child.stdin.take().expect("stdin was piped above");
        tokio::io::AsyncWriteExt::write_all(
            &mut stdin,
            format!("Authorization: Bearer {}\n", config.token).as_bytes(),
        )
        .await?;
        tokio::io::AsyncWriteExt::shutdown(&mut stdin).await?;
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(KsforgeError::Config(format!(
            "routine fire failed: curl exited {}: {}",
            output.status,
            truncate(&String::from_utf8_lossy(&output.stderr), 300)
        )));
    }
    parse_fire_response(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChangeRequest, DecisionOption};

    fn waiting_execution() -> Execution {
        let cr = ChangeRequest::from_text("As a user, I want to reset my password.").unwrap();
        let mut exec = Execution::start(cr, "implement");
        exec.ask(
            "OAuth2 or JWT?".into(),
            vec![
                DecisionOption {
                    id: "oauth2".into(),
                    label: "OAuth 2".into(),
                },
                DecisionOption {
                    id: "jwt".into(),
                    label: "JWT".into(),
                },
            ],
            Some("oauth2".into()),
            "The repo already has an OAuth-compatible identity boundary.".into(),
            vec!["Analyzed the authentication module.".into()],
        );
        exec
    }

    #[test]
    fn payload_is_self_contained() {
        let exec = waiting_execution();
        let payload = payload_for(&exec);

        // A session that has only just cloned the repo must be able to act
        // on this without any other context.
        assert!(payload.contains(&exec.id.0));
        assert!(payload.contains("OAuth2 or JWT?"));
        assert!(payload.contains("oauth2 — OAuth 2"));
        assert!(payload.contains("jwt — JWT"));
        assert!(payload.contains("Recommended option: oauth2"));
        assert!(payload.contains("Analyzed the authentication module."));
        assert!(payload.contains(&format!("ksforge resume {} --decision", exec.id)));
    }

    #[test]
    fn payload_without_a_gate_invents_no_question() {
        let cr = ChangeRequest::from_text("As a user...").unwrap();
        let exec = Execution::start(cr, "implement");
        let payload = payload_for(&exec);
        assert!(payload.contains("No open gate"));
        assert!(!payload.contains("Question:"));
    }

    #[test]
    fn parses_a_successful_fire() {
        let session = parse_fire_response(
            r#"{"type":"routine_fire",
                "claude_code_session_id":"session_01H",
                "claude_code_session_url":"https://claude.ai/code/session_01H"}"#,
        )
        .unwrap();
        assert_eq!(session.session_id, "session_01H");
        assert_eq!(session.session_url, "https://claude.ai/code/session_01H");
    }

    #[test]
    fn an_api_error_body_is_surfaced_verbatim() {
        let err =
            parse_fire_response(r#"{"type":"error","error":{"message":"not found"}}"#).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn a_non_json_body_is_not_a_panic() {
        let err = parse_fire_response("<html>502 Bad Gateway</html>").unwrap_err();
        assert!(err.to_string().contains("non-JSON"), "got: {err}");
    }

    #[test]
    fn a_non_https_url_is_rejected_before_any_request() {
        let err = Config::resolve("http://example.invalid/fire").unwrap_err();
        assert!(matches!(err, KsforgeError::Config(_)));
    }
}
