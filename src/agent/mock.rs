use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use super::executor::{AgentError, AgentExecutor, AgentRequest, AgentResult};

/// A canned, deterministic stand-in for [`super::ClaudeCodeExecutor`], so
/// tests (and this crate's own integration tests) never spawn a real
/// `claude` process or spend real API budget (§Djb7BJR).
pub struct MockAgentExecutor {
    responses: Mutex<Vec<Value>>,
    requests: Arc<Mutex<Vec<AgentRequest>>>,
}

impl MockAgentExecutor {
    /// Responses are consumed in order, one per call to `execute`; each
    /// must already match [`crate::agent::outcome::AgentOutcome`]'s shape.
    pub fn with_responses(responses: Vec<Value>) -> Self {
        Self {
            responses: Mutex::new(responses),
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
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(AgentError::MalformedOutput(
                "MockAgentExecutor ran out of canned responses".into(),
            ));
        }
        let value = responses.remove(0);
        Ok(AgentResult {
            raw_text: value.to_string(),
            structured: Some(value),
            session_id: Some("mock-session".into()),
        })
    }
}
