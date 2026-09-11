use crate::context::{CorrelationId, ExecutionId};
use crate::state::OperationalState;

/// One recorded state change, for whatever is watching the runtime from
/// outside — logs today, an intelligence layer's diagnostics later. Kept
/// as a plain, small record rather than a full event-sourcing/metrics
/// system: this is what a small core needs, not what a platform needs.
#[derive(Debug, Clone)]
pub struct StateTransitionRecord {
    pub execution_id: ExecutionId,
    pub correlation_id: CorrelationId,
    pub from: OperationalState,
    pub to: OperationalState,
    pub reason: &'static str,
}
