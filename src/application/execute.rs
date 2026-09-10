use crate::agent::{AgentOutcome, AgentRequest, OutcomeStatus, PermissionMode};
use crate::domain::{
    Capability, DecisionOption, Execution, ExecutionContext, ExecutionEvent, ExecutionResult,
    ImplementationRequest, KsforgeError, Result, ToolPolicy, ValidationOutcome,
};
use crate::workspace::{self, ExecutionStore, StoryArchive};
use crate::{application::prompt, validation};

/// The one execution pipeline shared by every capability (section 19: don't
/// duplicate this per-command). A capability supplies policy; this function
/// supplies mechanism: isolate-if-dry-run, prompt, invoke Claude Code,
/// interpret the result, validate, persist.
pub async fn run(
    capability: &dyn Capability,
    request: ImplementationRequest,
    context: ExecutionContext,
) -> Result<Execution> {
    let store = ExecutionStore::new(&request.workspace);
    let mut execution = Execution::start(request.story.clone(), capability.id());
    store.save_request(&execution.id, &request)?;
    store.save(&execution)?;

    // ksforge itself owns turning the raw story text into a durable
    // markdown + numbered-JSON record (section: story archive) — callers
    // (e.g. the GitHub Action) only ever hand it a plain string.
    StoryArchive::new(&request.workspace).record(&request.story, capability.id(), &execution.id)?;

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

    let (system_prompt, user_prompt) = prompt::build(capability, &request);
    let (tools, permission_mode) = tools_and_permission(capability.tool_policy());

    execution.record(ExecutionEvent::AgentStarted {
        at: chrono::Utc::now(),
    });

    let agent_request = AgentRequest {
        prompt: user_prompt,
        system_prompt: Some(system_prompt),
        working_directory: working_dir.clone(),
        tools,
        model: context.model.clone(),
        permission_mode,
        json_schema: Some(crate::agent::outcome::schema()),
        max_budget_usd: context.max_budget_usd,
        resume_session_id: None,
        mcp_config: context.mcp_config.clone(),
    };

    let agent_result = context.executor.execute(agent_request).await.map_err(|e| {
        KsforgeError::ExecutorFailed(format!("{} ({} capability): {e}", e, capability.id()))
    })?;
    execution.agent_session_id = agent_result.session_id.clone();

    let outcome: AgentOutcome = crate::agent::outcome::parse(&agent_result)
        .map_err(|e| KsforgeError::ExecutorFailed(e.to_string()))?;

    finish(
        &mut execution,
        capability,
        outcome,
        &working_dir,
        &before,
        &request,
    )
    .await?;

    store.save(&execution)?;
    Ok(execution)
}

pub(crate) fn tools_and_permission(policy: ToolPolicy) -> (Vec<String>, PermissionMode) {
    match policy {
        ToolPolicy::ReadOnly => (
            vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()],
            PermissionMode::ReadOnly,
        ),
        ToolPolicy::ReadWrite => (Vec::new(), PermissionMode::AcceptEdits),
    }
}

pub(crate) async fn finish(
    execution: &mut Execution,
    capability: &dyn Capability,
    outcome: AgentOutcome,
    working_dir: &std::path::Path,
    before: &std::collections::BTreeMap<std::path::PathBuf, String>,
    request: &ImplementationRequest,
) -> Result<()> {
    match outcome.status {
        OutcomeStatus::WaitingForHuman if capability.supports_human_interaction() => {
            let question = outcome
                .question
                .unwrap_or_else(|| "Claude Code needs a decision to continue.".to_string());
            let options: Vec<DecisionOption> = outcome.options;
            // A recommendation that names an option the agent didn't
            // actually offer is not trustworthy (section: never fully
            // trust model-reported facts) — drop it rather than surface a
            // dangling recommendation no option matches.
            let recommended_option = outcome
                .recommended_option
                .filter(|rec| options.iter().any(|o| &o.id == rec));
            let context = outcome.recommendation.unwrap_or_default();
            execution.ask(
                question,
                options,
                recommended_option,
                context,
                outcome.completed,
            );
            Ok(())
        }
        OutcomeStatus::WaitingForHuman => {
            // This capability never offered the human-interaction protocol
            // in its prompt, so a `waiting_for_human` reply here means the
            // agent output could not be trusted to follow instructions.
            execution.fail(format!(
                "{} does not support human-in-the-loop decisions, but the agent asked one: {}",
                capability.id(),
                outcome.question.unwrap_or_default()
            ));
            Ok(())
        }
        OutcomeStatus::Failed => {
            execution.fail(outcome.failure_reason.unwrap_or(outcome.summary));
            Ok(())
        }
        OutcomeStatus::Completed => {
            let after = workspace::snapshot::hash_tree(working_dir)?;
            // Filesystem bytes are ground truth; the model's own
            // `changed_files` claim is advisory only (section 13: never
            // fully trust model-reported facts about what it did).
            let changed = workspace::snapshot::changed_files(before, &after);

            let validation = if request.validation.commands.is_empty() {
                ValidationOutcome::default()
            } else {
                execution.record(ExecutionEvent::ValidationStarted {
                    at: chrono::Utc::now(),
                });
                let outcome = validation::run(&request.validation.commands, working_dir).await?;
                if outcome.passed {
                    execution.record(ExecutionEvent::ValidationPassed {
                        at: chrono::Utc::now(),
                    });
                } else {
                    let detail = outcome
                        .commands
                        .last()
                        .map(|c| format!("`{}` failed", c.command))
                        .unwrap_or_else(|| "validation failed".into());
                    execution.record(ExecutionEvent::ValidationFailed {
                        detail: detail.clone(),
                        at: chrono::Utc::now(),
                    });
                    execution.fail(detail);
                    return Ok(());
                }
                outcome
            };

            execution.complete(ExecutionResult {
                success: true,
                summary: outcome.summary,
                changed_files: changed,
                validation,
                completed: outcome.completed,
                open_items: outcome.open_items,
                recommendation: outcome.recommendation,
            });
            Ok(())
        }
    }
}
