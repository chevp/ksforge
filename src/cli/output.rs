use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::agent::CoordinationDecision;
use crate::domain::{CapabilityRegistry, Execution, ExecutionStatus, KsforgeError, Result};
use crate::github;

use super::args::OutputFormat;

/// Print a `ksforge coordinate` result. Same `Json`/`Text` contract as
/// [`print_execution`] — not an `Execution` (this command never starts
/// one), so it gets its own small printer instead of reusing that one.
pub fn print_coordination_decision(decision: &CoordinationDecision, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(decision).unwrap());
        }
        OutputFormat::Text => {
            println!();
            println!("Classification: {:?}", decision.classification);
            println!("Proceed: {}", if decision.proceed { "YES" } else { "NO" });
            print_bullets("Affected scope", &decision.affected_scope);
            print_bullets("Related executions", &decision.related_executions);
            print_bullets("Dependencies", &decision.dependencies);
            print_bullets("Conflicts", &decision.conflicts);
            print_bullets("Recommendations", &decision.recommendations);
            println!();
        }
    }
}

fn print_bullets(label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    println!();
    println!("{label}:");
    for item in items {
        println!("  - {item}");
    }
}

/// Print a finished or paused execution. In `Json` mode stdout carries
/// exactly one JSON object and nothing else (§A0SRRlR); in `Text` mode
/// it's a short human transcript. Diagnostics never go to stdout in either
/// mode — see [`eprint_notice`].
pub fn print_execution(execution: &Execution, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(&as_json(execution)).unwrap());
        }
        OutputFormat::Text => print_human(execution),
    }
}

fn as_json(execution: &Execution) -> serde_json::Value {
    json!({
        "execution_id": execution.id.0,
        "capability": execution.capability,
        "status": execution.status.to_string(),
        "pending_question": execution.pending_question,
        "gates": execution.gates,
        "result": execution.result,
        "artifacts": execution.artifacts,
    })
}

fn print_human(execution: &Execution) {
    println!();
    println!("ksforge");
    println!("{}", "─".repeat(36));
    println!();
    println!("Capability:");
    println!("  {}", execution.capability);
    println!();
    println!("Change request:");
    println!(
        "  {}",
        execution.change_request.text.lines().next().unwrap_or("")
    );
    println!();
    println!("Execution: {}", execution.id);

    match execution.status {
        ExecutionStatus::WaitingForHuman => {
            if let Some(q) = &execution.pending_question {
                println!();
                println!("Status:");
                println!("  Waiting for human input");
                println!();
                println!("Question:");
                println!("  {}", q.question);
                println!();
                println!("Options:");
                for (i, o) in q.options.iter().enumerate() {
                    println!("  {i}) {} - {}", o.id, o.label);
                }
                if let Some(rec) = &q.recommended_option {
                    println!();
                    println!("Recommended:");
                    println!("  {rec}");
                    if !q.context.is_empty() {
                        println!("  {}", q.context);
                    }
                }
                println!();
                println!(
                    "Resume:\n  ksforge resume {} --decision <number-or-option-id>",
                    execution.id
                );
            }
        }
        ExecutionStatus::Completed => {
            if let Some(result) = &execution.result {
                println!();
                if result.changed_files.is_empty() {
                    println!("Changed: none");
                } else {
                    println!("Changed:");
                    for f in &result.changed_files {
                        println!("  {}", f.display());
                    }
                }
                if !result.validation.commands.is_empty() {
                    println!();
                    println!("Validation:");
                    for c in &result.validation.commands {
                        println!("  {} {}", if c.passed { "✓" } else { "✗" }, c.command);
                    }
                }
                if let Some(review) = &result.validation.agent_review {
                    println!();
                    println!("Validation (agent): {}", review.summary);
                    for c in &review.completed {
                        println!("  ✓ {c}");
                    }
                    if !review.open_items.is_empty() {
                        println!("  Further findings:");
                        for item in &review.open_items {
                            println!("    - {item}");
                        }
                    }
                    if let Some(rec) = &review.recommendation {
                        println!("  Recommendation: {rec}");
                    }
                }
                if !result.open_items.is_empty() {
                    println!();
                    println!("Open:");
                    for item in &result.open_items {
                        println!("  {item}");
                    }
                }
                if let Some(rec) = &result.recommendation {
                    println!();
                    println!("Recommendation:");
                    println!("  {rec}");
                }
                println!();
                println!("Result:");
                println!("  {}", result.summary);
            }
        }
        ExecutionStatus::Failed => {
            println!();
            println!("Result:");
            println!(
                "  Failed: {}",
                execution
                    .result
                    .as_ref()
                    .map(|r| r.summary.as_str())
                    .unwrap_or("unknown error")
            );
        }
        ExecutionStatus::Running | ExecutionStatus::Cancelled => {
            println!();
            println!("Status: {}", execution.status);
        }
    }
    println!();
}

