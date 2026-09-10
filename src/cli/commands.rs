use std::sync::Arc;

use crate::agent::ClaudeCodeExecutor;
use crate::domain::{
    CapabilityRegistry, Execution, ExecutionContext, ExecutionEvent, ExecutionId, ExecutionStatus,
    ImplementationRequest, KsforgeError, Result, UserStory, ValidationPolicy,
};
use crate::{github, workspace};

use super::args::{CommonArgs, ResumeArgs, StatusArgs, StoryArgs};
use super::output;

pub async fn story_capability(capability_id: &str, args: StoryArgs) -> Result<i32> {
    let story = load_story(args.story, args.story_file)?;
    let common = args.common;

    let workspace_root = workspace::resolve_root(&common.workspace)?;
    let registry = CapabilityRegistry::with_defaults();
    let capability = registry
        .get(capability_id)
        .expect("capability_id is one of the built-in ids wired in main.rs");

    let executor = ClaudeCodeExecutor::discover(common.claude_path.as_deref())
        .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;

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
        executor: Arc::new(executor),
        workspace_root: workspace_root.clone(),
        model: common.model.clone(),
        max_budget_usd: common.max_budget_usd,
        dry_run: common.dry_run,
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

    let executor = ClaudeCodeExecutor::discover(common.claude_path.as_deref())
        .map_err(|e| KsforgeError::ExecutorUnavailable(e.to_string()))?;
    let context = ExecutionContext {
        executor: Arc::new(executor),
        workspace_root: workspace_root.clone(),
        model: common.model.clone(),
        max_budget_usd: common.max_budget_usd,
        dry_run: common.dry_run,
    };

    output::eprint_notice(&format!("Resuming {}...", args.execution_id));
    let execution = crate::application::resume::resume(
        &workspace_root,
        &ExecutionId(args.execution_id),
        args.decision,
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
