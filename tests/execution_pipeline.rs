use std::sync::Arc;

use ksforge::agent::MockAgentExecutor;
use ksforge::application::implement::Implement;
use ksforge::domain::{
    Capability, CapabilityRegistry, ChangeRequest, ExecutionContext, ExecutionStatus,
    ImplementationRequest, ValidationPolicy,
};
use ksforge::workspace::ExecutionStore;
use serde_json::json;

fn request(workspace: &std::path::Path) -> ImplementationRequest {
    ImplementationRequest {
        change_request: ChangeRequest::from_text("As a user, I want to log in.").unwrap(),
        workspace: workspace.to_path_buf(),
        capability: "implement".to_string(),
        constraints: Vec::new(),
        validation: ValidationPolicy::default(),
    }
}

#[tokio::test]
async fn completed_run_persists_and_reports_success() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![json!({
        "status": "completed",
        "summary": "Added login.",
        "changed_files": []
    })]));
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

    // Persisted for `ksforge status` / a later `resume` to find.
    let store = ExecutionStore::new(dir.path());
    let reloaded = store.load(&execution.id).unwrap();
    assert_eq!(reloaded.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn dry_run_never_touches_the_real_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("marker.txt"), "untouched").unwrap();

    let executor = Arc::new(MockAgentExecutor::with_responses(vec![json!({
        "status": "completed",
        "summary": "Would have added a file.",
        "changed_files": ["new.txt"]
    })]));
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
    let executor = Arc::new(MockAgentExecutor::with_responses(vec![json!({
        "status": "waiting_for_human",
        "summary": "Need a decision.",
        "question": "OAuth2 or JWT?",
        "options": [{"id": "oauth2", "label": "OAuth 2"}]
    })]));
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
