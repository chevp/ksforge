use crate::action::Action;
use crate::lifecycle::ExecutionLifecycle;

/// The runtime's business/operational condition — distinct from
/// `ExecutionLifecycle`. A run can be `Running` while operationally
/// `Degraded`; that is expected, not an error. There is deliberately no
/// generic `Failed` variant: every operational failure lands in a state
/// that means something specific (`Recovering`, `Degraded`), never a
/// catch-all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalState {
    Starting,
    Connecting,
    Ready,
    Processing,
    Recovering,
    Degraded,
}

impl OperationalState {
    /// A stable state is one the runtime is content to sit in indefinitely
    /// between events.
    pub fn is_stable(self) -> bool {
        matches!(self, OperationalState::Ready)
    }
}

/// The record of one state-machine decision. `lifecycle_from`/`_to` are
/// almost always equal — they only differ on the one event
/// (`ShutdownRequested`) that ends the run.
#[derive(Debug)]
pub struct Transition {
    pub operational_from: OperationalState,
    pub operational_to: OperationalState,
    pub lifecycle_from: ExecutionLifecycle,
    pub lifecycle_to: ExecutionLifecycle,
    pub actions: Vec<Action>,
    pub reason: &'static str,
}
