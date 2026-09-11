use std::sync::Arc;

use ksforge::agent::{AgentError, MockAgentExecutor};
use ksforge::application::implement::Implement;
use ksforge::domain::{
    Capability, CapabilityRegistry, ChangeRequest, ExecutionContext, ExecutionStatus,
    ImplementationRequest, Phase, ValidationPolicy,
};
use ksforge::workspace::ExecutionStore;
use serde_json::{Value, json};

fn request(workspace: &std::path::Path) -> ImplementationRequest {
    ImplementationRequest {
        change_request: ChangeRequest::from_text("As a user, I want to log in.").unwrap(),
        workspace: workspace.to_path_buf(),
        capability: "implement".to_string(),
        constraints: Vec::new(),
        validation: ValidationPolicy::default(),
    }
}

/// A minimal `completed` response for the UNDERSTAND/LOCATE phases, which
/// this suite doesn't otherwise care about — every `implement`/`fix` run
/// now takes 3 real turns (UNDERSTAND, LOCATE, ACT) before VALIDATE/REPORT
/// (host-only, no agent turn), so a scripted `MockAgentExecutor` needs one
/// canned response per LLM turn, not one per capability run.
fn understand_ok() -> Value {
    json!({ "status": "completed", "summary": "Wants a login page." })
}

fn locate_ok() -> Value {
    json!({ "status": "completed", "summary": "Found the auth module." })
}

#[tokio::test]
async fn completed_run_persists_and_reports_success() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({
            "status": "completed",
            "summary": "Added login.",
            "changed_files": []
        }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let execution = Implement
        .execute(request(dir.path()), context)
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
    assert_eq!(execution.result.as_ref().unwrap().summary, "Added login.");
    // The final report only ever comes from having reached `Report` — this
    // also catches a persisted-state bug where a terminal branch forgot to
    // leave `execution.workflow` at its own phase instead of the
    // `#[serde(default)]` `Legacy` placeholder.
    assert_eq!(execution.workflow.phase(), Phase::Report);

    // Persisted for `ksforge status` / a later `resume` to find.
    let store = ExecutionStore::new(dir.path());
    let reloaded = store.load(&execution.id).unwrap();
    assert_eq!(reloaded.status, ExecutionStatus::Completed);
    assert_eq!(reloaded.workflow.phase(), Phase::Report);
}

#[tokio::test]
async fn dry_run_never_touches_the_real_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("marker.txt"), "untouched").unwrap();

    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({
            "status": "completed",
            "summary": "Would have added a file.",
            "changed_files": ["new.txt"]
        }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: true,
        mcp_config: None,
    };

    let execution = Implement
        .execute(request(dir.path()), context)
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
    // The mock never actually writes files, so the isolated copy is
    // unchanged too — but the important property is the real workspace:
    assert!(!dir.path().join("new.txt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("marker.txt")).unwrap(),
        "untouched"
    );
}

#[tokio::test]
async fn waiting_for_human_pauses_without_blocking_then_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({
            "status": "waiting_for_human",
            "summary": "Need a decision.",
            "question": "OAuth2 or JWT?",
            "options": [
                {"id": "oauth2", "label": "OAuth 2"},
                {"id": "jwt", "label": "JWT"}
            ],
            "completed": ["Analyzed the authentication module."],
            "recommended_option": "oauth2",
            "recommendation": "The repo already has an OAuth-compatible identity boundary."
        }),
        json!({
            "status": "completed",
            "summary": "Implemented using OAuth2.",
            "changed_files": []
        }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let paused = Implement
        .execute(request(dir.path()), context.clone())
        .await
        .unwrap();

    assert_eq!(paused.status, ExecutionStatus::WaitingForHuman);
    let question = paused.pending_question.as_ref().unwrap();
    assert_eq!(question.options.len(), 2);
    assert_eq!(question.recommended_option.as_deref(), Some("oauth2"));
    assert_eq!(
        question.completed,
        vec!["Analyzed the authentication module."]
    );
    assert_eq!(paused.gates.len(), 1);
    assert_eq!(paused.gates[0].id, question.id);
    // Paused mid-ACT — resuming must re-enter ACT, not restart from
    // UNDERSTAND (see `resuming_reenters_the_paused_phase_not_the_start`).
    assert_eq!(paused.workflow.phase(), ksforge::domain::Phase::Act);

    let registry = CapabilityRegistry::with_defaults();
    let resumed = ksforge::application::resume::resume(
        dir.path(),
        &paused.id,
        "oauth2".to_string(),
        Some("chevp".to_string()),
        &registry,
        context,
    )
    .await
    .unwrap();

    assert_eq!(resumed.status, ExecutionStatus::Completed);
    assert!(resumed.pending_question.is_none());
    assert!(resumed.messages.iter().any(
        |e| matches!(e, ksforge::domain::ExecutionEvent::HumanDecided { option, decided_by, .. }
            if option == "oauth2" && decided_by.as_deref() == Some("chevp"))
    ));
}

