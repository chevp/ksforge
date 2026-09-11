use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use kstest_core::capability::{
    ActionFailure, Capability, CapabilityId, CapabilityRequest, CapabilityResult, FailureKind,
};
use kstest_core::context::ExecutionContext;
use kstest_core::event::Event;
use kstest_core::intelligence::{AnalysisContext, DiagnosticResult, Intelligence, Proposal};
use kstest_core::recovery::{Backoff, RetryPolicy};
use kstest_core::resource::MockResource;
use kstest_core::runtime::{Runtime, StepResult};
use kstest_core::state::OperationalState;

/// A capability with no domain (`kstest-core` must not know it exists to
/// demonstrate anything specific): succeeds on ordinary payloads,
/// simulates a transient fault that clears up on retry ("flaky"), a fault
/// that only clears up after more attempts than the deterministic policy
/// alone allows ("unstable" — this is what makes the diagnostic/
/// intelligence path worth having), and a fault that can never be fixed by
/// retrying at all ("broken").
struct TestProcessingCapability {
    unstable_calls: AtomicU32,
}

impl TestProcessingCapability {
    fn new() -> Self {
        Self {
            unstable_calls: AtomicU32::new(0),
        }
    }
}

impl Capability for TestProcessingCapability {
    fn id(&self) -> CapabilityId {
        CapabilityId("test-processing")
    }

    fn execute(&self, request: CapabilityRequest) -> CapabilityResult {
        match request.payload.as_str() {
            "flaky" if request.attempt < 2 => CapabilityResult::Failure(ActionFailure {
                reason: format!("transient failure on attempt {}", request.attempt),
                kind: FailureKind::Transient,
            }),
            "unstable" => {
                let calls = self.unstable_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if calls < 4 {
                    CapabilityResult::Failure(ActionFailure {
                        reason: format!("still unstable after {calls} attempt(s)"),
                        kind: FailureKind::Transient,
                    })
                } else {
                    CapabilityResult::Success(format!("processed after {calls} attempts"))
                }
            }
            "broken" => CapabilityResult::Failure(ActionFailure {
                reason: "payload is permanently invalid".to_string(),
                kind: FailureKind::Permanent,
            }),
            _ => CapabilityResult::Success(format!("processed: {}", request.payload)),
        }
    }
}

/// A deterministic stand-in for whatever advisory layer might eventually
/// be attached (an LLM being one possible implementation, never the only
/// one). It only ever returns data — see `Intelligence`.
struct MockIntelligenceProvider;

impl Intelligence for MockIntelligenceProvider {
    fn analyze(&self, context: AnalysisContext) -> DiagnosticResult {
        thread::sleep(Duration::from_millis(150));
        match context.last_failure.as_ref().map(|f| f.kind) {
            Some(FailureKind::Permanent) => DiagnosticResult {
                observation: "failure is permanent; retrying will not help".to_string(),
                proposal: Proposal::NoAction,
            },
            _ => DiagnosticResult {
                observation: format!(
                    "failure looked transient after {} attempt(s); recommend one more try",
                    context.attempt
                ),
                proposal: Proposal::Retry,
            },
        }
    }
}

fn execute_request(payload: &str) -> Event {
    Event::ExecuteRequested(CapabilityRequest {
        attempt: 0,
        payload: payload.to_string(),
    })
}

/// Drain the fast path until it goes idle or stops. Never touches the
/// diagnostics channel — this is what proves those two are independent.
fn drain(runtime: &mut Runtime) -> StepResult {
    loop {
        match runtime.step() {
            StepResult::Progressed => continue,
            other => return other,
        }
    }
}

/// Blocks exactly once for the one pending diagnostic result this demo
/// just triggered, then drains whatever it causes — the state machine may
/// accept the proposal (back to `Processing`/`Ready`) or reject it
/// (stays `Degraded`); either is a legitimate outcome.
fn wait_for_one_diagnosis(runtime: &mut Runtime) {
    if runtime.block_for_diagnostics() {
        drain(runtime);
    }
}

fn main() {
    println!("[kstest] execution started");

    let mut execution_context = ExecutionContext::new("exec-1", "corr-1");
    execution_context
        .metadata
        .insert("scenario".to_string(), "demo".to_string());

    let retry_policy = RetryPolicy {
        max_attempts: 2,
        backoff: Backoff::Fixed(Duration::from_millis(10)),
    };

    let mut runtime = Runtime::new(
        execution_context,
        Box::new(TestProcessingCapability::new()),
        Box::new(MockResource),
        retry_policy,
        Some(Arc::new(MockIntelligenceProvider)),
    );

    runtime.push_event(Event::StartRequested);
    drain(&mut runtime);

    // Normal operation: no recovery involved at all.
    runtime.push_event(execute_request("hello"));
    drain(&mut runtime);

    // Transient failure that clears up within the deterministic retry
    // policy — no intelligence needed.
    runtime.push_event(execute_request("flaky"));
    drain(&mut runtime);

    // A fault that outlasts the retry policy: degrades, and requests a
    // diagnostic in the background.
    runtime.push_event(execute_request("unstable"));
    drain(&mut runtime);
    assert_eq!(runtime.operational_state(), OperationalState::Degraded);

    // While DEGRADED, the fast path keeps taking events immediately — this
    // does not wait for the diagnostic thread running in the background.
    runtime.push_event(execute_request("hello"));
    drain(&mut runtime);

    // Let the slow path finish and hand its proposal to the state machine,
    // which decides to accept it (see `state_machine::accept_proposal`)
    // and retries — this time succeeding.
    wait_for_one_diagnosis(&mut runtime);
    assert_eq!(runtime.operational_state(), OperationalState::Ready);

    // A failure intelligence correctly declines to paper over: the state
    // machine stays DEGRADED rather than retrying forever.
    runtime.push_event(execute_request("broken"));
    drain(&mut runtime);
    wait_for_one_diagnosis(&mut runtime);
    assert_eq!(runtime.operational_state(), OperationalState::Degraded);

    // Only an explicit lifecycle event ends the runtime — even from a
    // non-Ready operational state.
    runtime.push_event(Event::ShutdownRequested);
    drain(&mut runtime);

    println!(
        "[kstest] runtime terminated ({} state transitions recorded)",
        runtime.transitions().len()
    );
}
