#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityId(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequest {
    pub attempt: u32,
    pub payload: String,
}

/// The known classes of failure the core can reason about without any
/// domain knowledge — see `recovery::decide`. A capability classifies its
/// own failures; the core never guesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Transient,
    ResourceUnavailable,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionFailure {
    pub reason: String,
    pub kind: FailureKind,
}

#[derive(Debug, Clone)]
pub enum CapabilityResult {
    Success(String),
    Failure(ActionFailure),
}

/// A concrete piece of work the runtime can execute. A capability knows its
/// own domain — the core never does — but it does not know the runtime's
/// operational state, does not decide whether to retry, and cannot stop
/// the runtime. It only ever reports what happened.
pub trait Capability {
    fn id(&self) -> CapabilityId;
    fn execute(&self, request: CapabilityRequest) -> CapabilityResult;
}
