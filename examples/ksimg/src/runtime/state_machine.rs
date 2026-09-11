use crate::techniques::TechniqueId;

/// The runtime's operational state. As in `kstest-core`, there is
/// deliberately no generic `Failed` state — every operational failure has
/// a specific, intentional destination (`Recovering`, `Degraded`), never
/// a catch-all. See CLAUDE.md section 11/12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    Starting,
    Ready,
    Processing,
    Recovering,
    Degraded,
    Stopping,
    Stopped,
}

impl MachineState {
    /// A stable state is one the runtime is content to sit in
    /// indefinitely between events. The others are transient, each with
    /// an explicit path onward.
    pub fn is_stable(self) -> bool {
        matches!(self, MachineState::Ready)
    }
}

/// Everything that can happen to the workflow runtime. A step failure is
/// a variant here like any other — data for the state machine to
/// interpret, never a reason to unwind. See CLAUDE.md section 12.
#[derive(Debug, Clone)]
pub enum Event {
    Start,
    RunStep,
    StepSucceeded,
    StepFailed(String),
    RetryRequested,
    AlternativeTechniqueRequested(TechniqueId),
    ReplanRequested,
    ReplanCompleted,
    ContinueDegradedRequested,
    ShutdownRequested,
    ShutdownCompleted,
}

/// The side effects a transition can request. The state machine returns
/// these rather than performing them, so it stays a pure decision point —
/// see `StateMachine::handle` and `runtime::engine::WorkflowEngine`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ExecuteStep,
    RequestRecoveryDiagnosis,
    Retry,
    SwapTechnique(TechniqueId),
    Replan,
    MarkDegraded,
    Shutdown,
}

#[derive(Debug)]
pub struct Transition {
    pub from: MachineState,
    pub to: MachineState,
    pub actions: Vec<Action>,
}

#[derive(Default)]
struct Context {
    attempt: u32,
    last_error: Option<String>,
}

/// The workflow runtime's sole source of control. No technique, agent, or
/// builder may reach in and change `state` directly — everything that
/// wants to affect it goes through `handle` as an `Event`. Mirrors the
/// pattern in `examples/kstest-core::state_machine::StateMachine`; see
/// that crate's README for the underlying idea, and this crate's README
/// for why `ksimg` does not depend on it directly.
pub struct StateMachine {
    state: MachineState,
    context: Context,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: MachineState::Starting,
            context: Context::default(),
        }
    }

    pub fn state(&self) -> MachineState {
        self.state
    }

    /// Number of retries already spent on the step currently in flight.
    /// Reset whenever a step succeeds, an alternative technique is
    /// swapped in, or a replan completes.
    pub fn attempt(&self) -> u32 {
        self.context.attempt
    }

    pub fn take_last_error(&mut self) -> Option<String> {
        self.context.last_error.take()
    }

    /// Interpret one event against the current state and return what
    /// changed. The only method in this module that assigns to `state`.
    pub fn handle(&mut self, event: Event) -> Transition {
        let from = self.state;
        let (to, actions) = self.next(event);
        self.state = to;
        Transition { from, to, actions }
    }

    fn next(&mut self, event: Event) -> (MachineState, Vec<Action>) {
        use Event::*;
        use MachineState::*;

        match (self.state, event) {
            (Starting, Start) => (Ready, vec![]),

            (Ready, RunStep) => (Processing, vec![Action::ExecuteStep]),
            (Degraded, RunStep) => (Processing, vec![Action::ExecuteStep]),

            (Processing, StepSucceeded) => {
                self.context.attempt = 0;
                (Ready, vec![])
            }
            (Processing, StepFailed(error)) => {
                self.context.last_error = Some(error);
                (Recovering, vec![Action::RequestRecoveryDiagnosis])
            }

            (Recovering, RetryRequested) => {
                self.context.attempt += 1;
                (Processing, vec![Action::Retry])
            }
            (Recovering, AlternativeTechniqueRequested(id)) => {
                self.context.attempt = 0;
                (Processing, vec![Action::SwapTechnique(id)])
            }
            (Recovering, ReplanRequested) => (Recovering, vec![Action::Replan]),
            (Recovering, ReplanCompleted) => {
                self.context.attempt = 0;
                (Ready, vec![])
            }
            (Recovering, ContinueDegradedRequested) => (Degraded, vec![Action::MarkDegraded]),

            // Only an explicit lifecycle event ends the runtime, from any
            // state — see CLAUDE.md section 12.
            (_, ShutdownRequested) => (Stopping, vec![Action::Shutdown]),
            (Stopping, ShutdownCompleted) => (Stopped, vec![]),

            // Anything not covered above does not apply to the current
            // state: acknowledged and dropped rather than mishandled.
            (state, _unhandled) => (state, vec![]),
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
