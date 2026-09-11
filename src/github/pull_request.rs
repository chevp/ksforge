use std::path::{Path, PathBuf};

use crate::domain::{Execution, ExecutionResult, ExecutionStatus, Result};

use super::process::{run_gh, run_git};
use super::repo::group_by_repo;

/// The only place in ksforge that knows about branches, commits, or pull
/// requests (§HFNMflB is mandatory: Git/GitHub are infrastructure, never
/// domain). Shells out to `git` and the `gh` CLI rather than reimplementing
/// either.
pub struct PullRequestOutcome {
    /// Repo root (absolute) this branch/PR belongs to — a single execution
    /// can touch several repos in a multi-repo workspace (docs/03-architecture.md,
    /// "Workspace shapes"), so one execution may produce several outcomes.
    pub repo: PathBuf,
    pub branch: String,
    pub url: Option<String>,
}

/// Result of a multi-repo git plumbing step: one [`PullRequestOutcome`] per
/// repo actually touched, plus any changed files that could not be
/// attributed to a repo at all (`unversioned` — no `.git` found between the
/// file and `workspace_root`, e.g. a plain non-git workspace folder). An
/// empty, all-`unversioned`-free batch is the same "nothing to do" signal
/// the old `Option<PullRequestOutcome>` `None` case was.
#[derive(Default)]
pub struct PullRequestBatch {
    pub outcomes: Vec<PullRequestOutcome>,
    pub unversioned: Vec<PathBuf>,
}

