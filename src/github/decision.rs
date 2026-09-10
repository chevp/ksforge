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
}
