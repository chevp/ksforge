use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::change_request::ChangeRequest;

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

/// Identifier for a [`HumanDecisionRequest`] ("gate"), stable across the
/// gate's lifetime so a GitHub comment reply or a `resume` call can address
/// it unambiguously — the human-readable `question`/label text is never the
/// only identity of a gate. Format: `gate_<uuidv4 simple>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GateId(pub String);

impl GateId {
    pub fn new() -> Self {
        Self(format!("gate_{}", Uuid::new_v4().simple()))
    }
}

impl Default for GateId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for GateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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

/// Raised when Claude Code reports it needs a human decision to continue —
/// a "gate" in spec terms. Addressed by its own [`GateId`], not by the
/// execution id or the question text (section 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecisionRequest {
    #[serde(default)]
    pub id: GateId,
    pub execution_id: ExecutionId,
    pub question: String,
    pub options: Vec<DecisionOption>,
    pub required: bool,
    /// Option id Claude Code recommends, when it could form a defensible
    /// preference from repository evidence (section 12). `None` means no
    /// recommendation was offered, not that one was omitted by accident.
    #[serde(default)]
    pub recommended_option: Option<String>,
    /// Free-text rationale behind the question/recommendation. Empty when
    /// the agent gave none.
    #[serde(default)]
    pub context: String,
    /// What was already done before this gate was raised (section 14: the
    /// "Completed" list on a waiting-for-human report).
    #[serde(default)]
    pub completed: Vec<String>,
}

/// The human's answer to a [`HumanDecisionRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    pub execution_id: ExecutionId,
    pub option: String,
    /// Who decided: a GitHub login when resumed via `/ksforge choose` (see
    /// `github::decision`), `None` for a local CLI `resume` with no
    /// `--decided-by`.
    #[serde(default)]
    pub decided_by: Option<String>,
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
        #[serde(default)]
        decided_by: Option<String>,
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
    /// Short, standalone headline from the agent, for the pull request
    /// title/commit subject — see `AgentOutcome::title`. `None` on
    /// failure/cancellation, or when the agent omitted it.
    #[serde(default)]
    pub title: Option<String>,
    pub summary: String,
    pub changed_files: Vec<PathBuf>,
    pub validation: ValidationOutcome,
    /// Work Claude Code reported as done this run (section 10/19). Empty
    /// when the agent didn't report any — not synthesized by ksforge.
    #[serde(default)]
    pub completed: Vec<String>,
    /// Work that remains and why, when the agent reported some even while
    /// completing successfully (e.g. follow-ups it flagged but didn't do).
    #[serde(default)]
    pub open_items: Vec<String>,
    /// Free-text next-step recommendation, when the agent gave one.
    #[serde(default)]
    pub recommendation: Option<String>,
}

