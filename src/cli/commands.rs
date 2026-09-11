use std::sync::Arc;

use crate::agent::{AgentExecutor, ClaudeCodeExecutor, CodexExecutor};
use crate::domain::{
    CapabilityRegistry, ChangeRequest, Execution, ExecutionContext, ExecutionEvent, ExecutionId,
    ExecutionStatus, ImplementationRequest, KsforgeError, Result, ValidationPolicy,
};
use crate::{github, workspace};

use super::args::{
    CancelArgs, ChangeRequestArgs, CommonArgs, CoordinateArgs, Engine, HandleCommentArgs,
    OutputFormat, PostReportArgs, ResumeArgs, StatusArgs,
};
use super::output;

/// Spawns the CLI selected by `--engine`, resolved from an explicit path or
/// PATH (§fMSksqK).
pub(crate) fn build_executor(
    engine: Engine,
    claude_path: Option<&std::path::Path>,
    codex_path: Option<&std::path::Path>,
) -> Result<Arc<dyn AgentExecutor>> {
    match engine {
        Engine::Claude => {
            let executor = ClaudeCodeExecutor::discover(claude_path)
                .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;
            Ok(Arc::new(executor))
        }
        Engine::Codex => {
            let executor = CodexExecutor::discover(codex_path)
                .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;
            Ok(Arc::new(executor))
        }
    }
}

/// `--model` defaults to Claude Code's "sonnet" alias only for `--engine
/// claude` (see `CommonArgs::model`'s doc comment) — passing that alias to
/// Codex would be meaningless, so `--engine codex` leaves it unset instead,
/// deferring to Codex's own configured default.
pub(crate) fn resolve_model(engine: Engine, model: Option<String>) -> Option<String> {
    match engine {
        Engine::Claude => Some(model.unwrap_or_else(|| "sonnet".to_string())),
        Engine::Codex => model,
    }
}

/// Report how a new change request relates to the workspace's other active
/// executions, without implementing anything (spec:
/// `prompts/coordinator/system-prompt.md`). Always exits `0` on a
/// successful analysis, whatever `proceed`/`classification` say — this
/// command only reports, it never itself blocks or starts anything; a
/// caller that wants to gate on the recommendation reads `--format json`.
/// Takes its own `CoordinateArgs` rather than `ChangeRequestArgs` — unlike
/// a capability, this never writes, so `--dry-run`/`--validate`/
/// `--create-pull-request`/`--push-to-branch`/`--mcp-config` would be
/// meaningless flags to expose here.
pub async fn coordinate(args: CoordinateArgs) -> Result<i32> {
    let change_request = load_change_request(args.change_request)?;

    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let executor = build_executor(
        args.engine,
        args.claude_path.as_deref(),
        args.codex_path.as_deref(),
    )?;

    let decision = crate::application::coordinate::coordinate(
        &workspace_root,
        &change_request,
        executor,
        resolve_model(args.engine, args.model),
        args.max_budget_usd,
    )
    .await?;

    output::print_coordination_decision(&decision, args.format);
    Ok(0)
}

pub async fn change_request_capability(
    capability_id: &str,
    args: ChangeRequestArgs,
) -> Result<i32> {
    let change_request = load_change_request(args.change_request)?;
    let common = args.common;

    let workspace_root = workspace::resolve_root(&common.workspace)?;
    let registry = CapabilityRegistry::with_defaults();
    let capability = registry
        .get(capability_id)
        .expect("capability_id is one of the built-in ids wired in main.rs");

    let executor = build_executor(
        common.engine,
        common.claude_path.as_deref(),
        common.codex_path.as_deref(),
    )?;

    let request = ImplementationRequest {
        change_request,
        workspace: workspace_root.clone(),
        capability: capability_id.to_string(),
        constraints: capability.default_constraints(),
        validation: ValidationPolicy {
            commands: common.validate.clone(),
            agent_review: !common.no_validate,
        },
    };
    let context = ExecutionContext {
        executor,
        workspace_root: workspace_root.clone(),
        model: resolve_model(common.engine, common.model.clone()),
        max_budget_usd: common.max_budget_usd,
        dry_run: common.dry_run,
        mcp_config: common.mcp_config.clone(),
    };

    output::eprint_notice(&format!(
        "Running {capability_id}{}...",
        if common.dry_run { " (dry run)" } else { "" }
    ));
    let execution = capability.execute(request, context.clone()).await?;
    output::print_execution(&execution, common.format);
    let execution = prompt_and_resume_while_waiting(
        execution,
        &workspace_root,
        &registry,
        &context,
        common.format,
    )
    .await?;

    maybe_open_pull_request(&workspace_root, &execution, &common).await?;

    Ok(exit_code_for(&execution))
}

