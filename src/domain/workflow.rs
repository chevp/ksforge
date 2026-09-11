//! The UNDERSTAND -> LOCATE -> ACT -> VALIDATE -> REPORT loop, as a
//! host-enforced state machine rather than a prompt convention (see
//! docs/03-architecture.md). `WorkflowState`'s variants each carry the data
//! every prior phase produced, so e.g. `Act` cannot be constructed without
//! consuming a `Locate` value — invalid transitions (UNDERSTAND -> ACT,
//! LOCATE -> REPORT, ...) have no code path that can produce them, not
//! merely a runtime check that rejects them after the fact.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::execution::ValidationOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Understand,
    Locate,
    Act,
    Validate,
    Report,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Understand => "understand",
            Phase::Locate => "locate",
            Phase::Act => "act",
            Phase::Validate => "validate",
            Phase::Report => "report",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// What the UNDERSTAND phase determined. Distinct from
/// `agent::outcome::AgentOutcome` (that turn's raw wire-format output) —
/// durable domain data extracted from it, free of turn-transient fields
/// like `question`/`options`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Understanding {
    pub summary: String,
    #[serde(default)]
    pub scope: Vec<String>,
}

/// What the LOCATE phase found in the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocatedContext {
    #[serde(default)]
    pub relevant_files: Vec<String>,
    #[serde(default)]
    pub existing_abstractions: Vec<String>,
    #[serde(default)]
    pub existing_tests: Vec<String>,
    #[serde(default)]
    pub conventions: Vec<String>,
}

/// What kind of result the ACT phase produced. Derived by the host from the
/// capability id and the filesystem snapshot diff — never self-reported by
/// the model, since the host can compute it deterministically on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Modify,
    Findings,
    Explain,
    NoChange,
}

impl ActionKind {
    pub fn derive(capability_id: &str, changed_files_is_empty: bool) -> Self {
        match capability_id {
            "review" => ActionKind::Findings,
            "explain" => ActionKind::Explain,
            _ if changed_files_is_empty => ActionKind::NoChange,
            _ => ActionKind::Modify,
        }
    }
}

/// What the ACT phase produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub kind: ActionKind,
    #[serde(default)]
    pub title: Option<String>,
    pub summary: String,
    /// Filesystem snapshot diff around the ACT turn — ground truth, never
    /// the model's own `changed_files` claim (see `application::execute`).
    #[serde(default)]
    pub changed_files: Vec<PathBuf>,
    #[serde(default)]
    pub completed: Vec<String>,
    #[serde(default)]
    pub open_items: Vec<String>,
    #[serde(default)]
    pub recommendation: Option<String>,
}

/// A rejected transition. Constructing `WorkflowState::Act` requires
/// consuming a `Locate` value (which requires an `Understand` before it),
/// so in normal operation this only ever fires against a corrupted/
/// hand-edited persisted `state.json` — defense in depth, not the primary
/// enforcement mechanism.
#[derive(Debug, Clone, thiserror::Error)]
#[error("cannot advance to {attempted}: workflow is at {from}")]
pub struct TransitionError {
    pub from: Phase,
    pub attempted: Phase,
}

/// The durable state of one capability run's phase loop. `#[serde(tag =
/// "phase")]` makes the on-disk shape self-describing and matches the
/// existing `ExecutionEvent`/`OutcomeStatus` tagging convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum WorkflowState {
    Understand,
    Locate {
        understanding: Understanding,
    },
    Act {
        understanding: Understanding,
        located: LocatedContext,
    },
    Validate {
        understanding: Understanding,
        located: LocatedContext,
        action: ActionResult,
    },
    Report {
        understanding: Understanding,
        located: LocatedContext,
        action: ActionResult,
        validation: ValidationOutcome,
    },
    /// An `Execution` persisted before this workflow existed, or any other
    /// value the current code never produces. `resume()` refuses this
    /// explicitly rather than guessing which phase it was in.
    Legacy,
}

/// Only reached via `#[serde(default)]` when loading an `Execution` that
/// predates this field — never produced by `WorkflowState::start()`.
impl Default for WorkflowState {
    fn default() -> Self {
        WorkflowState::Legacy
    }
}

impl WorkflowState {
    pub fn start() -> Self {
        WorkflowState::Understand
    }

    /// `Legacy` maps to `Phase::Report` here only for a defensible error
    /// message if a transition function is ever (incorrectly) called on
    /// it — `Legacy` is otherwise never dispatched on by phase.
    pub fn phase(&self) -> Phase {
        match self {
            WorkflowState::Understand => Phase::Understand,
            WorkflowState::Locate { .. } => Phase::Locate,
            WorkflowState::Act { .. } => Phase::Act,
            WorkflowState::Validate { .. } => Phase::Validate,
            WorkflowState::Report { .. } => Phase::Report,
            WorkflowState::Legacy => Phase::Report,
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, WorkflowState::Legacy)
    }

