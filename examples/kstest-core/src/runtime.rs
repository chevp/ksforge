use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::action::Action;
use crate::capability::{ActionFailure, Capability, CapabilityResult, FailureKind};
use crate::context::ExecutionContext;
use crate::event::{Event, RecoveryContext};
use crate::intelligence::Intelligence;
use crate::lifecycle::ExecutionLifecycle;
use crate::observability::StateTransitionRecord;
use crate::recovery::{self, RetryPolicy};
use crate::resource::{Resource, ResourceRequest, ResourceResult};
use crate::scheduler::{Scheduler, WorkClass};
use crate::state_machine::StateMachine;
use crate::validation;

/// What happened on one call to `Runtime::step`.
#[derive(Debug, PartialEq, Eq)]
pub enum StepResult {
    /// An event was handled; there may be more queued.
    Progressed,
    /// Nothing was queued on the fast path right now.
    Idle,
    /// The execution lifecycle reached `Stopped`. The runtime should not
    /// be stepped again.
    Stopped,
}

/// The continuous event loop: pulls one event at a time, hands it to the
/// state machine, and carries out whatever actions come back. It never
/// stops on its own — only `Event::ShutdownRequested` does that, via the
/// state machine (see `state_machine::StateMachine`).
///
/// This crate has no dependency on any LLM, agent, or external
/// intelligence. `intelligence: None` is a fully supported, fully
/// operational configuration — see `scheduler::Scheduler`.
pub struct Runtime {
    state_machine: StateMachine,
    events: VecDeque<Event>,
    diagnostics_rx: Receiver<Event>,
    scheduler: Scheduler,
    resource: Box<dyn Resource>,
    capability: Box<dyn Capability>,
    retry_policy: RetryPolicy,
    transitions: Vec<StateTransitionRecord>,
}

impl Runtime {
    pub fn new(
        execution_context: ExecutionContext,
        capability: Box<dyn Capability>,
        resource: Box<dyn Resource>,
        retry_policy: RetryPolicy,
        intelligence: Option<Arc<dyn Intelligence>>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let resource_id = resource.id();
        Self {
            state_machine: StateMachine::new(execution_context, resource_id),
            events: VecDeque::new(),
            diagnostics_rx: rx,
            scheduler: Scheduler::new(intelligence, tx),
            resource,
            capability,
            retry_policy,
            transitions: Vec::new(),
        }
    }

    pub fn operational_state(&self) -> crate::state::OperationalState {
        self.state_machine.operational_state()
    }

    pub fn lifecycle(&self) -> ExecutionLifecycle {
        self.state_machine.lifecycle()
    }

    pub fn transitions(&self) -> &[StateTransitionRecord] {
        &self.transitions
    }

    /// Feed an event in from outside (a start request, a client execution
    /// request, an operator's shutdown request, ...).
    pub fn push_event(&mut self, event: Event) {
        self.events.push_back(event);
    }

    /// Prefers a queued fast-path event; only checks whether a diagnostic
    /// result has *already* arrived. Never waits on the diagnostics
    /// channel — that would make the fast path hostage to the slow path,
    /// exactly what this architecture rules out.
    fn next_event(&mut self) -> Option<Event> {
        if let Some(event) = self.events.pop_front() {
            return Some(event);
        }
        self.diagnostics_rx.try_recv().ok()
    }

    /// Handle exactly one event. Returns `Idle` rather than blocking when
    /// there is nothing to do — callers decide for themselves whether
    /// that's worth waiting out (see `block_for_diagnostics`).
    pub fn step(&mut self) -> StepResult {
        let Some(event) = self.next_event() else {
            return StepResult::Idle;
        };
        println!("[FAST] event: {event:?}");
        let transition = self.state_machine.handle(event);
        println!(
            "[FAST] state: {:?} (lifecycle: {:?})",
            transition.operational_to, transition.lifecycle_to
        );
        self.transitions.push(StateTransitionRecord {
            execution_id: self.state_machine.execution_id().clone(),
            correlation_id: self.state_machine.correlation_id().clone(),
            from: transition.operational_from,
            to: transition.operational_to,
            reason: transition.reason,
        });
        for action in transition.actions {
            self.execute(action);
        }
        if self.state_machine.lifecycle() == ExecutionLifecycle::Stopped {
            StepResult::Stopped
        } else {
            StepResult::Progressed
        }
    }

    /// Blocks until the slow path delivers its next result. Only meant for
    /// a caller that is deliberately idle and waiting on diagnostics — the
    /// fast path itself never calls this.
    pub fn block_for_diagnostics(&mut self) -> bool {
        match self.diagnostics_rx.recv() {
            Ok(event) => {
                self.events.push_back(event);
                true
            }
            Err(_) => false,
        }
    }

    fn execute(&mut self, action: Action) {
        if action.work_class() == WorkClass::Diagnostic {
            if let Action::RequestDiagnostic(context) = action {
                self.scheduler.schedule_diagnostic(context);
            }
            return;
        }

        match action {
            Action::Execute(request) | Action::Retry(request) => {
                let event = match validation::validate_capability_request(&request) {
                    Err(error) => Event::ActionFailed(ActionFailure {
                        reason: error.0,
                        kind: FailureKind::Permanent,
                    }),
                    Ok(()) => match self.capability.execute(request) {
                        CapabilityResult::Success(output) => {
                            Event::ActionCompleted(self.capability.id(), output)
                        }
                        CapabilityResult::Failure(failure) => Event::ActionFailed(failure),
                    },
                };
                self.events.push_back(event);
            }
            Action::Reconnect(_resource_id) => {
                let event = match self.resource.execute(ResourceRequest::Connect) {
                    ResourceResult::Connected => Event::ResourceAvailable(self.resource.id()),
                    ResourceResult::Disconnected | ResourceResult::Unavailable(_) => {
                        Event::ResourceUnavailable(self.resource.id())
                    }
                };
                self.events.push_back(event);
            }
            Action::Recover => {
                let attempt = self.state_machine.attempt();
                let failure = self
                    .state_machine
                    .last_failure()
                    .cloned()
                    .expect("Recover always follows a recorded failure");
                // A short, deterministic backoff pause — not a wait on the
                // slow path, just the ordinary meaning of "back off".
                thread::sleep(self.retry_policy.delay_for(attempt));
                let exhausted = recovery::decide(attempt, &failure, &self.retry_policy)
                    == recovery::RecoveryDecision::Exhausted;
                self.events
                    .push_back(Event::RecoveryRequested(RecoveryContext {
                        attempt,
                        exhausted,
                    }));
            }
            Action::Shutdown => {
                self.resource.execute(ResourceRequest::Disconnect);
                println!("[FAST] resources stopped");
                self.state_machine.complete_shutdown();
            }
            Action::RequestDiagnostic(_) => unreachable!("routed via work_class above"),
        }
    }
}
