use crate::capability::{ActionFailure, CapabilityId, CapabilityRequest};
use crate::intelligence::DiagnosticResult;
use crate::resource::ResourceId;

/// The outcome of the deterministic recovery evaluation (`recovery::decide`,
/// run by `Action::Recover`): either try again, or the retry budget (or a
/// permanent failure) says stop trying.
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    pub attempt: u32,
    pub exhausted: bool,
}

/// Everything that can happen. A failure is a variant here like any
/// other — data for the state machine to interpret, never a reason to
/// unwind. Events are facts/requests; nothing here mutates state by
/// itself.
#[derive(Debug, Clone)]
pub enum Event {
    StartRequested,
    ResourceAvailable(ResourceId),
    ResourceUnavailable(ResourceId),
    ExecuteRequested(CapabilityRequest),
    ActionCompleted(CapabilityId, String),
    ActionFailed(ActionFailure),
    RecoveryRequested(RecoveryContext),
    DiagnosticAvailable(DiagnosticResult),
    ShutdownRequested,
}
