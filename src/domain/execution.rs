use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::story::UserStory;

/// Identifier for a persisted [`Execution`]. Format: `ksf_<uuidv4 simple>`,
/// e.g. `ksf_2f8b6a1c9d3e4f5a8b7c6d5e4f3a2b1c`. Not a Git concept — purely a
/// ksforge-local run identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionId(pub String);

impl ExecutionId {
    pub fn new() -> Self {
        Self(format!("ksf_{}", Uuid::new_v4().simple()))
    }
}

impl Default for ExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ExecutionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    WaitingForHuman,
    Completed,
    Failed,
    Cancelled,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ExecutionStatus::Running => "running",
            ExecutionStatus::WaitingForHuman => "waiting_for_human",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            ExecutionStatus::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

/// A single decision option offered to the human.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    pub id: String,
    pub label: String,
}

/// Raised when Claude Code reports it needs a human decision to continue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecisionRequest {
    pub execution_id: ExecutionId,
    pub question: String,
    pub options: Vec<DecisionOption>,
    pub required: bool,
}

/// The human's answer to a [`HumanDecisionRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    pub execution_id: ExecutionId,
    pub option: String,
}

/// Append-only log of what happened during an execution. This is the single
/// source of truth for `ksforge status`; no parallel state machine exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ExecutionEvent {
    Started {
        capability: String,
        at: DateTime<Utc>,
    },
    PlanCreated {
        summary: String,
        at: DateTime<Utc>,
    },
    AgentStarted {
        at: DateTime<Utc>,
    },
    QuestionRaised {
        question: String,
        at: DateTime<Utc>,
    },
    HumanDecided {
        option: String,
        at: DateTime<Utc>,
    },
    AgentResumed {
        at: DateTime<Utc>,
    },
    ValidationStarted {
        at: DateTime<Utc>,
    },
    ValidationPassed {
        at: DateTime<Utc>,
    },
    ValidationFailed {
        detail: String,
        at: DateTime<Utc>,
    },
    Completed {
        summary: String,
        at: DateTime<Utc>,
    },
    Failed {
        detail: String,
        at: DateTime<Utc>,
    },
    Cancelled {
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationCommandOutcome {
    pub command: String,
    pub passed: bool,
    pub output_tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationOutcome {
    pub passed: bool,
    pub commands: Vec<ValidationCommandOutcome>,
}

/// Terminal payload of a completed or failed execution (section 22 of the
/// spec). Distinct from `Execution` itself: this is the small, stable
/// result snapshot; `Execution` is the durable envelope/state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub summary: String,
    pub changed_files: Vec<PathBuf>,
    pub validation: ValidationOutcome,
}

/// A running or paused unit of orchestration work: a user story being
/// carried out under a capability. Durable so it can cross a process
/// boundary (a GitHub Actions job ending mid-story) — see docs/06.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: ExecutionId,
    pub user_story: UserStory,
    pub capability: String,
    pub status: ExecutionStatus,
    pub current_step: String,
    pub artifacts: Vec<PathBuf>,
    pub messages: Vec<ExecutionEvent>,
    pub pending_question: Option<HumanDecisionRequest>,
    pub result: Option<ExecutionResult>,
    /// Claude Code session id (from the agent's result envelope), used to
    /// resume the same underlying conversation via `claude --resume` when
    /// available. `None` when the executor did not report one (e.g. a mock
    /// executor in tests) or resume-by-session is not applicable.
    pub agent_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Execution {
    pub fn start(user_story: UserStory, capability: impl Into<String>) -> Self {
        let now = Utc::now();
        let capability = capability.into();
        let mut execution = Self {
            id: ExecutionId::new(),
            user_story,
            capability: capability.clone(),
            status: ExecutionStatus::Running,
            current_step: "started".into(),
            artifacts: Vec::new(),
            messages: Vec::new(),
            pending_question: None,
            result: None,
            agent_session_id: None,
            created_at: now,
            updated_at: now,
        };
        execution.record(ExecutionEvent::Started {
            capability,
            at: now,
        });
        execution
    }

    pub fn record(&mut self, event: ExecutionEvent) {
        self.updated_at = Utc::now();
        self.messages.push(event);
    }

    pub fn ask(&mut self, question: String, options: Vec<DecisionOption>) -> HumanDecisionRequest {
        let request = HumanDecisionRequest {
            execution_id: self.id.clone(),
            question: question.clone(),
            options,
            required: true,
        };
        self.record(ExecutionEvent::QuestionRaised {
            question,
            at: Utc::now(),
        });
        self.status = ExecutionStatus::WaitingForHuman;
        self.current_step = "waiting_for_human".into();
        self.pending_question = Some(request.clone());
        request
    }

    pub fn apply_decision(&mut self, decision: &HumanDecision) {
        self.record(ExecutionEvent::HumanDecided {
            option: decision.option.clone(),
            at: Utc::now(),
        });
        self.pending_question = None;
        self.status = ExecutionStatus::Running;
        self.current_step = "resuming".into();
        self.record(ExecutionEvent::AgentResumed { at: Utc::now() });
    }

    pub fn complete(&mut self, result: ExecutionResult) {
        self.record(ExecutionEvent::Completed {
            summary: result.summary.clone(),
            at: Utc::now(),
        });
        self.artifacts = result.changed_files.clone();
        self.status = ExecutionStatus::Completed;
        self.current_step = "completed".into();
        self.result = Some(result);
    }

    pub fn fail(&mut self, detail: impl Into<String>) {
        let detail = detail.into();
        self.record(ExecutionEvent::Failed {
            detail: detail.clone(),
            at: Utc::now(),
        });
        self.status = ExecutionStatus::Failed;
        self.current_step = "failed".into();
        self.result = Some(ExecutionResult {
            success: false,
            summary: detail,
            changed_files: self.artifacts.clone(),
            validation: ValidationOutcome::default(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_human_round_trips_pending_question() {
        let story = UserStory::from_text("As a user...").unwrap();
        let mut exec = Execution::start(story, "implement");
        let q = exec.ask(
            "OAuth2 or JWT?".into(),
            vec![
                DecisionOption {
                    id: "oauth2".into(),
                    label: "OAuth 2".into(),
                },
                DecisionOption {
                    id: "jwt".into(),
                    label: "JWT".into(),
                },
            ],
        );
        assert_eq!(exec.status, ExecutionStatus::WaitingForHuman);
        assert!(exec.pending_question.is_some());
        assert_eq!(q.options.len(), 2);

        exec.apply_decision(&HumanDecision {
            execution_id: exec.id.clone(),
            option: "oauth2".into(),
        });
        assert_eq!(exec.status, ExecutionStatus::Running);
        assert!(exec.pending_question.is_none());
    }

    #[test]
    fn json_round_trip() {
        let story = UserStory::from_text("As a user...").unwrap();
        let exec = Execution::start(story, "implement");
        let json = serde_json::to_string(&exec).unwrap();
        let back: Execution = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, exec.id);
        assert_eq!(back.status, exec.status);
    }
}
