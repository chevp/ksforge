use std::sync::Arc;

use crate::agent::{AgentExecutor, ClaudeCodeExecutor, CodexExecutor};
use crate::domain::{
    CapabilityRegistry, ChangeRequest, Execution, ExecutionContext, ExecutionEvent, ExecutionId,
    ExecutionStatus, ImplementationRequest, KsforgeError, Result, ValidationPolicy,
};
use crate::{github, workspace};

use super::args::{
    CancelArgs, ChangeRequestArgs, CommonArgs, CoordinateArgs, Engine, HandleCommentArgs,
    PostReportArgs, ResumeArgs, StatusArgs,
};
use super::output;

/// Spawns the CLI selected by `--engine`, resolved from an explicit path or
/// PATH (section 32).
fn build_executor(
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
fn resolve_model(engine: Engine, model: Option<String>) -> Option<String> {
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
    let change_request = load_change_request(args.change_request, args.change_request_file)?;

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
    let change_request = load_change_request(args.change_request, args.change_request_file)?;
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
    let execution = capability.execute(request, context).await?;
    output::print_execution(&execution, common.format);

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
        context,
    )
    .await?;
    output::print_execution(&execution, common.format);

    maybe_open_pull_request(&workspace_root, &execution, &common).await?;

    Ok(exit_code_for(&execution))
}

pub fn status(args: StatusArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;
    let store = workspace::ExecutionStore::new(&workspace_root);
    let execution = store.load(&ExecutionId(args.execution_id))?;
    output::print_execution(&execution, args.format);
    Ok(exit_code_for(&execution))
}

/// Stop an execution that will not be resumed (spec §4/20) — distinct from
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
/// pull request (spec §13-15). Read-only against the execution itself —
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
/// open gate and print the option id on success (spec §16-17/23). Does not
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

fn load_change_request(
    change_request: Option<String>,
    change_request_file: Option<std::path::PathBuf>,
) -> Result<ChangeRequest> {
    match (change_request, change_request_file) {
        (Some(text), None) => ChangeRequest::from_text(text),
        (None, Some(path)) => ChangeRequest::from_file(&path),
        (None, None) => Err(KsforgeError::Usage(
            "one of --change-request or --change-request-file is required".into(),
        )),
        (Some(_), Some(_)) => {
            unreachable!(
                "clap enforces --change-request/--change-request-file are mutually exclusive"
            )
        }
    }
}

async fn maybe_open_pull_request(
    workspace_root: &std::path::Path,
    execution: &Execution,
    common: &CommonArgs,
) -> Result<()> {
    if common.push_to_branch {
        return match github::push_follow_up(workspace_root, execution).await? {
            Some(title) => {
                output::eprint_notice(&format!("Pushed: {title}"));
                Ok(())
            }
            None => {
                output::eprint_notice("--push-to-branch set, but there is nothing to push yet.");
                Ok(())
            }
        };
    }
    if !common.create_pull_request {
        return Ok(());
    }
    match github::create_from_execution(workspace_root, execution, &common.base_branch).await? {
        Some(pr) => {
            output::eprint_notice(&format!("Pull request: {}", pr.url.unwrap_or(pr.branch)));
        }
        None => {
            output::eprint_notice(
                "--create-pull-request set, but there is nothing to open a PR for yet.",
            );
        }
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
