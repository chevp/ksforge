//! Domain model: the vocabulary ksforge reasons in, independent of Claude
//! Code's process interface and of Git/GitHub. See docs/03-architecture.md.

pub mod capability;
pub mod change_request;
pub mod constraint;
pub mod error;
pub mod execution;
pub mod request;

pub use capability::{Capability, CapabilityRegistry, ExecutionContext, ToolPolicy};
pub use change_request::ChangeRequest;
pub use constraint::Constraint;
pub use error::{KsforgeError, Result};
pub use execution::{
    DecisionOption, Execution, ExecutionEvent, ExecutionId, ExecutionResult, ExecutionStatus,
    GateId, HumanDecision, HumanDecisionRequest, ValidationCommandOutcome, ValidationOutcome,
};
pub use request::{ImplementationRequest, ValidationPolicy};
