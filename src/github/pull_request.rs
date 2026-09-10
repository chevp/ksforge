use std::path::Path;

use crate::domain::{Execution, ExecutionStatus, Result};

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

    let title = format!(
        "[{}] {}",
        execution.capability,
        truncate(&result.summary, 60)
    );
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

fn pr_body(execution: &Execution) -> String {
    let result = execution.result.as_ref();
    format!(
        "Story:\n\n{}\n\n---\nCapability: {}\nExecution: {}\n{}",
        execution.user_story.text,
        execution.capability,
        execution.id,
        result
            .map(|r| format!(
                "Validation: {}",
                if r.validation.commands.is_empty() {
                    "not configured".to_string()
                } else if r.validation.passed {
                    "passed".to_string()
                } else {
                    "failed".to_string()
                }
            ))
            .unwrap_or_default(),
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