/// One repo in a [`WorkspaceOverview`] — every `Execution` currently on
/// disk under it (any status; `WorkspaceOverview`'s printers decide what
/// counts as "active"). No git state here on purpose: this view answers
/// "what has ksforge done/is doing", not "what does `git status` say".
pub struct RepoOverview {
    pub path: PathBuf,
    pub executions: Vec<Execution>,
}

/// `ksforge status` with no execution id: every git repo `github::discover_repos`
/// found under `--workspace`, for the human orchestrating dozens or
/// hundreds of them at once (docs/12-interactive-chat.md's multi-repo
/// workspace shape) to see at a glance what is already running/waiting.
pub struct WorkspaceOverview {
    pub workspace_root: PathBuf,
    pub repos: Vec<RepoOverview>,
    /// See `github::discover_repos`'s second return value — the scan
    /// stopped early rather than running unbounded.
    pub scan_capped: bool,
}

/// Same `Json`/`Text` contract as [`print_execution`], plain stdout in
/// both modes (no color — see `crate::color`'s doc comment on why data
/// output, as opposed to progress notices, never carries ANSI).
pub fn print_workspace_overview(overview: &WorkspaceOverview, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string(&workspace_overview_json(overview)).unwrap()
            );
        }
        OutputFormat::Text => print_workspace_overview_human(overview),
    }
}

