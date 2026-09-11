use crate::action::Action;
use crate::capability::{ActionFailure, CapabilityRequest};
use crate::context::ExecutionContext;
use crate::event::{Event, RecoveryContext};
use crate::intelligence::{AnalysisContext, Proposal};
use crate::lifecycle::ExecutionLifecycle;
use crate::resource::ResourceId;
use crate::state::{OperationalState, Transition};

/// Everything the state machine needs to remember between events, kept as
/// explicit fields rather than hidden inside a capability — a test can
/// read `attempt` directly instead of inferring it from log output.
#[derive(Default)]
struct Context {
    attempt: u32,
    in_flight: Option<String>,
    last_failure: Option<ActionFailure>,
}

/// The runtime's sole source of control. No capability, resource,
/// scheduler, or intelligence provider may reach in and change operational
/// state or lifecycle directly — everything that wants to affect either
/// has to go through `handle` as an `Event`.
pub struct StateMachine {
    operational: OperationalState,
    lifecycle: ExecutionLifecycle,
    context: Context,
    execution_context: ExecutionContext,
    resource_id: ResourceId,
}

impl StateMachine {
    pub fn new(execution_context: ExecutionContext, resource_id: ResourceId) -> Self {
        Self {
            operational: OperationalState::Starting,
            lifecycle: ExecutionLifecycle::Created,
            context: Context::default(),
            execution_context,
            resource_id,
        }
    }

    pub fn operational_state(&self) -> OperationalState {
        self.operational
    }

    pub fn lifecycle(&self) -> ExecutionLifecycle {
        self.lifecycle
    }

    pub fn attempt(&self) -> u32 {
        self.context.attempt
    }

    pub fn last_failure(&self) -> Option<&ActionFailure> {
        self.context.last_failure.as_ref()
    }

    pub fn execution_id(&self) -> &crate::context::ExecutionId {
        &self.execution_context.execution_id
    }

    pub fn correlation_id(&self) -> &crate::context::CorrelationId {
        &self.execution_context.correlation_id
    }

    /// Interpret one event against the current state and return what
    /// changed. This is the only method in the crate that assigns to
    /// `self.operational` or `self.lifecycle`.
    pub fn handle(&mut self, event: Event) -> Transition {
        let operational_from = self.operational;
        let lifecycle_from = self.lifecycle;
        let (operational_to, actions, reason) = self.next(event);
        self.operational = operational_to;
        Transition {
            operational_from,
            operational_to,
            lifecycle_from,
            lifecycle_to: self.lifecycle,
            actions,
            reason,
        }
    }

    /// The one narrow, deliberate exception to "only `handle` mutates
    /// state": called by the runtime once the `Shutdown` action it was
    /// given has actually finished tearing resources down. It only ever
    /// moves `Stopping -> Stopped`, unconditionally — there is no decision
    /// left to make at that point, so routing it back through a synthetic
    /// event would be ceremony without substance.
    pub fn complete_shutdown(&mut self) {
        debug_assert_eq!(self.lifecycle, ExecutionLifecycle::Stopping);
        self.lifecycle = ExecutionLifecycle::Stopped;
    }

