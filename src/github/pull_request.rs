use std::path::Path;

use crate::domain::{Execution, ExecutionResult, ExecutionStatus, Result};

use super::process::{run_gh, run_git};

/// The only place in ksforge that knows about branches, commits, or pull
/// requests (section 20 is mandatory: Git/GitHub are infrastructure, never
/// domain). Shells out to `git` and the `gh` CLI rather than reimplementing
/// either.
pub struct PullRequestOutcome {
    pub branch: String,
    pub url: Option<String>,
}

/// Branch, commit the execution's changed files, push, and open a PR via
/// `gh`. Returns `Ok(None)` (not an error) when there is nothing to open a
/// PR for: the execution did not complete successfully, or it made no file
/// changes (e.g. `review`/`explain`).
pub async fn create_from_execution(
    workspace_root: &Path,
    execution: &Execution,
    base_branch: &str,
) -> Result<Option<PullRequestOutcome>> {
    if execution.status != ExecutionStatus::Completed {
        return Ok(None);
    }
    let Some(result) = &execution.result else {
        return Ok(None);
    };
    if result.changed_files.is_empty() {
        return Ok(None);
    }

    let branch = format!("ksforge/{}", short_id(&execution.id.0));
    run_git(workspace_root, &["checkout", "-b", &branch]).await?;

    let mut add_args: Vec<String> = vec!["add".into(), "--".into()];
    add_args.extend(result.changed_files.iter().map(|p| p.display().to_string()));
    run_git(
        workspace_root,
        &add_args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;

    let title = pull_request_title(&execution.capability, result);
    let body = pr_body(execution);
    run_git(workspace_root, &["commit", "-m", &title, "-m", &body]).await?;

    run_git(workspace_root, &["push", "-u", "origin", &branch]).await?;

    let url = run_gh(
        workspace_root,
        &[
            "pr",
            "create",
            "--title",
            &title,
            "--body",
            &body,
            "--base",
            base_branch,
            "--head",
            &branch,
        ],
    )
    .await?;

    Ok(Some(PullRequestOutcome {
        branch,
        url: url.trim().lines().last().map(str::to_string),
    }))
}

/// Prefers the agent's own headline (`AgentOutcome::title`) — a clean,
/// standalone commit-subject-style line — over a truncated `summary`
/// sentence fragment; the latter only exists as a fallback for an older or
/// non-compliant agent response (section: never fully trust model-reported
/// facts, but degrade gracefully rather than fail).
fn pull_request_title(capability: &str, result: &ExecutionResult) -> String {
    let headline = result
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| truncate(&result.summary, 60));
    format!("[{capability}] {headline}")
}

fn pr_body(execution: &Execution) -> String {
    let validation = execution
        .result
        .as_ref()
        .map(|r| {
            if r.validation.commands.is_empty() {
                "not configured"
            } else if r.validation.passed {
                "passed"
            } else {
                "failed"
            }
        })
        .unwrap_or("not configured");
    // One compact metadata line instead of three stacked "Key: value"
    // lines — the story itself is the part worth reading, this footer is
    // just provenance.
    format!(
        "## Story\n\n{}\n\n---\n`{}` · `{}` · validation: {validation}",
        execution.user_story.text.trim(),
        execution.capability,
        execution.id,
    )
}

fn short_id(id: &str) -> &str {
    id.strip_prefix("ksf_")
        .unwrap_or(id)
        .get(..12)
        .unwrap_or(id)
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.lines().next().unwrap_or(s);
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{UserStory, ValidationOutcome};

    fn result_with_title(title: Option<&str>) -> ExecutionResult {
        ExecutionResult {
            success: true,
            title: title.map(String::from),
            summary: "Added a '## User Stories einreichen' section to README, describing how to submit a story via the workflow.".into(),
            changed_files: vec!["README.md".into()],
            validation: ValidationOutcome::default(),
            completed: Vec::new(),
            open_items: Vec::new(),
            recommendation: None,
        }
    }

    #[test]
    fn prefers_the_agents_own_title() {
        let result = result_with_title(Some("Document user story submission in README"));
        assert_eq!(
            pull_request_title("implement", &result),
            "[implement] Document user story submission in README"
        );
    }

    #[test]
    fn falls_back_to_a_truncated_summary_without_a_title() {
        let result = result_with_title(None);
        assert_eq!(
            pull_request_title("implement", &result),
            "[implement] Added a '## User Stories einreichen' section to README, desc..."
        );
    }

    #[test]
    fn falls_back_when_the_title_is_blank() {
        let result = result_with_title(Some("   "));
        assert!(pull_request_title("implement", &result).starts_with("[implement] Added a"));
    }

    #[test]
    fn body_has_one_compact_metadata_line_not_three() {
        let mut execution = Execution::start(
            UserStory::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        execution.complete(result_with_title(Some("Document user story submission")));

        let body = pr_body(&execution);
        assert!(body.starts_with("## Story\n\nAs a user, I want X.\n\n---\n"));
        assert_eq!(body.lines().last().unwrap().matches('·').count(), 2);
    }
}
