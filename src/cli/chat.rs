//! Interactive chat front end (docs/12-interactive-chat.md). Every turn is
//! still one ordinary `Execution` going through the same
//! `application::execute::run`/`application::resume::resume` pipeline as
//! `ksforge implement` — this module only adds a REPL, capability routing
//! by prefix, and a local (never `gh`) branch/commit/merge-back step.

use std::io::{self, Write};
use std::path::Path;

use crate::domain::{
    CapabilityRegistry, ChangeRequest, Execution, ExecutionContext, ExecutionId, ExecutionStatus,
    ImplementationRequest, KsforgeError, Result, ValidationPolicy,
};
use crate::{github, workspace};

use super::args::{ChatArgs, OutputFormat};
use super::commands::{build_executor, resolve_model};
use super::output;

pub async fn run(args: ChatArgs) -> Result<i32> {
    let workspace_root = workspace::resolve_root(&args.workspace)?;

    let registry = CapabilityRegistry::with_defaults();

    println!("ksforge chat — describe a change in plain language.");
    println!(
        "Prefix with 'review:', 'fix:', or 'explain:' to pick a capability other\nthan the default ('implement'). Type 'exit' to quit."
    );

    let mut pending: Option<ExecutionId> = None;

    loop {
        println!();
        let Some(raw) = prompt("ksforge> ")? else {
            println!();
            return Ok(0);
        };
        let line = raw.trim();

        if line.eq_ignore_ascii_case("exit") || line.eq_ignore_ascii_case("quit") {
            return Ok(0);
        }
        if line.is_empty() {
            continue;
        }

        let outcome = if let Some(execution_id) = pending.clone() {
            resume_turn(
                &workspace_root,
                &registry,
                &args,
                &execution_id,
                line.to_string(),
            )
            .await
        } else {
            let (capability_id, change_request_text) = route(line);
            run_turn(
                &workspace_root,
                &registry,
                &args,
                capability_id,
                change_request_text,
            )
            .await
        };

        pending = match outcome {
            Ok(next) => next,
            Err(e) => {
                eprintln!("ksforge: {e}");
                pending
            }
        };
    }
}

/// Reads one line from stdin. `Ok(None)` means EOF (Ctrl-D on a POSIX
/// shell, Ctrl-Z Enter on `cmd.exe`/PowerShell) — the session-ending
/// condition, distinct from an empty line.
fn prompt(label: &str) -> Result<Option<String>> {
    print!("{label}");
    io::stdout().flush().ok();
    let mut line = String::new();
    let bytes_read = io::stdin().read_line(&mut line).map_err(KsforgeError::Io)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    Ok(Some(line))
}

/// Capability routing (step 1): a leading `review:`/`fix:`/`explain:`
/// (case insensitive) picks that capability with the remaining text as the
/// change request; anything else runs `implement`. String routing, not an
/// LLM classification step — deliberately simple and deterministic.
fn route(line: &str) -> (&'static str, String) {
    for (prefix, capability) in [
        ("review:", "review"),
        ("fix:", "fix"),
        ("explain:", "explain"),
    ] {
        if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return (capability, line[prefix.len()..].trim().to_string());
        }
    }
    ("implement", line.to_string())
}

async fn run_turn(
    workspace_root: &Path,
    registry: &CapabilityRegistry,
    args: &ChatArgs,
    capability_id: &str,
    change_request_text: String,
) -> Result<Option<ExecutionId>> {
    let change_request = ChangeRequest::from_text(change_request_text)?;
    let capability = registry
        .get(capability_id)
        .expect("capability_id is one of the built-in ids routed in `route`");

    let executor = build_executor(args.claude_path.as_deref())?;
    let context = ExecutionContext {
        executor,
        workspace_root: workspace_root.to_path_buf(),
        model: resolve_model(args.model.clone()),
        max_budget_usd: args.max_budget_usd,
        dry_run: args.dry_run,
        mcp_config: args.mcp_config.clone(),
    };
    let request = ImplementationRequest {
        change_request,
        workspace: workspace_root.to_path_buf(),
        capability: capability_id.to_string(),
        constraints: capability.default_constraints(),
        validation: ValidationPolicy {
            commands: args.validate.clone(),
            agent_review: !args.no_validate,
        },
    };

    output::eprint_notice(&format!("Starte {capability_id}..."));
    let execution = capability.execute(request, context).await?;
    post_execution(workspace_root, args, execution).await
}

