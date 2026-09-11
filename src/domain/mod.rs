//! Domain model: the vocabulary ksforge reasons in, independent of Claude
//! Code's process interface and of Git/GitHub. See docs/03-architecture.md.

pub mod capability;
pub mod change_request;
pub mod constraint;
pub mod error;
pub mod execution;
pub mod request;
pub mod test_scope;
pub mod workflow;

pub use capability::{Capability, CapabilityRegistry, ChangeScope, ExecutionContext, ToolPolicy};
pub use change_request::ChangeRequest;
pub use constraint::Constraint;
pub use error::{KsforgeError, Result};
pub use execution::{
    AgentValidationReport, DecisionOption, Execution, ExecutionEvent, ExecutionId, ExecutionResult,
    ExecutionStatus, GateId, HumanDecision, HumanDecisionRequest, ValidationCommandOutcome,
    ValidationOutcome,
};
pub use request::{ImplementationRequest, ValidationPolicy};
pub use workflow::{
    ActionKind, ActionResult, LocatedContext, Phase, TransitionError, Understanding, WorkflowState,
};
