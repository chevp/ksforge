use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ksforge::agent::{
    AgentError, AgentExecutor, AgentRequest, AgentResult, MockAgentExecutor, PermissionMode,
};
use ksforge::application::test::Test;
use ksforge::domain::{
    Capability, ChangeRequest, ExecutionContext, ExecutionStatus, ImplementationRequest,
    ValidationPolicy,
};
use serde_json::{Value, json};

fn request(workspace: &std::path::Path) -> ImplementationRequest {
    ImplementationRequest {
        change_request: ChangeRequest::from_text("Add a test for the add function.").unwrap(),
        workspace: workspace.to_path_buf(),
        capability: "test".to_string(),
        constraints: Vec::new(),
        validation: ValidationPolicy::default(),
    }
}

fn understand_ok() -> Value {
    json!({ "status": "completed", "summary": "Needs a test for add()." })
}

fn locate_ok() -> Value {
    json!({ "status": "completed", "summary": "Found lib.rs." })
}

/// Wraps `MockAgentExecutor`, writing one file to the real working
/// directory the first time it sees a write-capable (ACT-phase) call —
/// simulating what a real agent would have done, since the mock itself
/// never touches disk. Needed because `ChangeScope::TestsOnly` is checked
/// against the real filesystem diff, not the mock's claimed
/// `changed_files`.
struct WritingExecutor {
    inner: MockAgentExecutor,
    pending_write: Mutex<Option<(PathBuf, String)>>,
}

#[async_trait]
impl AgentExecutor for WritingExecutor {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError> {
        if request.permission_mode == PermissionMode::AcceptEdits
            && let Some((path, content)) = self.pending_write.lock().unwrap().take()
        {
            let full = request.working_directory.join(&path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, content).unwrap();
        }
        self.inner.execute(request).await
    }
}

fn context_with(executor: WritingExecutor, workspace: &std::path::Path) -> ExecutionContext {
    ExecutionContext {
        executor: Arc::new(executor),
        workspace_root: workspace.to_path_buf(),
        model: None,
        max_budget_usd: None,
        dry_run: false,
        mcp_config: None,
    }
}

#[tokio::test]
async fn touching_a_non_test_file_fails_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let executor = WritingExecutor {
        inner: MockAgentExecutor::with_responses(vec![
            understand_ok(),
            locate_ok(),
            json!({
                "status": "completed",
                "summary": "Fixed the bug.",
                "changed_files": ["src/lib.rs"]
            }),
        ]),
        pending_write: Mutex::new(Some((
            PathBuf::from("src/lib.rs"),
            // A "real" change, not a test — the agent ignored its scope.
            "pub fn add(a: i32, b: i32) -> i32 { a + b + 1 }\n".to_string(),
        ))),
    };

    let execution = Test
        .execute(request(dir.path()), context_with(executor, dir.path()))
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert!(
        execution
            .result
            .as_ref()
            .unwrap()
            .summary
            .contains("lib.rs"),
        "failure summary should name the offending file: {:?}",
        execution.result
    );
}

#[tokio::test]
async fn adding_a_dedicated_test_file_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let executor = WritingExecutor {
        inner: MockAgentExecutor::with_responses(vec![
            understand_ok(),
            locate_ok(),
            json!({
                "status": "completed",
                "summary": "Added a test.",
                "changed_files": ["tests/add_test.rs"]
            }),
        ]),
        pending_write: Mutex::new(Some((
            PathBuf::from("tests/add_test.rs"),
            "#[test]\nfn adds() { assert_eq!(1 + 1, 2); }\n".to_string(),
        ))),
    };

    let execution = Test
        .execute(request(dir.path()), context_with(executor, dir.path()))
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn adding_a_colocated_rust_test_block_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    // Pre-existing source file with no tests yet.
    std::fs::write(
        dir.path().join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();
    let executor = WritingExecutor {
        inner: MockAgentExecutor::with_responses(vec![
            understand_ok(),
            locate_ok(),
            json!({
                "status": "completed",
                "summary": "Added an inline test.",
                "changed_files": ["lib.rs"]
            }),
        ]),
        pending_write: Mutex::new(Some((
            PathBuf::from("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n\
             #[cfg(test)]\nmod tests {\n    use super::*;\n    \
             #[test]\n    fn adds() { assert_eq!(add(1, 1), 2); }\n}\n"
                .to_string(),
        ))),
    };

    let execution = Test
        .execute(request(dir.path()), context_with(executor, dir.path()))
        .await
        .unwrap();

    assert_eq!(execution.status, ExecutionStatus::Completed);
}