    fn next(&mut self, event: Event) -> (OperationalState, Vec<Action>, &'static str) {
        use Event::*;
        use OperationalState::*;

        if let ShutdownRequested = event {
            self.lifecycle = ExecutionLifecycle::Stopping;
            return (
                self.operational,
                vec![Action::Shutdown],
                "ShutdownRequested",
            );
        }
        if self.lifecycle == ExecutionLifecycle::Created {
            self.lifecycle = ExecutionLifecycle::Running;
        }

        match (self.operational, event) {
            (Starting, StartRequested) => (
                Connecting,
                vec![Action::Reconnect(self.resource_id)],
                "StartRequested",
            ),

            (Connecting, ResourceAvailable(_)) => (Ready, vec![], "ResourceAvailable"),
            (Connecting, ResourceUnavailable(_)) => (
                Connecting,
                vec![Action::Reconnect(self.resource_id)],
                "ResourceUnavailable",
            ),

            (Ready, ExecuteRequested(mut request)) => {
                request.attempt = self.context.attempt;
                self.context.in_flight = Some(request.payload.clone());
                (
                    Processing,
                    vec![Action::Execute(request)],
                    "ExecuteRequested",
                )
            }
            (Ready, ResourceUnavailable(_)) => (
                Connecting,
                vec![Action::Reconnect(self.resource_id)],
                "ResourceUnavailable",
            ),

            (Processing, ActionCompleted(_, _)) => {
                self.context.attempt = 0;
                self.context.in_flight = None;
                self.context.last_failure = None;
                (Ready, vec![], "ActionCompleted")
            }
            (Processing, ActionFailed(failure)) => {
                self.context.attempt += 1;
                self.context.last_failure = Some(failure);
                (Recovering, vec![Action::Recover], "ActionFailed")
            }

            (Recovering, RecoveryRequested(ctx)) => self.on_recovery_decision(ctx),

            (Degraded, ExecuteRequested(_)) => {
                // Still running: the event is acknowledged, not lost, but
                // real processing waits until recovery completes.
                (Degraded, vec![], "ExecuteRequestedWhileDegraded")
            }
            (Degraded, DiagnosticAvailable(result)) => {
                if accept_proposal(result.proposal) {
                    self.context.attempt = 0;
                    let request = self.pending_request();
                    (Processing, vec![Action::Retry(request)], "ProposalAccepted")
                } else {
                    (Degraded, vec![], "ProposalRejected")
                }
            }

            (state, _unhandled) => (state, vec![], "Unhandled"),
        }
    }

    fn on_recovery_decision(
        &mut self,
        ctx: RecoveryContext,
    ) -> (OperationalState, Vec<Action>, &'static str) {
        if ctx.exhausted {
            (
                OperationalState::Degraded,
                vec![Action::RequestDiagnostic(self.analysis_context())],
                "RecoveryExhausted",
            )
        } else {
            let request = self.pending_request();
            (
                OperationalState::Processing,
                vec![Action::Retry(request)],
                "RecoveryRequested",
            )
        }
    }

    fn pending_request(&self) -> CapabilityRequest {
        CapabilityRequest {
            attempt: self.context.attempt,
            payload: self
                .context
                .in_flight
                .clone()
                .expect("a retry always follows an in-flight request"),
        }
    }

    fn analysis_context(&self) -> AnalysisContext {
        AnalysisContext {
            execution_id: self.execution_context.execution_id.clone(),
            correlation_id: self.execution_context.correlation_id.clone(),
            attempt: self.context.attempt,
            last_failure: self.context.last_failure.clone(),
        }
    }
}

