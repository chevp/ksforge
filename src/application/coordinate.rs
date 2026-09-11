use std::path::Path;
use std::sync::Arc;

use crate::agent::{AgentExecutor, AgentRequest, CoordinationDecision, PermissionMode};
use crate::domain::{ChangeRequest, Execution, ExecutionStatus, KsforgeError, Result};
use crate::workspace::ExecutionStore;

/// The coordination agent's Core system prompt — see
/// `prompts/coordinator/system-prompt.md` for the full role. Unlike a
/// capability's prompt (`application::prompt`), this has no policies/
/// fragments split: the coordinator doc is already self-contained, and
/// this command never touches the change-request-archive/PR machinery a capability
/// run does.
const SYSTEM_PROMPT: &str = include_str!("../../prompts/coordinator/system-prompt.md");

/// Reports how a new change request relates to the workspace's other
/// active executions, without implementing anything itself (the
/// coordinator's own §1: "You do not implement application changes
/// yourself"). Read-only, same tool policy as `review`/`explain`.
pub async fn coordinate(
    workspace_root: &Path,
    change_request: &ChangeRequest,
    executor: Arc<dyn AgentExecutor>,
    model: Option<String>,
    max_budget_usd: Option<f64>,
) -> Result<CoordinationDecision> {
    let active = active_executions(workspace_root)?;
    let user_prompt = build_user_prompt(change_request, &active, workspace_root);

    let agent_request = AgentRequest {
        prompt: user_prompt,
        system_prompt: Some(SYSTEM_PROMPT.to_string()),
        working_directory: workspace_root.to_path_buf(),
        tools: vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()],
        model,
        permission_mode: PermissionMode::ReadOnly,
        json_schema: Some(crate::agent::coordination::schema()),
        max_budget_usd,
        resume_session_id: None,
        mcp_config: None,
    };

    let agent_result = executor
        .execute(agent_request)
        .await
        .map_err(|e| KsforgeError::ExecutorFailed(format!("{e} (coordinate)")))?;

    crate::agent::coordination::parse(&agent_result)
        .map_err(|e| KsforgeError::ExecutorFailed(e.to_string()))
}

/// `Running`/`WaitingForHuman` — a `Completed`/`Failed`/`Cancelled`
/// execution is finished work, not something a new change request could
/// still collide with (see the coordinator's §11 for the separate case of
/// *completed but not yet integrated* work, which this simple filter does
/// not yet distinguish from truly finished work).
fn active_executions(workspace_root: &Path) -> Result<Vec<Execution>> {
    Ok(ExecutionStore::new(workspace_root)
        .list_all()?
        .into_iter()
        .filter(|e| {
            matches!(
                e.status,
                ExecutionStatus::Running | ExecutionStatus::WaitingForHuman
            )
        })
        .collect())
}

fn build_user_prompt(
    change_request: &ChangeRequest,
    active: &[Execution],
    workspace_root: &Path,
) -> String {
    let mut s = format!(
        "New change request:\n{}\n\nWorkspace: {}\n\n",
        change_request.text,
        workspace_root.display()
    );
    if active.is_empty() {
        s.push_str("Active executions: none.\n");
    } else {
        s.push_str("Active executions:\n");
        for execution in active {
            s.push_str(&format!(
                "- {} capability={} status={}\n  change request: {}\n",
                execution.id,
                execution.capability,
                execution.status,
                execution.change_request.text.lines().next().unwrap_or("")
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_no_active_executions_when_there_are_none() {
        let change_request = ChangeRequest::from_text("Add X.").unwrap();
        let prompt = build_user_prompt(&change_request, &[], Path::new("/workspace"));
        assert!(prompt.contains("New change request:\nAdd X."));
        assert!(prompt.contains("Active executions: none."));
    }

    #[test]
    fn lists_each_active_execution_with_its_own_change_request() {
        let change_request = ChangeRequest::from_text("Add X.").unwrap();
        let a = Execution::start(
            ChangeRequest::from_text("Refactor the resolver.").unwrap(),
            "fix",
        );
        let b = Execution::start(
            ChangeRequest::from_text("Add a remote cache.").unwrap(),
            "implement",
        );
        let prompt = build_user_prompt(
            &change_request,
            &[a.clone(), b.clone()],
            Path::new("/workspace"),
        );

        assert!(prompt.contains(&format!("- {} capability=fix status=running", a.id)));
        assert!(prompt.contains("change request: Refactor the resolver."));
        assert!(prompt.contains(&format!("- {} capability=implement status=running", b.id)));
        assert!(prompt.contains("change request: Add a remote cache."));
    }
}