/// A running or paused unit of orchestration work: a change request being
/// carried out under a capability. Durable so it can cross a process
/// boundary (a GitHub Actions job ending mid-change-request) — see docs/06.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: ExecutionId,
    pub change_request: ChangeRequest,
    pub capability: String,
    pub status: ExecutionStatus,
    pub current_step: String,
    pub artifacts: Vec<PathBuf>,
    pub messages: Vec<ExecutionEvent>,
    pub pending_question: Option<HumanDecisionRequest>,
    /// Every gate ever raised on this execution, in order — the durable
    /// history spec §18/19 need for a multi-gate final report and for
    /// telling "already resolved" apart from "unknown gate" (section 6/17).
    /// `pending_question`, when `Some`, is always `gates.last()`.
    #[serde(default)]
    pub gates: Vec<HumanDecisionRequest>,
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
    pub fn start(change_request: ChangeRequest, capability: impl Into<String>) -> Self {
        let now = Utc::now();
        let capability = capability.into();
        let mut execution = Self {
            id: ExecutionId::new(),
            change_request,
            capability: capability.clone(),
            status: ExecutionStatus::Running,
            current_step: "started".into(),
            artifacts: Vec::new(),
            messages: Vec::new(),
            pending_question: None,
            gates: Vec::new(),
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

    pub fn ask(
        &mut self,
        question: String,
        options: Vec<DecisionOption>,
        recommended_option: Option<String>,
        context: String,
        completed: Vec<String>,
    ) -> HumanDecisionRequest {
        let request = HumanDecisionRequest {
            id: GateId::new(),
            execution_id: self.id.clone(),
            question: question.clone(),
            options,
            required: true,
            recommended_option,
            context,
            completed,
        };
        self.record(ExecutionEvent::QuestionRaised {
            question,
            at: Utc::now(),
        });
        self.status = ExecutionStatus::WaitingForHuman;
        self.current_step = "waiting_for_human".into();
        self.pending_question = Some(request.clone());
        self.gates.push(request.clone());
        request
    }

    /// The gate currently awaiting a decision, if any — `pending_question`
    /// under its "gate" name (section 6/16: a comment-driven decision
    /// addresses a `GateId`, not just "the execution").
    pub fn open_gate(&self) -> Option<&HumanDecisionRequest> {
        self.pending_question.as_ref()
    }

    pub fn apply_decision(&mut self, decision: &HumanDecision) {
        self.record(ExecutionEvent::HumanDecided {
            option: decision.option.clone(),
            decided_by: decision.decided_by.clone(),
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
            title: None,
            summary: detail,
            changed_files: self.artifacts.clone(),
            validation: ValidationOutcome::default(),
            completed: Vec::new(),
            open_items: Vec::new(),
            recommendation: None,
        });
    }

    /// Deliberately stop an execution that is not going to be resumed —
    /// distinct from `fail`: cancellation is a human/operator choice, not
    /// an error the agent or validation reported (spec §4/20).
    pub fn cancel(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.pending_question = None;
        self.status = ExecutionStatus::Cancelled;
        self.current_step = "cancelled".into();
        self.result = Some(ExecutionResult {
            success: false,
            title: None,
            summary: reason,
            changed_files: self.artifacts.clone(),
            validation: ValidationOutcome::default(),
            completed: Vec::new(),
            open_items: Vec::new(),
            recommendation: None,
        });
        self.record(ExecutionEvent::Cancelled { at: Utc::now() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_human_round_trips_pending_question() {
        let change_request = ChangeRequest::from_text("As a user...").unwrap();
        let mut exec = Execution::start(change_request, "implement");
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
            Some("oauth2".into()),
            "The repo already has an OAuth-compatible identity boundary.".into(),
            vec!["Analyzed the authentication module.".into()],
        );
        assert_eq!(exec.status, ExecutionStatus::WaitingForHuman);
        assert!(exec.pending_question.is_some());
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.recommended_option.as_deref(), Some("oauth2"));
        assert_eq!(exec.gates.len(), 1);
        assert_eq!(exec.gates[0].id, q.id);

        exec.apply_decision(&HumanDecision {
            execution_id: exec.id.clone(),
            option: "oauth2".into(),
            decided_by: Some("chevp".into()),
        });
        assert_eq!(exec.status, ExecutionStatus::Running);
        assert!(exec.pending_question.is_none());
        assert_eq!(exec.gates.len(), 1, "gate history is append-only");
    }

    #[test]
    fn multiple_sequential_gates_are_kept_in_history() {
        let change_request = ChangeRequest::from_text("As a user...").unwrap();
        let mut exec = Execution::start(change_request, "implement");

        exec.ask(
            "OAuth2 or JWT?".into(),
            vec![DecisionOption {
                id: "oauth2".into(),
                label: "OAuth 2".into(),
            }],
            None,
            String::new(),
            Vec::new(),
        );
        exec.apply_decision(&HumanDecision {
            execution_id: exec.id.clone(),
            option: "oauth2".into(),
            decided_by: None,
        });

        exec.ask(
            "Which session store?".into(),
            vec![DecisionOption {
                id: "redis".into(),
                label: "Redis".into(),
            }],
            None,
            String::new(),
            Vec::new(),
        );
        assert_eq!(exec.gates.len(), 2, "each ask() appends a new gate");
        assert_ne!(exec.gates[0].id, exec.gates[1].id);
    }

    #[test]
    fn cancel_stops_a_waiting_execution() {
        let change_request = ChangeRequest::from_text("As a user...").unwrap();
        let mut exec = Execution::start(change_request, "implement");
        exec.ask(
            "OAuth2 or JWT?".into(),
            vec![DecisionOption {
                id: "oauth2".into(),
                label: "OAuth 2".into(),
            }],
            None,
            String::new(),
            Vec::new(),
        );

        exec.cancel("no longer needed");
        assert_eq!(exec.status, ExecutionStatus::Cancelled);
        assert!(exec.pending_question.is_none());
        assert_eq!(exec.result.as_ref().unwrap().summary, "no longer needed");
    }

    #[test]
    fn json_round_trip() {
        let change_request = ChangeRequest::from_text("As a user...").unwrap();
        let exec = Execution::start(change_request, "implement");
        let json = serde_json::to_string(&exec).unwrap();
        let back: Execution = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, exec.id);
        assert_eq!(back.status, exec.status);
    }
}
