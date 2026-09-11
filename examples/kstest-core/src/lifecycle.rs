/// The runtime process's own lifecycle — separate from `OperationalState`
/// (see `state.rs`). A run can be `Running` while its operational state is
/// `Recovering` or `Degraded`: those are business-level conditions, not
/// reasons for the process itself to stop. Only `ShutdownRequested` moves
/// this past `Running`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLifecycle {
    Created,
    Running,
    Stopping,
    Stopped,
}
