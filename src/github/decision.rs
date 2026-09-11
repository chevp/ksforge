use std::path::Path;

use crate::domain::Result;

use super::process::run_gh;

/// Extract the option id from a `/ksforge choose <option>` comment body, if
/// present anywhere in it (section 16). Deliberately no `regex` dependency
/// — the command has exactly one fixed shape, `strip_prefix` covers it.
pub fn parse_choose_command(body: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("/ksforge choose") {
            let option = rest.trim();
            if !option.is_empty() && !option.contains(char::is_whitespace) {
                return Some(option.to_string());
            }
        }
    }
    None
}

/// A `/ksforge implement <change-request>` or `/ksforge fix <change-request>`
/// PR comment, asking ksforge to run that capability against the PR's own
/// branch and push the result back onto it
/// (`github::pull_request::push_follow_up`) — distinct from `/ksforge choose
/// <option>` above, which only ever resumes an existing paused execution and
/// never starts a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpCommand {
    pub capability: String,
    pub change_request: String,
}

/// Capabilities a PR comment may trigger this way — `review`/`explain`
/// never write, so there would be nothing to push back for them (same
/// read/write split as `domain::capability::ToolPolicy`).
const FOLLOW_UP_CAPABILITIES: &[&str] = &["implement", "fix"];

/// The trigger must be the whole of its line (optionally followed by the
/// start of the change request, inline) — not merely a substring anywhere
/// in a sentence, since unlike `/ksforge choose <option>` (self-limiting:
/// it must also name one of the agent's own offered options), matching
/// here starts a real, write-capable, billed agent run. The change request
/// is the trigger line's own inline remainder, if any, plus every line
/// after it — comment bodies are real multi-line text (a GitHub
/// `<textarea>`, not a single-line `workflow_dispatch` input), so no
/// reflow/escaping is needed.
pub fn parse_follow_up_command(body: &str) -> Option<FollowUpCommand> {
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        for capability in FOLLOW_UP_CAPABILITIES {
            let prefix = format!("/ksforge {capability}");
            let inline = if trimmed == prefix {
                Some("")
            } else {
                trimmed.strip_prefix(&format!("{prefix} ")).map(str::trim)
            };
            let Some(inline) = inline else { continue };

            let mut change_request_lines: Vec<&str> = Vec::new();
            if !inline.is_empty() {
                change_request_lines.push(inline);
            }
            change_request_lines.extend(lines.clone());
            let change_request = change_request_lines.join("\n").trim().to_string();
            if change_request.is_empty() {
                return None;
            }
            return Some(FollowUpCommand {
                capability: capability.to_string(),
                change_request,
            });
        }
    }
    None
}

/// Re-check the commenter's own repository permission via the GitHub API,
/// independent of whatever the invoking workflow's `if:` already checked
/// (section 23: a comment is never trusted as workflow authority on its
/// own — defense in depth, see docs/09-security.md). `true` only for
/// `admin`/`write`; anything else — including a failed/not-found lookup —
/// is treated as unauthorized, never as an error to propagate.
pub async fn authorize_commenter(workspace_root: &Path, login: &str) -> Result<bool> {
    let output = match run_gh(
        workspace_root,
        &[
            "api",
            &format!("repos/{{owner}}/{{repo}}/collaborators/{login}/permission"),
        ],
    )
    .await
    {
        Ok(output) => output,
        Err(_) => return Ok(false),
    };
    let value: serde_json::Value = serde_json::from_str(&output)?;
    Ok(matches!(
        value.get("permission").and_then(|p| p.as_str()),
        Some("admin") | Some("write")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_bare_choose_command() {
        assert_eq!(
            parse_choose_command("/ksforge choose oauth2"),
            Some("oauth2".into())
        );
    }

    #[test]
    fn parses_a_choose_command_among_other_text() {
        let body = "Sounds good, let's go with that.\n\n/ksforge choose oauth2\n\nthanks!";
        assert_eq!(parse_choose_command(body), Some("oauth2".into()));
    }

    #[test]
    fn ignores_unrelated_comments() {
        assert_eq!(parse_choose_command("looks good to me"), None);
        assert_eq!(parse_choose_command("/ksforge choose"), None);
        assert_eq!(parse_choose_command("/ksforge choose  "), None);
    }

    #[test]
    fn parses_an_inline_follow_up_command() {
        assert_eq!(
            parse_follow_up_command("/ksforge fix the button is misaligned on mobile"),
            Some(FollowUpCommand {
                capability: "fix".into(),
                change_request: "the button is misaligned on mobile".into(),
            })
        );
    }

    #[test]
    fn parses_a_multiline_follow_up_command_with_a_bare_trigger_line() {
        let body =
            "/ksforge implement\n\nAlso add a CHANGELOG entry for this.\n- and update the README";
        assert_eq!(
            parse_follow_up_command(body),
            Some(FollowUpCommand {
                capability: "implement".into(),
                change_request: "Also add a CHANGELOG entry for this.\n- and update the README"
                    .into(),
            })
        );
    }

    #[test]
    fn combines_inline_text_with_following_lines() {
        let body = "/ksforge fix crashes on startup\n\nStack trace:\nfoo.rs:42";
        assert_eq!(
            parse_follow_up_command(body),
            Some(FollowUpCommand {
                capability: "fix".into(),
                change_request: "crashes on startup\n\nStack trace:\nfoo.rs:42".into(),
            })
        );
    }

    #[test]
    fn ignores_a_bare_trigger_with_no_change_request_at_all() {
        assert_eq!(parse_follow_up_command("/ksforge fix"), None);
        assert_eq!(parse_follow_up_command("/ksforge fix\n\n   \n"), None);
    }

    #[test]
    fn does_not_false_match_a_word_that_merely_starts_with_the_capability_name() {
        assert_eq!(
            parse_follow_up_command("/ksforge implementation details below"),
            None
        );
    }

    #[test]
    fn does_not_confuse_a_choose_command_with_a_follow_up() {
        assert_eq!(parse_follow_up_command("/ksforge choose oauth2"), None);
    }

    #[test]
    fn ignores_comments_with_no_recognized_command() {
        assert_eq!(parse_follow_up_command("looks good to me"), None);
    }
}
