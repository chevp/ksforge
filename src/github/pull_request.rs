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

    ensure_git_identity(workspace_root).await?;

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

/// Commits the execution's changed files directly onto the *currently
/// checked-out* branch and pushes — for a PR follow-up run (`ksforge
/// handle-comment` classifies a `/ksforge implement|fix <change-request>` comment,
/// then a normal `ksforge implement`/`ksforge fix --push-to-branch` call
/// runs it against a checkout of the PR's own head branch), which must
/// update that existing PR rather than open a new one the way
/// `create_from_execution` does. Returns `Ok(None)` for the same
/// "nothing to do" cases as `create_from_execution`; the caller (`ksforge
/// post-report --pr <number>`) posts/updates the PR comment separately.
pub async fn push_follow_up(
    workspace_root: &Path,
    execution: &Execution,
) -> Result<Option<String>> {
    if execution.status != ExecutionStatus::Completed {
        return Ok(None);
    }
    let Some(result) = &execution.result else {
        return Ok(None);
    };
    if result.changed_files.is_empty() {
        return Ok(None);
    }

    ensure_git_identity(workspace_root).await?;

    let mut add_args: Vec<String> = vec!["add".into(), "--".into()];
    add_args.extend(result.changed_files.iter().map(|p| p.display().to_string()));
    run_git(
        workspace_root,
        &add_args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;

    let title = pull_request_title(&execution.capability, result);
    run_git(workspace_root, &["commit", "-m", &title]).await?;
    run_git(workspace_root, &["push"]).await?;

    Ok(Some(title))
}

/// `git commit` needs an identity to attribute the commit to, and a fresh
/// CI runner has none configured at any level (local/global/system) by
/// default — confirmed against a real run: "Author identity unknown ...
/// fatal: empty ident name". Every consumer workflow remembering its own
/// "Configure git identity" step is exactly the kind of boilerplate
/// ksforge should own once here instead. Never overwrites an identity
/// that's already configured (checked, not just set unconditionally) —
/// scoped `--local` so the fallback never leaks into the user's own global
/// git config.
async fn ensure_git_identity(workspace_root: &Path) -> Result<()> {
    if run_git(workspace_root, &["config", "user.email"])
        .await
        .is_err()
    {
        run_git(
            workspace_root,
            &[
                "config",
                "--local",
                "user.email",
                "41898282+github-actions[bot]@users.noreply.github.com",
            ],
        )
        .await?;
    }
    if run_git(workspace_root, &["config", "user.name"])
        .await
        .is_err()
    {
        run_git(
            workspace_root,
            &["config", "--local", "user.name", "github-actions[bot]"],
        )
        .await?;
    }
    Ok(())
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
    // lines — the change request itself is the part worth reading, this footer is
    // just provenance.
    format!(
        "## Change Request\n\n{}\n\n---\n`{}` · `{}` · validation: {validation}",
        execution.change_request.text.trim(),
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
mod ensure_git_identity_tests {
    use super::*;

    #[tokio::test]
    async fn leaves_the_repo_with_a_usable_identity() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-q"]).await.unwrap();

        ensure_git_identity(dir.path()).await.unwrap();

        assert!(run_git(dir.path(), &["config", "user.email"]).await.is_ok());
        assert!(run_git(dir.path(), &["config", "user.name"]).await.is_ok());
    }

    #[tokio::test]
    async fn is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-q"]).await.unwrap();

        ensure_git_identity(dir.path()).await.unwrap();
        ensure_git_identity(dir.path()).await.unwrap();

        assert!(run_git(dir.path(), &["config", "user.email"]).await.is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChangeRequest, ValidationOutcome};

    fn result_with_title(title: Option<&str>) -> ExecutionResult {
        ExecutionResult {
            success: true,
            title: title.map(String::from),
            summary: "Added a '## Change Requests einreichen' section to README, describing how to submit a change request via the workflow.".into(),
            changed_files: vec!["README.md".into()],
            validation: ValidationOutcome::default(),
            completed: Vec::new(),
            open_items: Vec::new(),
            recommendation: None,
        }
    }

    #[test]
    fn prefers_the_agents_own_title() {
        let result = result_with_title(Some("Document change request submission in README"));
        assert_eq!(
            pull_request_title("implement", &result),
            "[implement] Document change request submission in README"
        );
    }

    #[test]
    fn falls_back_to_a_truncated_summary_without_a_title() {
        let result = result_with_title(None);
        assert_eq!(
            pull_request_title("implement", &result),
            "[implement] Added a '## Change Requests einreichen' section to README, d..."
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
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        execution.complete(result_with_title(Some(
            "Document change request submission",
        )));

        let body = pr_body(&execution);
        assert!(body.starts_with("## Change Request\n\nAs a user, I want X.\n\n---\n"));
        assert_eq!(body.lines().last().unwrap().matches('·').count(), 2);
    }

    #[tokio::test]
    async fn push_follow_up_is_a_noop_before_the_execution_completes() {
        let execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "fix",
        );
        // Still `Running` — never reaches a git command, so no real repo
        // is needed at `workspace_root` for this case.
        let result = push_follow_up(Path::new("/does/not/exist"), &execution)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn push_follow_up_is_a_noop_with_no_changed_files() {
        let mut execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "fix",
        );
        let mut result = result_with_title(Some("Nothing to change"));
        result.changed_files = Vec::new();
        execution.complete(result);

        let result = push_follow_up(Path::new("/does/not/exist"), &execution)
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
