use crate::agent::AgentRequest;
use crate::application::execute::{finish, tools_and_permission};
use crate::domain::{
    CapabilityRegistry, Execution, ExecutionContext, ExecutionId, ExecutionStatus, HumanDecision,
    KsforgeError, Result,
};
use crate::workspace::{self, ExecutionStore};

/// Continue a paused `Execution` with the human's decision. Prefers
/// resuming the original Claude Code conversation via `--resume
/// <session_id>` (full context, when the session is still on disk or was
/// restored from a cache — see docs/06); falls back to a fresh turn that
/// states the story and the decision explicitly when no session id was
/// recorded (e.g. a `MockAgentExecutor` in tests, or a cross-machine
/// resume with no cached session store).
pub async fn resume(
    workspace_root: &std::path::Path,
    id: &ExecutionId,
    option: String,
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
    let pending = execution.pending_question.clone().ok_or_else(|| {
        KsforgeError::Workspace(format!("execution {id} has no pending question on record"))
    })?;
    let chosen = pending
        .options
        .iter()
        .find(|o| o.id == option)
        .ok_or_else(|| {
            KsforgeError::Usage(format!(
                "'{option}' is not one of the offered options for execution {id}: {}",
                pending
                    .options
                    .iter()
                    .map(|o| o.id.as_str())
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
        option: option.clone(),
    });
    store.save(&execution)?;

    let isolated = if context.dry_run {
        Some(workspace::isolate::prepare(&request.workspace)?)
    } else {
        None
    };
    let working_dir = isolated
        .as_ref()
        .map(|w| w.path().to_path_buf())
        .unwrap_or_else(|| request.workspace.clone());
    let before = workspace::snapshot::hash_tree(&working_dir)?;

    let (system_prompt, base_user_prompt) =
        crate::application::prompt::build(&*capability, &request);
    let user_prompt = format!(
        "{base_user_prompt}\n\nYou previously asked: {}\nThe human decided: {} ({})\nContinue the {} capability, taking this decision as settled; do not ask about it again.",
        pending.question,
        chosen.label,
        chosen.id,
        capability.id()
    );
    let (tools, permission_mode) = tools_and_permission(capability.tool_policy());

    let agent_request = AgentRequest {
        prompt: user_prompt,
        system_prompt: Some(system_prompt),
        working_directory: working_dir.clone(),
        tools,
        model: context.model.clone(),
        permission_mode,
        json_schema: Some(crate::agent::outcome::schema()),
        max_budget_usd: context.max_budget_usd,
        resume_session_id: execution.agent_session_id.clone(),
    };

    let agent_result = context
        .executor
        .execute(agent_request)
        .await
        .map_err(|e| KsforgeError::ExecutorFailed(format!("{e} (resuming {id})")))?;
    if let Some(session_id) = &agent_result.session_id {
        execution.agent_session_id = Some(session_id.clone());
    }

    let outcome = crate::agent::outcome::parse(&agent_result)
        .map_err(|e| KsforgeError::ExecutorFailed(e.to_string()))?;

    finish(
        &mut execution,
        &*capability,
        outcome,
        &working_dir,
        &before,
        &request,
    )
    .await?;
    store.save(&execution)?;
    Ok(execution)
}
