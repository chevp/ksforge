use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use super::executor::{AgentError, AgentExecutor, AgentRequest, AgentResult};

/// A canned, deterministic stand-in for [`super::ClaudeCodeExecutor`], so
/// tests (and this crate's own integration tests) never spawn a real
/// `claude` process or spend real API budget (§Djb7BJR).
pub struct MockAgentExecutor {
    results: Mutex<Vec<Result<Value, AgentError>>>,
    requests: Arc<Mutex<Vec<AgentRequest>>>,
}

impl MockAgentExecutor {
    /// Responses are consumed in order, one per call to `execute`; each
    /// must already match [`crate::agent::outcome::AgentOutcome`]'s shape.
    pub fn with_responses(responses: Vec<Value>) -> Self {
        Self::with_results(responses.into_iter().map(Ok).collect())
    }

    /// Like `with_responses`, but a call can also be scripted to fail with a
    /// given `AgentError` — e.g. a transient error before an eventual
    /// success, to exercise `execute::execute_with_retry`.
    pub fn with_results(results: Vec<Result<Value, AgentError>>) -> Self {
        Self {
            results: Mutex::new(results),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn requests(&self) -> Arc<Mutex<Vec<AgentRequest>>> {
        self.requests.clone()
    }
}

#[async_trait]
impl AgentExecutor for MockAgentExecutor {
    async fn execute(&self, request: AgentRequest) -> Result<AgentResult, AgentError> {
        self.requests.lock().unwrap().push(request);
        let mut results = self.results.lock().unwrap();
        if results.is_empty() {
            return Err(AgentError::MalformedOutput(
                "MockAgentExecutor ran out of canned responses".into(),
            ));
        }
        let value = results.remove(0)?;
        Ok(AgentResult {
            raw_text: value.to_string(),
            structured: Some(value),
            session_id: Some("mock-session".into()),
        })
    }
}
