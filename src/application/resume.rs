use crate::application::execute::run_phase_loop;
use crate::domain::{
    CapabilityRegistry, Execution, ExecutionContext, ExecutionId, ExecutionStatus, HumanDecision,
    KsforgeError, Result,
};
use crate::workspace::{self, ExecutionStore};

/// Continue a paused `Execution` with the human's decision, re-entering the
/// phase loop (`application::execute::run_phase_loop`) at exactly the phase
/// it paused at (`execution.workflow`) — never from the start. Prefers
/// resuming the original Claude Code conversation via `--resume
/// <session_id>` (full context, when the session is still on disk or was
/// restored from a cache — see docs/06) for continuity, but correctness
/// does not depend on it: the decision is also restated explicitly in that
/// phase's prompt, the same idiom this function already used before the
/// phase loop existed.
pub async fn resume(
    workspace_root: &std::path::Path,
    id: &ExecutionId,
    option: String,
    decided_by: Option<String>,
    registry: &CapabilityRegistry,
    context: ExecutionContext,
) -> Result<Execution> {
    let store = ExecutionStore::new(workspace_root);
    let mut execution = store.load(id)?;
    let request = store.load_request(id)?;

    if execution.status != ExecutionStatus::WaitingForHuman {
        return Err(KsforgeError::Usage(format!(
            "execution {id} is not waiting for human input (status: {})",
            execution.status
        )));
    }
    if execution.workflow.is_legacy() {
        return Err(KsforgeError::Usage(
            "this execution predates the phase-based workflow; cancel it and re-run the \
             capability instead"
                .into(),
        ));
    }
    let pending = execution.pending_question.clone().ok_or_else(|| {
        KsforgeError::Workspace(format!("execution {id} has no pending question on record"))
    })?;
    // Accept either the option's actual id or its position in the printed
    // list (`ksforge status`/the interactive prompt number the options
    // 0, 1, 2, ...) — an exact id match wins first, so an id that happens
    // to look like a number is never shadowed by the index it sits at.
    let chosen = pending
        .options
        .iter()
        .find(|o| o.id == option)
        .or_else(|| {
            option
                .parse::<usize>()
                .ok()
                .and_then(|i| pending.options.get(i))
        })
        .ok_or_else(|| {
            KsforgeError::Usage(format!(
                "'{option}' is not one of the offered options for execution {id}: {}",
                pending
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("{i}) {}", o.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?
        .clone();

    let capability = registry.get(&execution.capability).ok_or_else(|| {
        KsforgeError::Config(format!(
            "unknown capability '{}' on execution {id}",
            execution.capability
        ))
    })?;

    execution.apply_decision(&HumanDecision {
        execution_id: id.clone(),
        option: chosen.id.clone(),
        decided_by,
    });
    store.save(&execution)?;

    let isolated = if context.dry_run {
        Some(workspace::isolate::prepare(&request.workspace)?)
    } else {
        None
    };
    let working_dir = isolated
        .as_ref()
        .map(|w| workspace::isolate::strip_windows_verbatim_prefix(w.path().to_path_buf()))
        .unwrap_or_else(|| request.workspace.clone());

    let decision_suffix = format!(
        "\n\nYou previously asked: {}\nThe human decided: {} ({})\nContinue this phase, taking \
         this decision as settled; do not ask about it again.",
        pending.question, chosen.label, chosen.id
    );

    let mut session_id = execution.agent_session_id.clone();
    run_phase_loop(
        &mut execution,
        &*capability,
        &request,
        &context,
        &working_dir,
        &mut session_id,
        &store,
        Some(decision_suffix),
    )
    .await?;

    Ok(execution)
}
