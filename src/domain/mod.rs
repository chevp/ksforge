//! Domain model: the vocabulary ksforge reasons in, independent of Claude
//! Code's process interface and of Git/GitHub. See docs/03-architecture.md.

pub mod capability;
pub mod constraint;
pub mod error;
pub mod execution;
pub mod request;
pub mod story;

pub use capability::{Capability, CapabilityRegistry, ExecutionContext, ToolPolicy};
pub use constraint::Constraint;
pub use error::{KsforgeError, Result};
pub use execution::{
    DecisionOption, Execution, ExecutionEvent, ExecutionId, ExecutionResult, ExecutionStatus,
    GateId, HumanDecision, HumanDecisionRequest, ValidationCommandOutcome, ValidationOutcome,
};
pub use request::{ImplementationRequest, ValidationPolicy};
pub use story::UserStory;