#[tokio::test]
async fn resuming_with_an_unoffered_option_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({
            "status": "waiting_for_human",
            "summary": "Need a decision.",
            "question": "OAuth2 or JWT?",
            "options": [{"id": "oauth2", "label": "OAuth 2"}]
        }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let paused = Implement
        .execute(request(dir.path()), context.clone())
        .await
        .unwrap();

    let registry = CapabilityRegistry::with_defaults();
    let err = ksforge::application::resume::resume(
        dir.path(),
        &paused.id,
        "saml".to_string(),
        None,
        &registry,
        context,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, ksforge::domain::KsforgeError::Usage(_)));
}

/// The core architectural invariant this suite exists to prove: UNDERSTAND
/// and LOCATE never get write tools, even for a write-capable capability
/// like `implement` — only ACT does, and only because `Implement`'s own
/// `tool_policy()` grants it. This is enforced by
/// `application::execute::run_phase_loop`, not by prompt instructions.
#[tokio::test]
async fn understand_and_locate_are_always_read_only_even_for_a_write_capability() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({ "status": "completed", "summary": "No changes needed.", "changed_files": [] }),
    ]));
    let captured = {
        let executor = executor.clone();
        executor.requests()
    };
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let execution = Implement
        .execute(request(dir.path()), context)
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);

    let requests = captured.lock().unwrap();
    assert_eq!(
        requests.len(),
        3,
        "one turn per phase: understand, locate, act"
    );

    let read_only_tools = vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()];
    // UNDERSTAND
    assert_eq!(
        requests[0].permission_mode,
        ksforge::agent::PermissionMode::ReadOnly
    );
    assert_eq!(requests[0].tools, read_only_tools);
    // LOCATE
    assert_eq!(
        requests[1].permission_mode,
        ksforge::agent::PermissionMode::ReadOnly
    );
    assert_eq!(requests[1].tools, read_only_tools);
    // ACT — `Implement::tool_policy()` is `ReadWrite`, only granted here.
    assert_eq!(
        requests[2].permission_mode,
        ksforge::agent::PermissionMode::AcceptEdits
    );
    assert!(requests[2].tools.is_empty());
}

/// A malformed/schema-non-conforming phase response fails the execution
/// rather than being coerced or silently accepted into the next
/// `WorkflowState` — and does so after exactly one call to the executor for
/// that phase, i.e. no automatic retry.
#[tokio::test]
async fn a_malformed_phase_response_fails_the_execution_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    // Missing the required `summary` field.
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![json!({
        "status": "completed"
    })]));
    let captured = executor.requests();
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let err = Implement.execute(request(dir.path()), context).await;
    assert!(err.is_err());
    assert_eq!(
        captured.lock().unwrap().len(),
        1,
        "no retry after a malformed response"
    );
}

/// A transient executor failure (e.g. a network blip or a rate limit,
/// keyword-sniffed by `AgentError::is_transient`) is retried within the
/// same phase turn instead of failing the execution outright.
#[tokio::test(start_paused = true)]
async fn a_transient_executor_error_is_retried_within_the_same_phase() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_results(vec![
        Err(AgentError::NonZeroExit(
            "529 Overloaded: please retry".into(),
        )),
        Ok(understand_ok()),
        Ok(locate_ok()),
        Ok(json!({ "status": "completed", "summary": "Added login.", "changed_files": [] })),
    ]));
    let captured = executor.requests();
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let execution = Implement
        .execute(request(dir.path()), context)
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
    assert_eq!(
        captured.lock().unwrap().len(),
        4,
        "one retried attempt for UNDERSTAND, plus one call each for LOCATE and ACT"
    );
}