/// `application::resume::resume` returns a `Usage` error (option not among
/// those offered) before mutating anything, so the execution's stored
/// status is still `waiting_for_human` on error — the caller keeps `pending`
/// set to `execution_id` in that case, which is exactly what "the gate
/// stays open" (step 3) requires without any extra bookkeeping here.
async fn resume_turn(
    workspace_root: &Path,
    registry: &CapabilityRegistry,
    args: &ChatArgs,
    execution_id: &ExecutionId,
    decision: String,
) -> Result<Option<ExecutionId>> {
    let executor = build_executor(args.claude_path.as_deref())?;
    let context = ExecutionContext {
        executor,
        workspace_root: workspace_root.to_path_buf(),
        model: resolve_model(args.model.clone()),
        max_budget_usd: args.max_budget_usd,
        dry_run: args.dry_run,
        mcp_config: args.mcp_config.clone(),
    };

    let execution = crate::application::resume::resume(
        workspace_root,
        execution_id,
        decision,
        None,
        registry,
        context,
    )
    .await?;
    post_execution(workspace_root, args, execution).await
}

/// Shared tail for both a fresh turn and a resumed one (steps 3-6): print
/// the same transcript `ksforge status` would show, reopen the gate if the
/// turn paused, otherwise branch/commit/offer-to-merge when there is
/// something to commit.
async fn post_execution(
    workspace_root: &Path,
    args: &ChatArgs,
    execution: Execution,
) -> Result<Option<ExecutionId>> {
    output::print_execution(&execution, OutputFormat::Text);

    if execution.status == ExecutionStatus::WaitingForHuman {
        return Ok(Some(execution.id));
    }

    if !args.dry_run && execution.status == ExecutionStatus::Completed {
        maybe_branch_and_merge(workspace_root, &args.base_branch, &execution).await?;
    }

    Ok(None)
}

/// Steps 4-6: local branch + commit is unattended, merging into
/// `base_branch` is confirmed — once per repo the turn actually touched
/// (a chat turn can edit files across several repos in a multi-repo
/// workspace). A declined or EOF'd confirmation leaves that repo's commit
/// on its feature branch, checked out, so no work is lost; other repos are
/// asked about independently. Changed files with no discoverable git repo
/// (a plain non-git workspace folder) are reported, not silently dropped.
async fn maybe_branch_and_merge(
    workspace_root: &Path,
    base_branch: &str,
    execution: &Execution,
) -> Result<()> {
    let batch = github::commit_to_new_branch(workspace_root, execution).await?;
    if batch.outcomes.is_empty() {
        if !batch.unversioned.is_empty() {
            println!();
            output::print_repo_commit_batch(&batch, workspace_root);
        }
        return Ok(());
    }
    println!();
    output::print_repo_commit_batch(&batch, workspace_root);

    for outcome in &batch.outcomes {
        let confirmed = matches!(
            prompt(&format!(
                "'{}' nach '{base_branch}' mergen? [y/N] ",
                outcome.label
            ))?
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str(),
            "y" | "yes" | "j" | "ja"
        );
        if !confirmed {
            continue;
        }

        github::merge_branch_into(&outcome.repo, &outcome.label, base_branch).await?;
        println!(
            "Gemergt nach '{base_branch}', Branch '{}' gelöscht.",
            outcome.label
        );
    }
    Ok(())
}

#[cfg(test)]
mod route_tests {
    use super::route;

    #[test]
    fn defaults_to_implement() {
        assert_eq!(
            route("add a login page"),
            ("implement", "add a login page".to_string())
        );
    }

    #[test]
    fn routes_review_case_insensitively() {
        assert_eq!(
            route("Review: check the auth module"),
            ("review", "check the auth module".to_string())
        );
    }

    #[test]
    fn routes_fix_and_trims_the_remainder() {
        assert_eq!(
            route("fix:   the login button is dead"),
            ("fix", "the login button is dead".to_string())
        );
    }

    #[test]
    fn routes_explain() {
        assert_eq!(
            route("explain: how does auth work"),
            ("explain", "how does auth work".to_string())
        );
    }

    #[test]
    fn a_colon_with_no_recognized_prefix_still_implements() {
        assert_eq!(
            route("note: remember to update the docs"),
            ("implement", "note: remember to update the docs".to_string())
        );
    }
}
