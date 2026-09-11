use crate::capability::CapabilityRequest;
use crate::intelligence::AnalysisContext;
use crate::resource::ResourceId;
use crate::scheduler::WorkClass;

/// The side effects a transition can request. The state machine returns
/// these rather than performing them, so it stays a pure decision point —
/// see `StateMachine::handle`. None of these carry any domain concept.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Execute(CapabilityRequest),
    Retry(CapabilityRequest),
    Reconnect(ResourceId),
    /// Evaluate the retry policy against the last failure and report back
    /// as an event — the deterministic recovery step described in
    /// `recovery::decide`. Not a capability: this never leaves the
    /// runtime/state-machine layer.
    Recover,
    RequestDiagnostic(AnalysisContext),
    Shutdown,
}

impl Action {
    /// Which lane this action's work belongs in. Only `RequestDiagnostic`
    /// is diagnostic; everything else is realtime and runs inline.
    pub fn work_class(&self) -> WorkClass {
        match self {
            Action::RequestDiagnostic(_) => WorkClass::Diagnostic,
            _ => WorkClass::Realtime,
        }
    }
}