fn workspace_overview_json(overview: &WorkspaceOverview) -> serde_json::Value {
    json!({
        "workspace_root": overview.workspace_root.display().to_string(),
        "scan_capped": overview.scan_capped,
        "repos": overview.repos.iter().map(|r| json!({
            "path": r.path.display().to_string(),
            "executions": r.executions.iter().map(execution_summary_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn execution_summary_json(execution: &Execution) -> serde_json::Value {
    json!({
        "id": execution.id.0,
        "capability": execution.capability,
        "status": execution.status.to_string(),
        "summary": execution.change_request.text.lines().next().unwrap_or(""),
        "updated_at": execution.updated_at.to_rfc3339(),
    })
}

fn is_active(execution: &Execution) -> bool {
    matches!(
        execution.status,
        ExecutionStatus::Running | ExecutionStatus::WaitingForHuman
    )
}

fn print_workspace_overview_human(overview: &WorkspaceOverview) {
    let mut all_executions: Vec<(&RepoOverview, &Execution)> = overview
        .repos
        .iter()
        .flat_map(|r| r.executions.iter().map(move |e| (r, e)))
        .collect();
    all_executions.sort_by_key(|(_, e)| std::cmp::Reverse(e.updated_at));

    let active_count = all_executions.iter().filter(|(_, e)| is_active(e)).count();
    let active_repo_count = overview
        .repos
        .iter()
        .filter(|r| r.executions.iter().any(is_active))
        .count();
    let waiting_count = all_executions
        .iter()
        .filter(|(_, e)| e.status == ExecutionStatus::WaitingForHuman)
        .count();

    println!();
    println!("ksforge");
    println!("{}", "─".repeat(36));
    println!();
    println!("Workspace:");
    println!("  path      {}", overview.workspace_root.display());
    println!("  repos     {}", overview.repos.len());
    if active_count > 0 {
        println!(
            "  active    {} execution{} across {} repo{}{}",
            active_count,
            if active_count == 1 { "" } else { "s" },
            active_repo_count,
            if active_repo_count == 1 { "" } else { "s" },
            if waiting_count > 0 {
                format!(" ({waiting_count} waiting for human)")
            } else {
                String::new()
            }
        );
    }
    if overview.scan_capped {
        println!("  note      repo scan stopped early (workspace is very large)");
    }

    if overview.repos.is_empty() {
        println!();
        println!("(no git repositories found under this workspace)");
        println!();
        return;
    }

    println!();
    println!("Executions:");
    if all_executions.is_empty() {
        println!("  (none found)");
    } else {
        let (active, recent): (Vec<_>, Vec<_>) =
            all_executions.into_iter().partition(|(_, e)| is_active(e));
        for (repo, execution) in active.iter().chain(recent.iter().take(10)) {
            let relative = repo
                .path
                .strip_prefix(&overview.workspace_root)
                .unwrap_or(&repo.path);
            let label = if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.display().to_string()
            };
            let summary = execution.change_request.text.lines().next().unwrap_or("");
            println!(
                "  {}  {:<24}  {:<10}  {:<18}  {}",
                execution.id, label, execution.capability, execution.status, summary
            );
        }
    }

    println!();
    let capabilities = CapabilityRegistry::with_defaults();
    println!(
        "Capabilities: {}",
        capabilities
            .list()
            .iter()
            .map(|c| c.id())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("  ksforge status <id>                       full detail for one execution");
    println!("  ksforge resume <id> --decision <option>   unblock a waiting execution");
    println!(
        "  ksforge coordinate --change \"...\"         check a new request against active work"
    );
    println!();
}

/// Progress/diagnostic text. Always stderr, in every output mode, so
/// `--format json` stdout is never polluted (§A0SRRlR) — colored the same
/// way (see `crate::color`) for exactly that reason: it's never data.
pub fn eprint_notice(message: &str) {
    eprintln!(
        "{}",
        crate::color::paint(message, crate::color::Color::Cyan, false)
    );
}

/// Whether ksforge should ask for a `waiting_for_human` decision right in
/// this console session instead of exiting `6` for an external mechanism
/// (a GitHub Actions PR-comment round trip — `post-report`/`handle-comment`,
/// see docs/07-github-actions.md) to resume it later. Both streams must be
/// real terminals: reading a decision needs stdin, showing the prompt
/// sensibly needs stdout. A CI runner's stdio is never a tty, so a GitHub
/// Actions run is already excluded here without a separate env-var check.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Echoes a [`github::PullRequestBatch`] (`github::create_from_execution`)
/// to stderr: one colored line per repo actually touched, then a summary
/// when more than one repo was involved — loosely modeled on `luma ship`'s
/// per-repo/summary sections (`tools/luma/src/commands/ship-workspace.ts`),
/// scaled down to ksforge's existing [`eprint_notice`] diagnostic channel
/// (never stdout — see `crate::color`'s doc comment on why plumbing/data
/// stays uncolored there).
pub fn print_pull_request_batch(batch: &github::PullRequestBatch, workspace_root: &Path) {
    for outcome in &batch.outcomes {
        eprint_notice(&format!(
            "  {} — branch '{}'{}",
            relative(&outcome.repo, workspace_root),
            outcome.branch,
            outcome
                .url
                .as_deref()
                .map(|u| format!(" -> {u}"))
                .unwrap_or_default(),
        ));
    }
    print_unversioned_warning(&batch.unversioned, workspace_root);
    if batch.outcomes.len() > 1 {
        eprint_notice(&format!(
            "{} pull requests opened across {} repos",
            batch.outcomes.iter().filter(|o| o.url.is_some()).count(),
            batch.outcomes.len()
        ));
    }
}

/// Same shape as [`print_pull_request_batch`] for a
/// [`github::RepoCommitBatch`] (`github::push_follow_up`/
/// `github::commit_to_new_branch`, neither of which produces a PR URL).
pub fn print_repo_commit_batch(batch: &github::RepoCommitBatch, workspace_root: &Path) {
    for outcome in &batch.outcomes {
        eprint_notice(&format!(
            "  {} — {}",
            relative(&outcome.repo, workspace_root),
            outcome.label,
        ));
    }
    print_unversioned_warning(&batch.unversioned, workspace_root);
}

/// Not an error: the execution itself already completed successfully, only
/// the auto-commit step is structurally impossible for these files (no
/// `.git` anywhere between them and `workspace_root` — a plain non-git
/// workspace folder, see `github::repo::group_by_repo`).
fn print_unversioned_warning(unversioned: &[PathBuf], workspace_root: &Path) {
    if unversioned.is_empty() {
        return;
    }
    eprint_notice(&format!(
        "Changed but not committed — no git repo found under {}: {}",
        workspace_root.display(),
        unversioned
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    ));
}

fn relative(path: &Path, workspace_root: &Path) -> String {
    path.strip_prefix(workspace_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Reads one line from stdin, printing `label` first with no trailing
/// newline. `Ok(None)` means EOF (Ctrl-D on a POSIX shell, Ctrl-Z Enter on
/// `cmd.exe`/PowerShell).
pub fn prompt_line(label: &str) -> Result<Option<String>> {
    print!("{label}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    let bytes_read = std::io::stdin()
        .read_line(&mut line)
        .map_err(KsforgeError::Io)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}