pub async fn resume(args: ResumeArgs) -> Result<i32> {
    let common = args.common;
    let workspace_root = workspace::resolve_root(&common.workspace)?;
    let registry = CapabilityRegistry::with_defaults();

    let executor = build_executor(
        common.engine,
        common.claude_path.as_deref(),
        common.codex_path.as_deref(),
    )?;
    let context = ExecutionContext {
        executor,
        workspace_root: workspace_root.clone(),
        model: resolve_model(common.engine, common.model.clone()),
        max_budget_usd: common.max_budget_usd,
        dry_run: common.dry_run,
        mcp_config: common.mcp_config.clone(),
    };

    output::eprint_notice(&format!("Resuming {}...", args.execution_id));
    let execution = crate::application::resume::resume(
        &workspace_root,
        &ExecutionId(args.execution_id),
        args.decision,
        args.decided_by,
        &registry,
        context.clone(),
    )
    .await?;
    output::print_execution(&execution, common.format);
    let execution = prompt_and_resume_while_waiting(
        execution,
        &workspace_root,
        &registry,
        &context,
        common.format,
    )
    .await?;

    maybe_open_pull_request(&workspace_root, &execution, &common).await?;

    Ok(exit_code_for(&execution))
}

/// After a run/resume pauses `waiting_for_human`, keep asking for a
/// decision right in this process and resuming immediately — but only in
/// a real interactive terminal (`output::is_interactive()`); in a GitHub
/// Actions run (or any other non-interactive/piped invocation) this is a
/// no-op, leaving today's behavior in place: exit `6`, and let the
/// PR-comment round trip (`post-report`/`handle-comment`, see
/// docs/07-github-actions.md) resume it later instead. `decided_by` is
/// always `None` here — a `/ksforge choose` comment's own commenter identity
/// only applies to the *first* resume of a run, handled by the caller
/// before this loop ever starts.
async fn prompt_and_resume_while_waiting(
    mut execution: Execution,
    workspace_root: &std::path::Path,
    registry: &CapabilityRegistry,
    context: &ExecutionContext,
    format: OutputFormat,
) -> Result<Execution> {
    while execution.status == ExecutionStatus::WaitingForHuman && output::is_interactive() {
        let Some(decision) = prompt_for_decision(&execution)? else {
            break;
        };
        output::eprint_notice(&format!("Resuming {}...", execution.id));
        execution = crate::application::resume::resume(
            workspace_root,
            &execution.id,
            decision,
            None,
            registry,
            context.clone(),
        )
        .await?;
        output::print_execution(&execution, format);
    }
    Ok(execution)
}