/// A non-transient executor error (e.g. the CLI not being installed at
/// all) fails the execution after exactly one attempt — retrying an
/// unrecoverable, static failure would only waste time and (for a paid
/// engine) money.
#[tokio::test]
async fn a_non_transient_executor_error_fails_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_results(vec![Err(
        AgentError::NotFound("claude (not on PATH)".into()),
    )]));
    let captured = executor.requests();
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };

    let err = Implement.execute(request(dir.path()), context).await;
    assert!(err.is_err());
    assert_eq!(captured.lock().unwrap().len(), 1, "no retry attempt");
}

/// A failing `--validate` command fails the execution — VALIDATE runs
/// after ACT, deterministically, independent of what the agent reported.
#[tokio::test]
async fn a_failing_validate_command_fails_the_execution_after_act() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({ "status": "completed", "summary": "Added a file.", "changed_files": [] }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };
    let mut req = request(dir.path());
    req.validation = ValidationPolicy {
        commands: vec!["exit 1".to_string()],
        agent_review: false,
    };

    let execution = Implement.execute(req, context).await.unwrap();
    assert_eq!(execution.status, ExecutionStatus::Failed);
    // Failed at VALIDATE, not silently reset to the `Legacy` `#[serde(default)]`
    // placeholder — a persisted `state.json` should always show where the
    // run actually stopped.
    assert_eq!(execution.workflow.phase(), Phase::Validate);
    assert!(
        execution
            .messages
            .iter()
            .any(|e| matches!(e, ksforge::domain::ExecutionEvent::ValidationFailed { .. }))
    );
}

/// `ValidationPolicy::agent_review` runs a fourth turn (VALIDATE), after ACT
/// and before REPORT, with execute-only tool access — it can run Bash but,
/// per `understand_and_locate_are_always_read_only_even_for_a_write_capability`'s
/// sibling assertion below, is never handed Edit/Write.
#[tokio::test]
async fn agent_review_runs_a_validate_turn_with_execute_only_access() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({ "status": "completed", "summary": "Added a file.", "changed_files": [] }),
        json!({
            "status": "completed",
            "summary": "Rebuilt and confirmed the fix.",
            "completed": ["Ran the full build and confirmed the artifact is correct."],
            "open_items": ["A sibling module has the same latent issue."],
            "recommendation": "File a follow-up for the sibling module."
        }),
    ]));
    let captured = executor.requests();
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };
    let mut req = request(dir.path());
    req.validation = ValidationPolicy {
        commands: Vec::new(),
        agent_review: true,
    };

    let execution = Implement.execute(req, context).await.unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 4, "understand, locate, act, validate");
    assert_eq!(
        requests[3].permission_mode,
        ksforge::agent::PermissionMode::ExecuteOnly
    );
    assert!(requests[3].tools.contains(&"Bash".to_string()));
    assert!(!requests[3].tools.contains(&"Edit".to_string()));

    let review = execution
        .result
        .as_ref()
        .unwrap()
        .validation
        .agent_review
        .as_ref()
        .expect("agent_review populated");
    assert_eq!(review.summary, "Rebuilt and confirmed the fix.");
    assert_eq!(
        review.open_items,
        vec!["A sibling module has the same latent issue."]
    );
}

/// The VALIDATE turn's own judgment can fail the run — not just the
/// deterministic `--validate` commands.
#[tokio::test]
async fn agent_review_failure_fails_the_execution() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![
        understand_ok(),
        locate_ok(),
        json!({ "status": "completed", "summary": "Added a file.", "changed_files": [] }),
        json!({
            "status": "failed",
            "summary": "Validation failed.",
            "failure_reason": "The packaged installer still does not contain the fix."
        }),
    ]));
    let context = ExecutionContext {
        executor,
        workspace_root: dir.path().to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    };
    let mut req = request(dir.path());
    req.validation = ValidationPolicy {
        commands: Vec::new(),
        agent_review: true,
    };

    let execution = Implement.execute(req, context).await.unwrap();

    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(execution.workflow.phase(), Phase::Validate);
    assert_eq!(
        execution.result.as_ref().unwrap().summary,
        "The packaged installer still does not contain the fix."
    );
}
