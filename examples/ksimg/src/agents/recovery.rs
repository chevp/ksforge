use crate::techniques::TechniqueId;
use crate::workflow::ir::{StepId, WorkflowId};

/// Everything the `RecoveryAgent` gets to see about one failed step. It
/// has no access to the state machine or the registry — only this.
#[derive(Debug, Clone)]
pub struct FailureContext {
    pub workflow_id: WorkflowId,
    pub step_id: StepId,
    pub technique: TechniqueId,
    pub error: String,
    pub attempt: u32,
}

/// What the `RecoveryAgent` may suggest. It is a proposal, never an
/// instruction — the state machine decides whether and how to act on it.
/// See CLAUDE.md section 13.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryProposal {
    Retry,
    RetryWithParameters(String),
    AlternativeTechnique(TechniqueId),
    Replan,
    ContinueDegraded,
}

pub trait RecoveryAgent {
    fn diagnose(&self, failure: &FailureContext) -> RecoveryProposal;
}

/// Deterministic mock, in the same spirit as `kstest-core`'s
/// `RecoveryProcessor`: allow a bounded number of retries, then give up
/// and let the runtime continue in a degraded mode rather than stopping.
pub struct MockRecoveryAgent {
    pub max_retries: u32,
}

impl RecoveryAgent for MockRecoveryAgent {
    fn diagnose(&self, failure: &FailureContext) -> RecoveryProposal {
        if failure.attempt < self.max_retries {
            RecoveryProposal::Retry
        } else {
            RecoveryProposal::ContinueDegraded
        }
    }
}
