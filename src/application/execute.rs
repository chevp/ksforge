use std::path::Path;
use std::time::Duration;

use crate::agent::{AgentOutcome, AgentRequest, AgentResult, OutcomeStatus, PermissionMode};
use crate::color::{self, Color};
use crate::domain::{
    ActionKind, ActionResult, AgentValidationReport, Capability, ChangeScope, DecisionOption,
    Execution, ExecutionContext, ExecutionEvent, ExecutionResult, ImplementationRequest,
    KsforgeError, LocatedContext, Result, ToolPolicy, TransitionError, Understanding,
    ValidationOutcome, WorkflowState,
};
use crate::workspace::{self, ChangeRequestArchive, ExecutionStore};
use crate::{application::prompt, validation};

/// The one execution pipeline shared by every capability. A capability
/// supplies policy; this module supplies mechanism: isolate-if-dry-run,
/// drive the UNDERSTAND -> LOCATE -> ACT -> VALIDATE -> REPORT phase loop
/// (`domain::workflow::WorkflowState`), persist after every phase.
pub async fn run(
    capability: &dyn Capability,
    request: ImplementationRequest,
    context: ExecutionContext,
) -> Result<Execution> {
    let store = ExecutionStore::new(&request.workspace);
    let mut execution = Execution::start(request.change_request.clone(), capability.id());
    store.save_request(&execution.id, &request)?;
    store.save(&execution)?;

    // ksforge itself owns turning the raw change request text into a durable
    // markdown + numbered-JSON record (change request archive) — callers
    // (e.g. the GitHub Action) only ever hand it a plain string.
    ChangeRequestArchive::new(&request.workspace).record(
        &request.change_request,
        capability.id(),
        &execution.id,
    )?;

    let isolated = if context.dry_run {
        Some(workspace::isolate::prepare(&request.workspace)?)
    } else {
        None
    };
    let working_dir = isolated
        .as_ref()
        .map(|w| workspace::isolate::strip_windows_verbatim_prefix(w.path().to_path_buf()))
        .unwrap_or_else(|| request.workspace.clone());

    let mut session_id: Option<String> = None;
    run_phase_loop(
        &mut execution,
        capability,
        &request,
        &context,
        &working_dir,
        &mut session_id,
        &store,
        None,
    )
    .await?;

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

/// The VALIDATE turn's fixed tool scope: the same for every capability,
/// unrelated to `Capability::tool_policy()` — it can execute (to actually
/// build/test/package what it's checking) but never edit.
fn validate_tools_and_permission() -> (Vec<String>, PermissionMode) {
    (
        vec![
            "Read".to_string(),
            "Grep".to_string(),
            "Glob".to_string(),
            "Bash".to_string(),
            "PowerShell".to_string(),
        ],
        PermissionMode::ExecuteOnly,
    )
}

fn workflow_corruption(e: TransitionError) -> KsforgeError {
    // In normal operation this is unreachable: each match arm below only
    // ever calls the `complete_*` that matches the state it just matched
    // on. It can only fire against a hand-edited/corrupted `state.json`.
    KsforgeError::Workspace(format!("corrupted execution state: {e}"))
}

/// The first line of a phase's summary, for a one-line progress notice —
/// the rest is still in `execution.result`/`--format json` for anyone who
/// needs it in full.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

/// Phase-progress banners on stderr, colored like `cargo`/`rustc`'s own
/// status lines (bold cyan for in-progress, bold green for done, bold red
/// for failed) — see `crate::color` for why this is stderr-only.
fn phase_start(line: &str) {
    eprintln!("\n{}", color::paint(line, Color::Cyan, true));
}

fn phase_done(line: &str) {
    eprintln!("{}", color::paint(line, Color::Green, true));
}

fn phase_failed(line: &str) {
    eprintln!("{}", color::paint(line, Color::Red, true));
}

/// What an LLM phase turn (UNDERSTAND/LOCATE/ACT) produced. VALIDATE/REPORT
/// never reach this — they run no agent turn at all.
enum PhaseTurn {
    /// Completed; the caller extracts phase-specific fields from `AgentOutcome`
    /// and advances `WorkflowState` itself (what to extract differs per phase).
    Advance(Box<AgentOutcome>),
    /// Paused (human gate) or failed — already applied to `execution`
    /// (`execution.ask`/`execution.fail`) and `execution.workflow` restored
    /// to `current`; the caller just stops.
    Stopped,
}

/// Retries a transient executor failure (network blip, provider rate
/// limit/overload — see `AgentError::is_transient`) with exponential
/// backoff, up to `MAX_ATTEMPTS` total tries, before giving up. A phase
/// turn only runs after UNDERSTAND/LOCATE (and their already-spent cost)
/// have completed, so one flaky call should not throw the whole execution
/// away. Anything not classified as transient (a bad request, a missing
/// engine, the model's own malformed output) fails on the first attempt,
/// same as before this existed.
const MAX_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

async fn execute_with_retry(
    context: &ExecutionContext,
    request: &AgentRequest,
    capability: &dyn Capability,
) -> Result<AgentResult> {
    let mut attempt = 1;
    loop {
        match context.executor.execute(request.clone()).await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < MAX_ATTEMPTS && e.is_transient() => {
                let delay = RETRY_BASE_DELAY * 2u32.pow(attempt - 1);
                eprintln!(
                    "  {}",
                    color::paint(
                        &format!(
                            "[ksforge] transient executor error, retrying in {}s (attempt {} of {MAX_ATTEMPTS}): {e}",
                            delay.as_secs(),
                            attempt + 1,
                        ),
                        Color::Yellow,
                        false,
                    )
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => {
                return Err(KsforgeError::ExecutorFailed(format!(
                    "{} ({} capability): {e}",
                    e,
                    capability.id()
                )));
            }
        }
    }
}

/// Runs one LLM phase turn: builds the `AgentRequest` for `tool_policy`,
/// invokes the executor, and interprets the resulting `AgentOutcome` — the
/// same three-way status handling every phase needs (human gate / failure /
/// completed), factored out once instead of duplicated per phase.
#[allow(clippy::too_many_arguments)]
async fn run_llm_phase(
    execution: &mut Execution,
    capability: &dyn Capability,
    context: &ExecutionContext,
    working_dir: &Path,
    session_id: &mut Option<String>,
    current: WorkflowState,
    tools: Vec<String>,
    permission_mode: PermissionMode,
    system_prompt: String,
    user_prompt: String,
) -> Result<PhaseTurn> {
    execution.record(ExecutionEvent::AgentStarted {
        at: chrono::Utc::now(),
    });

    let agent_request = AgentRequest {
        prompt: user_prompt,
        system_prompt: Some(system_prompt),
        working_directory: working_dir.to_path_buf(),
        tools,
        model: context.model.clone(),
        permission_mode,
        json_schema: Some(crate::agent::outcome::schema()),
        max_budget_usd: context.max_budget_usd,
        resume_session_id: session_id.clone(),
        mcp_config: context.mcp_config.clone(),
    };

    let agent_result = execute_with_retry(context, &agent_request, capability).await?;
    if let Some(sid) = &agent_result.session_id {
        *session_id = Some(sid.clone());
    }
    execution.agent_session_id = session_id.clone();

    let outcome: AgentOutcome = crate::agent::outcome::parse(&agent_result)
        .map_err(|e| KsforgeError::ExecutorFailed(e.to_string()))?;

    match outcome.status {
        OutcomeStatus::WaitingForHuman
            if capability.supports_human_interaction() && outcome.options.is_empty() =>
        {
            // A gate with no options can never be resumed (`resume` only
            // accepts a decision matching one of `pending.options[].id`) —
            // that is a dead end, not a valid pause. Fail loudly instead of
            // parking the execution somewhere it can never leave.
            execution.workflow = current;
            execution.fail(format!(
                "agent asked a question without offering any options: {}",
                outcome
                    .question
                    .unwrap_or_else(|| "(no question text)".to_string())
            ));
            Ok(PhaseTurn::Stopped)
        }
        OutcomeStatus::WaitingForHuman if capability.supports_human_interaction() => {
            execution.workflow = current;
            let question = outcome
                .question
                .unwrap_or_else(|| "Claude Code needs a decision to continue.".to_string());
            let options: Vec<DecisionOption> = outcome.options;
            // A recommendation that names an option the agent didn't
            // actually offer is not trustworthy — drop it rather than
            // surface a dangling recommendation no option matches.
            let recommended_option = outcome
                .recommended_option
                .filter(|rec| options.iter().any(|o| &o.id == rec));
            let ctx = outcome.recommendation.unwrap_or_default();
            execution.ask(
                question,
                options,
                recommended_option,
                ctx,
                outcome.completed,
            );
            Ok(PhaseTurn::Stopped)
        }
        OutcomeStatus::WaitingForHuman => {
            // This capability never offered the human-interaction protocol
            // in its prompt, so a `waiting_for_human` reply here means the
            // agent output could not be trusted to follow instructions.
            execution.workflow = current;
            execution.fail(format!(
                "{} does not support human-in-the-loop decisions, but the agent asked one: {}",
                capability.id(),
                outcome.question.unwrap_or_default()
            ));
            Ok(PhaseTurn::Stopped)
        }
        OutcomeStatus::Failed => {
            execution.workflow = current;
            execution.fail(outcome.failure_reason.unwrap_or(outcome.summary));
            Ok(PhaseTurn::Stopped)
        }
        OutcomeStatus::Completed => Ok(PhaseTurn::Advance(Box::new(outcome))),
    }
}

/// Drives `execution.workflow` from wherever it currently is through to
/// `Report` (or a pause/failure). Shared by a fresh `run()` and a `resume()`
/// continuing a paused execution — `decision_suffix`, when `Some`, is
/// appended to the *first* phase turn's user prompt only (the human's
/// decision, restated explicitly rather than relied on via `--resume`
/// memory alone — same idiom the old single-turn `resume` already used).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_phase_loop(
    execution: &mut Execution,
    capability: &dyn Capability,
    request: &ImplementationRequest,
    context: &ExecutionContext,
    working_dir: &Path,
    session_id: &mut Option<String>,
    store: &ExecutionStore,
    mut decision_suffix: Option<String>,
) -> Result<()> {
    loop {
        let state = std::mem::take(&mut execution.workflow);
        match state {
            WorkflowState::Understand => {
                phase_start("== Understand: analyzing the change request ==");
                let (system_prompt, mut user_prompt) =
                    prompt::for_understand(capability, request, working_dir);
                if let Some(suffix) = decision_suffix.take() {
                    user_prompt.push_str(&suffix);
                }
                let (tools, permission_mode) = tools_and_permission(ToolPolicy::ReadOnly);
                let turn = run_llm_phase(
                    execution,
                    capability,
                    context,
                    working_dir,
                    session_id,
                    WorkflowState::Understand,
                    tools,
                    permission_mode,
                    system_prompt,
                    user_prompt,
                )
                .await?;
                store.save(execution)?;
                let PhaseTurn::Advance(outcome) = turn else {
                    return Ok(());
                };
                let understanding = Understanding {
                    summary: outcome.summary,
                    scope: outcome.scope,
                };
                phase_done(&format!(
                    "== Understand: done — {} ==",
                    first_line(&understanding.summary)
                ));
                let next = WorkflowState::Understand
                    .complete_understand(understanding)
                    .map_err(workflow_corruption)?;
                execution.advance(next);
                store.save(execution)?;
            }
            WorkflowState::Locate { understanding } => {
                phase_start("== Locate: finding relevant files and conventions ==");
                let (system_prompt, mut user_prompt) =
                    prompt::for_locate(capability, request, working_dir, &understanding);
                if let Some(suffix) = decision_suffix.take() {
                    user_prompt.push_str(&suffix);
                }
                let current = WorkflowState::Locate {
                    understanding: understanding.clone(),
                };
                let (tools, permission_mode) = tools_and_permission(ToolPolicy::ReadOnly);
                let turn = run_llm_phase(
                    execution,
                    capability,
                    context,
                    working_dir,
                    session_id,
                    current.clone(),
                    tools,
                    permission_mode,
                    system_prompt,
                    user_prompt,
                )
                .await?;
                store.save(execution)?;
                let PhaseTurn::Advance(outcome) = turn else {
                    return Ok(());
                };
                let located = LocatedContext {
                    relevant_files: outcome.relevant_files,
                    existing_abstractions: outcome.existing_abstractions,
                    existing_tests: outcome.existing_tests,
                    conventions: outcome.conventions,
                };
                phase_done(&format!(
                    "== Locate: done — {} relevant file(s) ==",
                    located.relevant_files.len()
                ));
                let next = current
                    .complete_locate(located)
                    .map_err(workflow_corruption)?;
                execution.advance(next);
                store.save(execution)?;
            }
            WorkflowState::Act {
                understanding,
                located,
            } => {
                phase_start("== Act: implementing the change ==");
                // Understand/Locate are read-only, so nothing could have
                // changed the tree before this point — safe to snapshot
                // right here rather than at the very start of the run.
                let before = workspace::snapshot::hash_tree(working_dir)?;
                let (system_prompt, mut user_prompt) =
                    prompt::for_act(capability, request, working_dir, &understanding, &located);
                if let Some(suffix) = decision_suffix.take() {
                    user_prompt.push_str(&suffix);
                }
                let current = WorkflowState::Act {
                    understanding: understanding.clone(),
                    located: located.clone(),
                };
                let (tools, permission_mode) = tools_and_permission(capability.tool_policy());
                let turn = run_llm_phase(
                    execution,
                    capability,
                    context,
                    working_dir,
                    session_id,
                    current.clone(),
                    tools,
                    permission_mode,
                    system_prompt,
                    user_prompt,
                )
                .await?;
                store.save(execution)?;
                let PhaseTurn::Advance(outcome) = turn else {
                    return Ok(());
                };

                let after = workspace::snapshot::hash_tree(working_dir)?;
                // Filesystem bytes are ground truth; the model's own
                // `changed_files` claim is advisory only — never fully
                // trust model-reported facts about what it did.
                let changed_files = workspace::snapshot::changed_files(&before, &after);

                // Deterministic scope check — see `ChangeScope`. Not the
                // prompt's job to enforce; ksforge rejects the run itself
                // if the agent touched anything outside its declared scope,
                // regardless of what it reported.
                if let Some(scope) = capability.change_scope() {
                    let violations = match scope {
                        ChangeScope::TestsOnly => {
                            crate::domain::test_scope::violations(&changed_files, working_dir)
                        }
                    };
                    if !violations.is_empty() {
                        let paths = violations
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        phase_failed(&format!(
                            "== Act: failed — touched files outside scope: {paths} =="
                        ));
                        execution.workflow = current;
                        execution.fail(format!(
                            "`{}` may only touch test files, but changed: {paths}",
                            capability.id()
                        ));
                        store.save(execution)?;
                        return Ok(());
                    }
                }

                let kind = ActionKind::derive(capability.id(), changed_files.is_empty());

                phase_done(&format!(
                    "== Act: done — {} file(s) changed ==",
                    changed_files.len()
                ));

                let action = ActionResult {
                    kind,
                    title: outcome.title,
                    summary: outcome.summary,
                    changed_files,
                    completed: outcome.completed,
                    open_items: outcome.open_items,
                    recommendation: outcome.recommendation,
                };
                let next = current.complete_act(action).map_err(workflow_corruption)?;
                execution.advance(next);
                store.save(execution)?;
            }
            WorkflowState::Validate {
                understanding,
                located,
                action,
            } => {
                let current = WorkflowState::Validate {
                    understanding: understanding.clone(),
                    located,
                    action: action.clone(),
                };

                if !request.validation.commands.is_empty() || request.validation.agent_review {
                    execution.record(ExecutionEvent::ValidationStarted {
                        at: chrono::Utc::now(),
                    });
                }

                // Layer 1: exact, user-authored commands, run deterministically
                // — unchanged from before, still never decided by the model.
                let mut command_outcomes = Vec::new();
                if !request.validation.commands.is_empty() {
                    phase_start(&format!(
                        "== Validate: running {} command(s) ==",
                        request.validation.commands.len()
                    ));
                    let outcome =
                        validation::run(&request.validation.commands, working_dir).await?;
                    if !outcome.passed {
                        let detail = outcome
                            .commands
                            .last()
                            .map(|c| format!("`{}` failed", c.command))
                            .unwrap_or_else(|| "validation failed".into());
                        phase_failed(&format!("== Validate: failed — {detail} =="));
                        execution.record(ExecutionEvent::ValidationFailed {
                            detail: detail.clone(),
                            at: chrono::Utc::now(),
                        });
                        execution.workflow = current;
                        execution.fail(detail);
                        store.save(execution)?;
                        return Ok(());
                    }
                    phase_done("== Validate: commands passed ==");
                    command_outcomes = outcome.commands;
                }

                // Layer 2: an agent turn that works out *what* validating this
                // specific change requires and does it, with real (but
                // execute-only) tool access — see prompts/phases/validate.md.
                // Only *that* this turn happens is hardcoded; what it checks
                // is the agent's own judgment, never a ksforge command list.
                let agent_review = if request.validation.agent_review {
                    phase_start("== Validate: agent reviewing the change ==");
                    let before = workspace::snapshot::hash_tree(working_dir)?;
                    let (system_prompt, user_prompt) = prompt::for_validate(
                        capability,
                        request,
                        working_dir,
                        &understanding,
                        &action,
                    );
                    let (tools, permission_mode) = validate_tools_and_permission();
                    let turn = run_llm_phase(
                        execution,
                        capability,
                        context,
                        working_dir,
                        session_id,
                        current.clone(),
                        tools,
                        permission_mode,
                        system_prompt,
                        user_prompt,
                    )
                    .await?;
                    store.save(execution)?;
                    let PhaseTurn::Advance(outcome) = turn else {
                        return Ok(());
                    };

                    // Never fully trust the executor's own permission
                    // enforcement (§ExecuteOnly's doc comment) — the
                    // filesystem diff is the actual guarantee this turn
                    // didn't change anything.
                    let after = workspace::snapshot::hash_tree(working_dir)?;
                    let touched = workspace::snapshot::changed_files(&before, &after);
                    if !touched.is_empty() {
                        let paths = touched
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        let detail =
                            format!("validate must not modify the workspace, but changed: {paths}");
                        phase_failed(&format!("== Validate: failed — {detail} =="));
                        execution.record(ExecutionEvent::ValidationFailed {
                            detail: detail.clone(),
                            at: chrono::Utc::now(),
                        });
                        execution.workflow = current;
                        execution.fail(detail);
                        store.save(execution)?;
                        return Ok(());
                    }

                    phase_done(&format!(
                        "== Validate: done — {} ==",
                        first_line(&outcome.summary)
                    ));
                    Some(AgentValidationReport {
                        summary: outcome.summary,
                        completed: outcome.completed,
                        open_items: outcome.open_items,
                        recommendation: outcome.recommendation,
                    })
                } else {
                    None
                };

                if !command_outcomes.is_empty() || agent_review.is_some() {
                    execution.record(ExecutionEvent::ValidationPassed {
                        at: chrono::Utc::now(),
                    });
                }
                let validation = ValidationOutcome {
                    passed: true,
                    commands: command_outcomes,
                    agent_review,
                };
                let next = current
                    .complete_validate(validation)
                    .map_err(workflow_corruption)?;
                execution.advance(next);
                store.save(execution)?;
            }
            WorkflowState::Report {
                understanding,
                located,
                action,
                validation,
            } => {
                phase_start("== Report: finalizing ==");
                let result = ExecutionResult {
                    success: true,
                    title: action.title.clone(),
                    summary: action.summary.clone(),
                    changed_files: action.changed_files.clone(),
                    validation: validation.clone(),
                    completed: action.completed.clone(),
                    open_items: action.open_items.clone(),
                    recommendation: action.recommendation.clone(),
                };
                execution.workflow = WorkflowState::Report {
                    understanding,
                    located,
                    action,
                    validation,
                };
                execution.complete(result);
                store.save(execution)?;
                return Ok(());
            }
            WorkflowState::Legacy => {
                execution.workflow = WorkflowState::Legacy;
                return Err(KsforgeError::Usage(
                    "this execution predates the phase-based workflow; cancel it and re-run \
                     the capability instead"
                        .into(),
                ));
            }
        }
    }
}