/// Reads decisions from stdin until one matches an offered option — either
/// its id or the number it was printed under (`output::print_human`'s
/// `WaitingForHuman` branch numbers them `0)`, `1)`, ...) — or `Ok(None)`
/// on EOF (the gate stays open, resumable later the normal way: `ksforge
/// resume <id> --decision <number-or-option-id>`). A gate with no options
/// at all cannot be resumed this way (or any way — `application::resume`
/// only accepts a decision matching one of `pending.options[].id`), so this
/// gives up immediately rather than looping forever asking for a choice
/// that does not exist.
fn prompt_for_decision(execution: &Execution) -> Result<Option<String>> {
    let Some(gate) = execution.pending_question.as_ref() else {
        return Ok(None);
    };
    if gate.options.is_empty() {
        eprintln!(
            "ksforge: this execution offers no options to choose from; cancel it instead: \
             ksforge cancel {}",
            execution.id
        );
        return Ok(None);
    }
    loop {
        let Some(line) = output::prompt_line("Decision: ")? else {
            return Ok(None);
        };
        if line.is_empty() {
            continue;
        }
        if let Some(o) = gate
            .options
            .iter()
            .find(|o| o.id == line)
            .or_else(|| line.parse::<usize>().ok().and_then(|i| gate.options.get(i)))
        {
            return Ok(Some(o.id.clone()));
        }
        eprintln!(
            "ksforge: '{line}' is not one of the offered options: {}",
            gate.options
                .iter()
                .enumerate()
                .map(|(i, o)| format!("{i}) {}", o.id))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// With an `execution_id`: unchanged single-execution report. Without one:
/// a workspace-wide overview — every git repo under `--workspace` plus each
/// one's stored executions, for orchestrating dozens or hundreds of
/// concurrent repos at a glance (docs/04-cli-reference.md).
pub async fn status(args: StatusArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let Some(execution_id) = args.execution_id else {
        let overview = build_workspace_overview(&workspace_root)?;
        output::print_workspace_overview(&overview, args.format);
        return Ok(0);
    };
    let store = workspace::ExecutionStore::new(&workspace_root);
    let execution = store.load(&ExecutionId(execution_id))?;
    output::print_execution(&execution, args.format);
    Ok(exit_code_for(&execution))
}

/// Discovers every repo under `workspace_root` (`github::discover_repos`)
/// and pairs each with whatever `Execution`s `workspace::ExecutionStore`
/// finds stored under it. Deliberately git-state-free: this is a view of
/// ksforge's own run history, not of the repos' working-tree status.
fn build_workspace_overview(workspace_root: &std::path::Path) -> Result<output::WorkspaceOverview> {
    let (mut repo_paths, scan_capped) = github::discover_repos(workspace_root);
    repo_paths.sort();

    let repos = repo_paths
        .into_iter()
        .map(|path| {
            let executions = workspace::ExecutionStore::new(&path)
                .list_all()
                .unwrap_or_default();
            output::RepoOverview { path, executions }
        })
        .collect();

    Ok(output::WorkspaceOverview {
        workspace_root: workspace_root.to_path_buf(),
        repos,
        scan_capped,
    })
}

/// Stop an execution that will not be resumed (spec §xjZiT6h/§HFNMflB) — distinct from
/// a failure: this is an operator decision, recorded as such.
pub fn cancel(args: CancelArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let store = workspace::ExecutionStore::new(&workspace_root);
    let mut execution = store.load(&ExecutionId(args.execution_id))?;
    execution.cancel(args.reason);
    store.save(&execution)?;
    output::print_execution(&execution, args.format);
    Ok(exit_code_for(&execution))
}

/// Render and post/update the ksforge status comment for an execution on a
/// pull request (spec §egkEINs/§qHiXmf0/§sTdJsQn). Read-only against the execution itself —
/// only `gh` sees a write.
pub async fn post_report(args: PostReportArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let store = workspace::ExecutionStore::new(&workspace_root);
    let execution = store.load(&ExecutionId(args.execution_id))?;
    github::post_or_update_report(&workspace_root, args.pr, &execution).await?;
    output::eprint_notice(&format!(
        "Posted report for {} to PR #{}",
        execution.id, args.pr
    ));
    Ok(0)
}

/// Validate a `/ksforge choose <option>` comment against an execution's
/// open gate and print the option id on success (spec §pewY5yG/§DlruVSP/§D3QZhrY). Does not
/// itself resume — chain with `ksforge resume <id> --decision <option>
/// --decided-by <commenter>` so exactly one code path ever talks to Claude
/// Code again.
pub async fn handle_comment(args: HandleCommentArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let store = workspace::ExecutionStore::new(&workspace_root);

    if let Some(option) = github::parse_choose_command(&args.body) {
        return handle_choose(&workspace_root, &store, args, option).await;
    }
    if let Some(follow_up) = github::parse_follow_up_command(&args.body) {
        return handle_follow_up(&workspace_root, &store, args, follow_up).await;
    }
    Err(KsforgeError::Usage(
        "comment does not contain a recognized `/ksforge` command (`choose <option>`, \
         `implement <change-request>`, or `fix <change-request>`)"
            .into(),
    ))
}

async fn handle_choose(
    workspace_root: &std::path::Path,
    store: &workspace::ExecutionStore,
    args: HandleCommentArgs,
    option: String,
) -> Result<i32> {
    let execution_id = args
        .execution_id
        .clone()
        .ok_or_else(|| KsforgeError::Usage("`/ksforge choose` requires --execution-id".into()))?;
    let id = ExecutionId(execution_id.clone());

    if store.event_already_processed(&id, &args.comment_id) {
        output::eprint_notice("Comment already processed; nothing to do.");
        return Ok(0);
    }

    if !github::authorize_commenter(workspace_root, &args.commenter).await? {
        return Err(KsforgeError::PolicyViolation(format!(
            "{} is not authorized to decide on {execution_id}",
            args.commenter
        )));
    }

    let execution = store.load(&id)?;
    if execution.status != ExecutionStatus::WaitingForHuman {
        return Err(KsforgeError::Usage(format!(
            "{execution_id} is not waiting for human input (status: {})",
            execution.status
        )));
    }
    let gate = execution.pending_question.as_ref().ok_or_else(|| {
        KsforgeError::Workspace(format!("{execution_id} has no open gate on record"))
    })?;
    if !gate.options.iter().any(|o| o.id == option) {
        return Err(KsforgeError::Usage(format!(
            "'{option}' is not one of the offered options: {}",
            gate.options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    store.mark_event_processed(&id, &args.comment_id)?;

    println!("{option}");
    write_github_output(&[
        ("kind", "choose"),
        ("execution-id", &execution_id),
        ("decision", &option),
    ]);
    Ok(0)
}

/// Unlike `handle_choose` (which only ever picks among options the agent
/// itself already offered on an existing execution), this starts a brand
/// new, write-capable, billed run from arbitrary comment text — the
/// authorization check below is therefore the load-bearing safeguard, not
/// the comment's shape (section: docs/09-security.md, "Integrations" for
/// the analogous reasoning). Idempotency is keyed on a fixed pseudo-id
/// rather than a real `ExecutionId`, since a follow-up has none yet at
/// this point — a GitHub comment id is already unique repo-wide, so one
/// shared namespace for every follow-up comment is enough.
async fn handle_follow_up(
    workspace_root: &std::path::Path,
    store: &workspace::ExecutionStore,
    args: HandleCommentArgs,
    follow_up: github::FollowUpCommand,
) -> Result<i32> {
    let scope = ExecutionId("ksf_pr_followups".to_string());
    if store.event_already_processed(&scope, &args.comment_id) {
        output::eprint_notice("Comment already processed; nothing to do.");
        return Ok(0);
    }

    if !github::authorize_commenter(workspace_root, &args.commenter).await? {
        return Err(KsforgeError::PolicyViolation(format!(
            "{} is not authorized to trigger a {} run via comment",
            args.commenter, follow_up.capability
        )));
    }

    store.mark_event_processed(&scope, &args.comment_id)?;

    println!("{}", follow_up.capability);
    write_github_output(&[("kind", "follow_up"), ("capability", &follow_up.capability)]);
    write_github_output_multiline("change_request", &follow_up.change_request);
    Ok(0)
}

fn write_github_output(pairs: &[(&str, &str)]) {
    let Ok(path) = std::env::var("GITHUB_OUTPUT") else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        for (key, value) in pairs {
            let _ = writeln!(file, "{key}={value}");
        }
    }
}

/// `GITHUB_OUTPUT`'s multi-line form (`name<<DELIM` ... `DELIM`) — a plain
/// `key=value` line cannot carry a change request that spans several lines.
fn write_github_output_multiline(key: &str, value: &str) {
    let Ok(path) = std::env::var("GITHUB_OUTPUT") else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{key}<<KSFORGE_COMMENT_EOF");
        let _ = writeln!(file, "{value}");
        let _ = writeln!(file, "KSFORGE_COMMENT_EOF");
    }
}

pub fn capabilities() -> i32 {
    let registry = CapabilityRegistry::with_defaults();
    println!("Capabilities:");
    for capability in registry.list() {
        println!("  {:<10} {}", capability.id(), capability.description());
    }
    0
}

/// `@<path>` reads the change request from a file instead of taking `value`
/// as the literal text — the same convention `curl`/`gh` use for "this
/// value, or a file holding it," so `--change` never needs a second flag.
fn load_change_request(change_request: Option<String>) -> Result<ChangeRequest> {
    let value = change_request.ok_or_else(|| KsforgeError::Usage("--change is required".into()))?;
    match value.strip_prefix('@') {
        Some(path) => ChangeRequest::from_file(std::path::Path::new(path)),
        None => ChangeRequest::from_text(value),
    }
}

async fn maybe_open_pull_request(
    workspace_root: &std::path::Path,
    execution: &Execution,
    common: &CommonArgs,
) -> Result<()> {
    if common.push_to_branch {
        let batch = github::push_follow_up(workspace_root, execution).await?;
        if batch.outcomes.is_empty() && batch.unversioned.is_empty() {
            output::eprint_notice("--push-to-branch set, but there is nothing to push yet.");
        } else {
            output::print_repo_commit_batch(&batch, workspace_root);
        }
        return Ok(());
    }
    if !common.create_pull_request {
        return Ok(());
    }
    let batch =
        github::create_from_execution(workspace_root, execution, &common.base_branch).await?;
    if batch.outcomes.is_empty() && batch.unversioned.is_empty() {
        output::eprint_notice(
            "--create-pull-request set, but there is nothing to open a PR for yet.",
        );
    } else {
        output::print_pull_request_batch(&batch, workspace_root);
    }
    Ok(())
}

/// See docs/04-cli-reference.md for the full exit code table.
fn exit_code_for(execution: &Execution) -> i32 {
    match execution.status {
        ExecutionStatus::Completed => 0,
        ExecutionStatus::WaitingForHuman => 6,
        ExecutionStatus::Failed => {
            if execution
                .messages
                .iter()
                .any(|e| matches!(e, ExecutionEvent::ValidationFailed { .. }))
            {
                4
            } else {
                3
            }
        }
        ExecutionStatus::Cancelled | ExecutionStatus::Running => 1,
    }
}

#[cfg(test)]
mod load_change_request_tests {
    use super::load_change_request;

    #[test]
    fn none_is_a_usage_error() {
        let err = load_change_request(None).unwrap_err();
        assert!(matches!(err, crate::domain::KsforgeError::Usage(_)));
    }

    #[test]
    fn plain_text_is_taken_literally() {
        let cr = load_change_request(Some("As a user, I want X.".into())).unwrap();
        assert_eq!(cr.text, "As a user, I want X.");
    }

    #[test]
    fn at_prefix_reads_the_named_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("change.md");
        std::fs::write(&path, "From a file.").unwrap();

        let cr = load_change_request(Some(format!("@{}", path.display()))).unwrap();
        assert_eq!(cr.text, "From a file.");
    }

    #[test]
    fn at_prefix_with_a_missing_file_is_a_usage_error() {
        let err = load_change_request(Some("@does-not-exist.md".into())).unwrap_err();
        assert!(matches!(err, crate::domain::KsforgeError::Usage(_)));
    }
}