/// Branch, commit the execution's changed files, push, and open a PR via
/// `gh` — once per repo the execution actually touched (see
/// `super::repo::group_by_repo`). Returns an empty batch (not an error) when
/// there is nothing to open a PR for: the execution did not complete
/// successfully, or it made no file changes (e.g. `review`/`explain`).
pub async fn create_from_execution(
    workspace_root: &Path,
    execution: &Execution,
    base_branch: &str,
) -> Result<PullRequestBatch> {
    if execution.status != ExecutionStatus::Completed {
        return Ok(PullRequestBatch::default());
    }
    let Some(result) = &execution.result else {
        return Ok(PullRequestBatch::default());
    };
    if result.changed_files.is_empty() {
        return Ok(PullRequestBatch::default());
    }

    let (groups, unversioned) = group_by_repo(workspace_root, &result.changed_files);
    let title = pull_request_title(&execution.capability, result);
    let body = pr_body(execution);

    let mut outcomes = Vec::new();
    for (repo, files) in groups {
        ensure_git_identity(&repo).await?;

        let branch = format!("ksforge/{}", short_id(&execution.id.0));
        run_git(&repo, &["checkout", "-b", &branch]).await?;

        let mut add_args: Vec<String> = vec!["add".into(), "--".into()];
        add_args.extend(files.iter().map(|p| p.display().to_string()));
        run_git(
            &repo,
            &add_args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;

        run_git(&repo, &["commit", "-m", &title, "-m", &body]).await?;
        run_git(&repo, &["push", "-u", "origin", &branch]).await?;

        let url = run_gh(
            &repo,
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

        outcomes.push(PullRequestOutcome {
            repo,
            branch,
            url: url.trim().lines().last().map(str::to_string),
        });
    }

    Ok(PullRequestBatch {
        outcomes,
        unversioned,
    })
}

/// One repo's outcome from [`push_follow_up`] or [`commit_to_new_branch`] —
/// no URL (neither ever calls `gh`), see [`PullRequestOutcome`] for the
/// `create_from_execution` shape which does.
pub struct RepoCommitOutcome {
    pub repo: PathBuf,
    /// The commit title (`push_follow_up`) or the new branch name
    /// (`commit_to_new_branch`) — always non-empty for an actual entry.
    pub label: String,
}

/// See [`PullRequestBatch`]; the same "one entry per touched repo, plus
/// leftover unversioned files" shape for functions that don't produce a PR.
#[derive(Default)]
pub struct RepoCommitBatch {
    pub outcomes: Vec<RepoCommitOutcome>,
    pub unversioned: Vec<PathBuf>,
}

/// Commits the execution's changed files directly onto the *currently
/// checked-out* branch and pushes — for a PR follow-up run (`ksforge
/// handle-comment` classifies a `/ksforge implement|fix <change-request>` comment,
/// then a normal `ksforge implement`/`ksforge fix --push-to-branch` call
/// runs it against a checkout of the PR's own head branch), which must
/// update that existing PR rather than open a new one the way
/// `create_from_execution` does — once per repo the execution touched.
/// Returns an empty batch for the same "nothing to do" cases as
/// `create_from_execution`; the caller (`ksforge post-report --pr <number>`)
/// posts/updates the PR comment separately.
pub async fn push_follow_up(
    workspace_root: &Path,
    execution: &Execution,
) -> Result<RepoCommitBatch> {
    if execution.status != ExecutionStatus::Completed {
        return Ok(RepoCommitBatch::default());
    }
    let Some(result) = &execution.result else {
        return Ok(RepoCommitBatch::default());
    };
    if result.changed_files.is_empty() {
        return Ok(RepoCommitBatch::default());
    }

    let (groups, unversioned) = group_by_repo(workspace_root, &result.changed_files);
    let title = pull_request_title(&execution.capability, result);

    let mut outcomes = Vec::new();
    for (repo, files) in groups {
        ensure_git_identity(&repo).await?;

        let mut add_args: Vec<String> = vec!["add".into(), "--".into()];
        add_args.extend(files.iter().map(|p| p.display().to_string()));
        run_git(
            &repo,
            &add_args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;

        run_git(&repo, &["commit", "-m", &title]).await?;
        run_git(&repo, &["push"]).await?;

        outcomes.push(RepoCommitOutcome {
            repo,
            label: title.clone(),
        });
    }

    Ok(RepoCommitBatch {
        outcomes,
        unversioned,
    })
}

/// Branches and commits the execution's changed files locally — the same
/// branch/add/commit shape as `create_from_execution`'s first half, but
/// never pushes or calls `gh` (interactive chat's local merge-back, section
/// "Local branch, commit — automatic" in docs/12-interactive-chat.md) —
/// once per repo the execution touched. Returns an empty batch for the same
/// "nothing to do" cases as `create_from_execution`.
pub async fn commit_to_new_branch(
    workspace_root: &Path,
    execution: &Execution,
) -> Result<RepoCommitBatch> {
    if execution.status != ExecutionStatus::Completed {
        return Ok(RepoCommitBatch::default());
    }
    let Some(result) = &execution.result else {
        return Ok(RepoCommitBatch::default());
    };
    if result.changed_files.is_empty() {
        return Ok(RepoCommitBatch::default());
    }

    let (groups, unversioned) = group_by_repo(workspace_root, &result.changed_files);
    let title = pull_request_title(&execution.capability, result);
    let body = pr_body(execution);

    let mut outcomes = Vec::new();
    for (repo, files) in groups {
        ensure_git_identity(&repo).await?;

        let branch = format!("ksforge/{}", short_id(&execution.id.0));
        run_git(&repo, &["checkout", "-b", &branch]).await?;

        let mut add_args: Vec<String> = vec!["add".into(), "--".into()];
        add_args.extend(files.iter().map(|p| p.display().to_string()));
        run_git(
            &repo,
            &add_args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .await?;

        run_git(&repo, &["commit", "-m", &title, "-m", &body]).await?;

        outcomes.push(RepoCommitOutcome {
            repo,
            label: branch,
        });
    }

    Ok(RepoCommitBatch {
        outcomes,
        unversioned,
    })
}

/// Merges `branch` into `base_branch` locally and deletes `branch`, inside
/// `repo` — the confirmed half of chat's merge-back (section "Merge back —
/// confirmed" in docs/12-interactive-chat.md). Never touches a remote, never
/// calls `gh`. `repo` is a [`RepoCommitOutcome::repo`] from
/// `commit_to_new_branch`, not necessarily the workspace root.
pub async fn merge_branch_into(repo: &Path, branch: &str, base_branch: &str) -> Result<()> {
    run_git(repo, &["checkout", base_branch]).await?;
    let message = format!("Merge branch '{branch}' into {base_branch}");
    run_git(repo, &["merge", "--no-ff", branch, "-m", &message]).await?;
    run_git(repo, &["branch", "-d", branch]).await?;
    Ok(())
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
        assert!(result.outcomes.is_empty());
        assert!(result.unversioned.is_empty());
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
        assert!(result.outcomes.is_empty());
    }

    #[tokio::test]
    async fn commit_to_new_branch_is_a_noop_before_the_execution_completes() {
        let execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        let result = commit_to_new_branch(Path::new("/does/not/exist"), &execution)
            .await
            .unwrap();
        assert!(result.outcomes.is_empty());
    }

    #[tokio::test]
    async fn commit_to_new_branch_is_a_noop_with_no_changed_files() {
        let mut execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "review",
        );
        let mut result = result_with_title(Some("Nothing to change"));
        result.changed_files = Vec::new();
        execution.complete(result);

        let result = commit_to_new_branch(Path::new("/does/not/exist"), &execution)
            .await
            .unwrap();
        assert!(result.outcomes.is_empty());
    }

    /// A changed file with no `.git` anywhere between it and `workspace_root`
    /// (a plain non-git workspace folder) is reported as `unversioned`
    /// rather than failing the whole batch.
    #[tokio::test]
    async fn changed_files_outside_any_repo_are_reported_not_erred() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "hi").unwrap();

        let mut execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        let mut result = result_with_title(Some("Add notes"));
        result.changed_files = vec!["notes.md".into()];
        execution.complete(result);

        let batch = commit_to_new_branch(dir.path(), &execution).await.unwrap();
        assert!(batch.outcomes.is_empty());
        assert_eq!(batch.unversioned, vec![PathBuf::from("notes.md")]);
    }

    #[tokio::test]
    async fn commit_to_new_branch_and_merge_branch_into_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-q", "-b", "main"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("README.md"), "hello\n").unwrap();
        run_git(dir.path(), &["add", "README.md"]).await.unwrap();
        ensure_git_identity(dir.path()).await.unwrap();
        run_git(dir.path(), &["commit", "-q", "-m", "initial"])
            .await
            .unwrap();

        std::fs::write(dir.path().join("README.md"), "hello world\n").unwrap();
        let mut execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        let mut result = result_with_title(Some("Update README"));
        result.changed_files = vec!["README.md".into()];
        execution.complete(result);

        let batch = commit_to_new_branch(dir.path(), &execution).await.unwrap();
        let outcome = batch
            .outcomes
            .into_iter()
            .next()
            .expect("there is a change to commit");
        assert!(outcome.label.starts_with("ksforge/"));

        merge_branch_into(&outcome.repo, &outcome.label, "main")
            .await
            .unwrap();

        let branches = run_git(dir.path(), &["branch", "--list", &outcome.label])
            .await
            .unwrap();
        assert!(
            branches.trim().is_empty(),
            "merged branch should be deleted, got: {branches:?}"
        );
        // Not `assert_eq!` against a literal "\n" ending: on Windows, a
        // repo/global `core.autocrlf` can rewrite line endings on checkout,
        // which is unrelated to what this test actually verifies.
        let content = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
        assert!(content.trim_end() == "hello world");
    }

    /// Two independent sibling repos under a non-git workspace root (the
    /// chevp multi-repo workspace shape) each get their own branch/commit.
    #[tokio::test]
    async fn commit_to_new_branch_handles_sibling_repos_independently() {
        let root = tempfile::tempdir().unwrap();
        for repo in ["one", "two"] {
            let path = root.path().join(repo);
            std::fs::create_dir_all(&path).unwrap();
            run_git(&path, &["init", "-q", "-b", "main"]).await.unwrap();
            std::fs::write(path.join("f.txt"), "before\n").unwrap();
            run_git(&path, &["add", "f.txt"]).await.unwrap();
            ensure_git_identity(&path).await.unwrap();
            run_git(&path, &["commit", "-q", "-m", "initial"])
                .await
                .unwrap();
            std::fs::write(path.join("f.txt"), format!("{repo} changed\n")).unwrap();
        }

        let mut execution = Execution::start(
            ChangeRequest::from_text("As a user, I want X.").unwrap(),
            "implement",
        );
        let mut result = result_with_title(Some("Update both repos"));
        result.changed_files = vec!["one/f.txt".into(), "two/f.txt".into()];
        execution.complete(result);

        let batch = commit_to_new_branch(root.path(), &execution).await.unwrap();
        assert_eq!(batch.outcomes.len(), 2);
        assert!(batch.unversioned.is_empty());
        let repos: Vec<_> = batch.outcomes.iter().map(|o| o.repo.clone()).collect();
        assert!(repos.contains(&root.path().join("one")));
        assert!(repos.contains(&root.path().join("two")));
    }
}
