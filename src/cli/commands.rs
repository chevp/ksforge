use std::sync::Arc;

use crate::agent::{AgentExecutor, ClaudeCodeExecutor, CodexExecutor};
use crate::domain::{
    CapabilityRegistry, Execution, ExecutionContext, ExecutionEvent, ExecutionId, ExecutionStatus,
    ImplementationRequest, KsforgeError, Result, UserStory, ValidationPolicy,
};
use crate::{github, workspace};

use super::args::{
    CancelArgs, CommonArgs, Engine, HandleCommentArgs, PostReportArgs, ResumeArgs, StatusArgs,
    StoryArgs,
};
use super::output;

/// Spawns the CLI selected by `--engine`, resolved from an explicit path or
/// PATH (section 32).
fn build_executor(common: &CommonArgs) -> Result<Arc<dyn AgentExecutor>> {
    match common.engine {
        Engine::Claude => {
            let executor = ClaudeCodeExecutor::discover(common.claude_path.as_deref())
                .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;
            Ok(Arc::new(executor))
        }
        Engine::Codex => {
            let executor = CodexExecutor::discover(common.codex_path.as_deref())
                .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;
            Ok(Arc::new(executor))
        }
    }
}

/// `--model` defaults to Claude Code's "sonnet" alias only for `--engine
/// claude` (see `CommonArgs::model`'s doc comment) — passing that alias to
/// Codex would be meaningless, so `--engine codex` leaves it unset instead,
/// deferring to Codex's own configured default.
fn resolve_model(common: &CommonArgs) -> Option<String> {
    match common.engine {
        Engine::Claude => Some(common.model.clone().unwrap_or_else(|| "sonnet".to_string())),
        Engine::Codex => common.model.clone(),
    }
}

pub async fn story_capability(capability_id: &str, args: StoryArgs) -> Result<i32> {
    let story = load_story(args.story, args.story_file)?;
    let common = args.common;

    let workspace_root = workspace::resolve_root(&common.workspace)?;
    let registry = CapabilityRegistry::with_defaults();
    let capability = registry
        .get(capability_id)
        .expect("capability_id is one of the built-in ids wired in main.rs");

    let executor = build_executor(&common)?;

    let request = ImplementationRequest {
        story,
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
        model: resolve_model(&common),
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

    let executor = build_executor(&common)?;
    let context = ExecutionContext {
        executor,
        workspace_root: workspace_root.clone(),
        model: resolve_model(&common),
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
    let id = ExecutionId(args.execution_id.clone());

    if store.event_already_processed(&id, &args.comment_id) {
        output::eprint_notice("Comment already processed; nothing to do.");
        return Ok(0);
    }

    let option = github::parse_choose_command(&args.body).ok_or_else(|| {
        KsforgeError::Usage("comment does not contain a `/ksforge choose <option>` command".into())
    })?;

    if !github::authorize_commenter(&workspace_root, &args.commenter).await? {
        return Err(KsforgeError::PolicyViolation(format!(
            "{} is not authorized to decide on {}",
            args.commenter, args.execution_id
        )));
    }

    let execution = store.load(&id)?;
    if execution.status != ExecutionStatus::WaitingForHuman {
        return Err(KsforgeError::Usage(format!(
            "{} is not waiting for human input (status: {})",
            args.execution_id, execution.status
        )));
    }
    let gate = execution.pending_question.as_ref().ok_or_else(|| {
        KsforgeError::Workspace(format!("{} has no open gate on record", args.execution_id))
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
    if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "execution-id={}", args.execution_id);
            let _ = writeln!(file, "decision={option}");
        }
    }
    Ok(0)
}

pub fn capabilities() -> i32 {
    let registry = CapabilityRegistry::with_defaults();
    println!("Capabilities:");
    for capability in registry.list() {
        println!("  {:<10} {}", capability.id(), capability.description());
    }
    0
}

fn load_story(story: Option<String>, story_file: Option<std::path::PathBuf>) -> Result<UserStory> {
    match (story, story_file) {
        (Some(text), None) => UserStory::from_text(text),
        (None, Some(path)) => UserStory::from_file(&path),
        (None, None) => Err(KsforgeError::Usage(
            "one of --story or --story-file is required".into(),
        )),
        (Some(_), Some(_)) => {
            unreachable!("clap enforces --story/--story-file are mutually exclusive")
        }
    }
}

async fn maybe_open_pull_request(
    workspace_root: &std::path::Path,
    execution: &Execution,
    common: &CommonArgs,
) -> Result<()> {
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
