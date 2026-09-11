use crate::capability::ActionFailure;
use crate::context::{CorrelationId, ExecutionId};

/// What the optional intelligence layer is given to work with. Plain,
/// owned data — nothing here is a handle to runtime state, so nothing
/// implementing `Intelligence` can reach in and mutate it even if it
/// wanted to.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisContext {
    pub execution_id: ExecutionId,
    pub correlation_id: CorrelationId,
    pub attempt: u32,
    pub last_failure: Option<ActionFailure>,
}

/// What intelligence may suggest. Never a command — the state machine
/// decides whether to accept it (see `state_machine::accept_proposal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proposal {
    Retry,
    Degrade,
    NoAction,
}

#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    pub observation: String,
    pub proposal: Proposal,
}

/// Optional advisory layer: diagnostics, planning, debugging, optimization,
/// commissioning assistance, recovery suggestions — an LLM is one possible
/// implementation, never a required one. `analyze` only ever returns data;
/// it has no way to change what the runtime is doing.
pub trait Intelligence: Send + Sync {
    fn analyze(&self, context: AnalysisContext) -> DiagnosticResult;
}

/// Used when no `Intelligence` is registered (see `Scheduler`). Keeps the
/// runtime fully operational with `intelligence = None`: an unrecoverable
/// failure still gets an answer, just a conservative one, and immediately
/// rather than after a round trip to nowhere.
pub fn deterministic_fallback(context: &AnalysisContext) -> DiagnosticResult {
    DiagnosticResult {
        observation: format!(
            "no intelligence provider registered; falling back after {} attempt(s)",
            context.attempt
        ),
        proposal: Proposal::NoAction,
    }
}