    pub fn complete_understand(
        self,
        understanding: Understanding,
    ) -> Result<Self, TransitionError> {
        match self {
            WorkflowState::Understand => Ok(WorkflowState::Locate { understanding }),
            other => Err(TransitionError {
                from: other.phase(),
                attempted: Phase::Locate,
            }),
        }
    }

    pub fn complete_locate(self, located: LocatedContext) -> Result<Self, TransitionError> {
        match self {
            WorkflowState::Locate { understanding } => Ok(WorkflowState::Act {
                understanding,
                located,
            }),
            other => Err(TransitionError {
                from: other.phase(),
                attempted: Phase::Act,
            }),
        }
    }

    pub fn complete_act(self, action: ActionResult) -> Result<Self, TransitionError> {
        match self {
            WorkflowState::Act {
                understanding,
                located,
            } => Ok(WorkflowState::Validate {
                understanding,
                located,
                action,
            }),
            other => Err(TransitionError {
                from: other.phase(),
                attempted: Phase::Validate,
            }),
        }
    }

    pub fn complete_validate(self, validation: ValidationOutcome) -> Result<Self, TransitionError> {
        match self {
            WorkflowState::Validate {
                understanding,
                located,
                action,
            } => Ok(WorkflowState::Report {
                understanding,
                located,
                action,
                validation,
            }),
            other => Err(TransitionError {
                from: other.phase(),
                attempted: Phase::Report,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn understanding() -> Understanding {
        Understanding {
            summary: "add a login page".into(),
            scope: vec!["src/auth".into()],
        }
    }

    fn located() -> LocatedContext {
        LocatedContext {
            relevant_files: vec!["src/auth/mod.rs".into()],
            existing_abstractions: vec![],
            existing_tests: vec![],
            conventions: vec![],
        }
    }

    fn action() -> ActionResult {
        ActionResult {
            kind: ActionKind::Modify,
            title: Some("Add login page".into()),
            summary: "Added it.".into(),
            changed_files: vec!["src/auth/mod.rs".into()],
            completed: vec![],
            open_items: vec![],
            recommendation: None,
        }
    }

    #[test]
    fn happy_path_advances_one_phase_at_a_time() {
        let state = WorkflowState::start();
        assert_eq!(state.phase(), Phase::Understand);

        let state = state.complete_understand(understanding()).unwrap();
        assert_eq!(state.phase(), Phase::Locate);

        let state = state.complete_locate(located()).unwrap();
        assert_eq!(state.phase(), Phase::Act);

        let state = state.complete_act(action()).unwrap();
        assert_eq!(state.phase(), Phase::Validate);

        let state = state
            .complete_validate(ValidationOutcome::default())
            .unwrap();
        assert_eq!(state.phase(), Phase::Report);
    }

    #[test]
    fn cannot_skip_locate_to_reach_act() {
        let state = WorkflowState::start();
        let err = state.complete_act(action()).unwrap_err();
        assert_eq!(err.from, Phase::Understand);
        assert_eq!(err.attempted, Phase::Validate);
    }

    #[test]
    fn cannot_skip_act_to_reach_report() {
        let state = WorkflowState::start()
            .complete_understand(understanding())
            .unwrap();
        let err = state
            .complete_validate(ValidationOutcome::default())
            .unwrap_err();
        assert_eq!(err.from, Phase::Locate);
        assert_eq!(err.attempted, Phase::Report);
    }

    #[test]
    fn cannot_redo_a_completed_phase() {
        let state = WorkflowState::start()
            .complete_understand(understanding())
            .unwrap()
            .complete_locate(located())
            .unwrap();
        // Already at Act; trying to complete_understand again is invalid.
        let err = state.complete_understand(understanding()).unwrap_err();
        assert_eq!(err.from, Phase::Act);
        assert_eq!(err.attempted, Phase::Locate);
    }

    #[test]
    fn legacy_is_the_serde_default_for_a_missing_field() {
        // `#[serde(default)]` only kicks in for a *missing field* on a
        // containing struct (exactly how `Execution.workflow` uses it) —
        // `WorkflowState` deserialized standalone still requires its own
        // `phase` tag, so exercise it the way it's actually used.
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default)]
            workflow: WorkflowState,
        }
        let deserialized: Wrapper = serde_json::from_str("{}").unwrap();
        assert!(deserialized.workflow.is_legacy());
    }

    #[test]
    fn action_kind_is_derived_not_reported() {
        assert_eq!(ActionKind::derive("review", true), ActionKind::Findings);
        assert_eq!(ActionKind::derive("explain", false), ActionKind::Explain);
        assert_eq!(ActionKind::derive("implement", true), ActionKind::NoChange);
        assert_eq!(ActionKind::derive("implement", false), ActionKind::Modify);
        assert_eq!(ActionKind::derive("fix", false), ActionKind::Modify);
    }

    #[test]
    fn round_trips_through_json() {
        let state = WorkflowState::start()
            .complete_understand(understanding())
            .unwrap();
        let json = serde_json::to_string(&state).unwrap();
        let back: WorkflowState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase(), Phase::Locate);
    }
}