/// Policy/validation gate between an intelligence proposal and an actual
/// action — the state machine, not the intelligence layer, has the final
/// say (section 15).
fn accept_proposal(proposal: Proposal) -> bool {
    matches!(proposal, Proposal::Retry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::FailureKind;
    use crate::intelligence::DiagnosticResult;
    use crate::resource::ResourceId;

    fn machine() -> StateMachine {
        StateMachine::new(ExecutionContext::new("exec", "corr"), ResourceId("primary"))
    }

    fn ready() -> StateMachine {
        let mut machine = machine();
        machine.handle(Event::StartRequested);
        machine.handle(Event::ResourceAvailable(ResourceId("primary")));
        assert_eq!(machine.operational_state(), OperationalState::Ready);
        machine
    }

    fn execute(payload: &str) -> Event {
        Event::ExecuteRequested(CapabilityRequest {
            attempt: 0,
            payload: payload.to_string(),
        })
    }

    fn failure(kind: FailureKind) -> ActionFailure {
        ActionFailure {
            reason: "x".to_string(),
            kind,
        }
    }

    /// Test: normal execution — Start -> Ready -> Execute -> Completed ->
    /// Ready.
    #[test]
    fn normal_execution_returns_to_ready() {
        let mut machine = ready();
        let transition = machine.handle(execute("hello"));
        assert_eq!(transition.operational_to, OperationalState::Processing);

        let transition = machine.handle(Event::ActionCompleted(
            crate::capability::CapabilityId("test"),
            "done".to_string(),
        ));
        assert_eq!(transition.operational_to, OperationalState::Ready);
        assert_eq!(machine.attempt(), 0);
    }

    /// Test: failure does not terminate — Processing -> ActionFailed ->
    /// Recovering, and the machine is still fully usable afterwards.
    #[test]
    fn action_failed_transitions_to_recovering_not_termination() {
        let mut machine = ready();
        machine.handle(execute("x"));
        let transition = machine.handle(Event::ActionFailed(failure(FailureKind::Transient)));
        assert_eq!(transition.operational_to, OperationalState::Recovering);
        assert!(transition.actions.contains(&Action::Recover));
        assert_eq!(machine.lifecycle(), ExecutionLifecycle::Running);
    }

    /// Test: deterministic retry — Recovering -> Retry -> Processing ->
    /// Success -> Ready.
    #[test]
    fn recovery_requested_retries_then_succeeds() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Transient)));
        assert_eq!(machine.operational_state(), OperationalState::Recovering);

        let transition = machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 1,
            exhausted: false,
        }));
        assert_eq!(transition.operational_to, OperationalState::Processing);
        assert!(
            transition
                .actions
                .contains(&Action::Retry(CapabilityRequest {
                    attempt: 1,
                    payload: "x".to_string(),
                }))
        );

        let transition = machine.handle(Event::ActionCompleted(
            crate::capability::CapabilityId("test"),
            "ok".to_string(),
        ));
        assert_eq!(transition.operational_to, OperationalState::Ready);
    }

    /// Test: repeated failure — exhausting the recovery budget lands in
    /// Degraded, not a generic `Failed` state.
    #[test]
    fn recovery_exhausted_transitions_to_degraded() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Transient)));
        let transition = machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 3,
            exhausted: true,
        }));
        assert_eq!(transition.operational_to, OperationalState::Degraded);
        assert!(matches!(
            transition.actions.as_slice(),
            [Action::RequestDiagnostic(_)]
        ));
    }

    #[test]
    fn degraded_remains_operational() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Permanent)));
        machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 1,
            exhausted: true,
        }));
        assert_eq!(machine.operational_state(), OperationalState::Degraded);

        // Still alive: it keeps acknowledging events rather than being
        // stuck.
        let transition = machine.handle(execute("y"));
        assert_eq!(transition.operational_to, OperationalState::Degraded);
    }

    /// Test: intelligence proposal — the state machine, not the proposal
    /// itself, decides whether it is accepted.
    #[test]
    fn degraded_accepts_a_retry_proposal() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Transient)));
        machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 3,
            exhausted: true,
        }));
        assert_eq!(machine.operational_state(), OperationalState::Degraded);

        let transition = machine.handle(Event::DiagnosticAvailable(DiagnosticResult {
            observation: "looks recoverable".to_string(),
            proposal: Proposal::Retry,
        }));
        assert_eq!(transition.operational_to, OperationalState::Processing);
        assert_eq!(machine.attempt(), 0);
    }

    #[test]
    fn degraded_rejects_a_no_action_proposal() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Permanent)));
        machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 1,
            exhausted: true,
        }));

        let transition = machine.handle(Event::DiagnosticAvailable(DiagnosticResult {
            observation: "not fixable by retrying".to_string(),
            proposal: Proposal::NoAction,
        }));
        assert_eq!(transition.operational_to, OperationalState::Degraded);
    }

    /// Test: explicit shutdown — only `ShutdownRequested` moves the
    /// lifecycle to `Stopping`, from any operational state.
    #[test]
    fn shutdown_requested_moves_lifecycle_to_stopping_from_degraded() {
        let mut machine = ready();
        machine.handle(execute("x"));
        machine.handle(Event::ActionFailed(failure(FailureKind::Permanent)));
        machine.handle(Event::RecoveryRequested(RecoveryContext {
            attempt: 1,
            exhausted: true,
        }));
        assert_eq!(machine.operational_state(), OperationalState::Degraded);

        let transition = machine.handle(Event::ShutdownRequested);
        assert_eq!(transition.lifecycle_to, ExecutionLifecycle::Stopping);
        // A processing failure alone never did this; only the explicit
        // event does.
        assert_ne!(transition.lifecycle_from, ExecutionLifecycle::Stopping);
    }
}
