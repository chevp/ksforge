use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use kstest_core::capability::{
    ActionFailure, Capability, CapabilityId, CapabilityRequest, CapabilityResult, FailureKind,
};
use kstest_core::context::ExecutionContext;
use kstest_core::event::Event;
use kstest_core::intelligence::{AnalysisContext, DiagnosticResult, Intelligence, Proposal};
use kstest_core::lifecycle::ExecutionLifecycle;
use kstest_core::recovery::{Backoff, RetryPolicy};
use kstest_core::resource::MockResource;
use kstest_core::runtime::{Runtime, StepResult};
use kstest_core::state::OperationalState;

/// Fails every attempt, classified `Permanent` — always exhausts recovery
/// on the very first try, so tests reach `Degraded` in one step.
struct AlwaysFailsCapability;

impl Capability for AlwaysFailsCapability {
    fn id(&self) -> CapabilityId {
        CapabilityId("always-fails")
    }

    fn execute(&self, _request: CapabilityRequest) -> CapabilityResult {
        CapabilityResult::Failure(ActionFailure {
            reason: "always fails".to_string(),
            kind: FailureKind::Permanent,
        })
    }
}

/// Returns a fixed proposal, instantly — fast enough for tests, still
/// exercised through the same `Intelligence` trait as a real provider.
struct FixedIntelligence(Proposal);

impl Intelligence for FixedIntelligence {
    fn analyze(&self, _context: AnalysisContext) -> DiagnosticResult {
        DiagnosticResult {
            observation: "fixed test response".to_string(),
            proposal: self.0,
        }
    }
}

/// Never resolves within any test's lifetime — used to prove the fast path
/// does not wait on the slow path.
struct NeverIntelligence;

impl Intelligence for NeverIntelligence {
    fn analyze(&self, _context: AnalysisContext) -> DiagnosticResult {
        thread::sleep(Duration::from_secs(60));
        DiagnosticResult {
            observation: "unreachable in a test run".to_string(),
            proposal: Proposal::NoAction,
        }
    }
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 1,
        backoff: Backoff::Fixed(Duration::from_millis(1)),
    }
}

fn execute_request(payload: &str) -> Event {
    Event::ExecuteRequested(CapabilityRequest {
        attempt: 0,
        payload: payload.to_string(),
    })
}

fn drain(runtime: &mut Runtime) -> StepResult {
    loop {
        match runtime.step() {
            StepResult::Progressed => continue,
            other => return other,
        }
    }
}

fn started(intelligence: Option<Arc<dyn Intelligence>>) -> Runtime {
    let mut runtime = Runtime::new(
        ExecutionContext::new("exec", "corr"),
        Box::new(AlwaysFailsCapability),
        Box::new(MockResource),
        policy(),
        intelligence,
    );
    runtime.push_event(Event::StartRequested);
    drain(&mut runtime);
    assert_eq!(runtime.operational_state(), OperationalState::Ready);
    runtime
}

#[test]
fn runtime_does_not_terminate_on_processing_failure() {
    let mut runtime = started(None);

    runtime.push_event(execute_request("x"));
    let result = drain(&mut runtime);

    assert_ne!(result, StepResult::Stopped);
    assert_ne!(runtime.lifecycle(), ExecutionLifecycle::Stopping);
    // Still fully usable — it can accept more events.
    runtime.push_event(execute_request("y"));
    drain(&mut runtime);
}

#[test]
fn runtime_terminates_only_after_shutdown_requested() {
    let mut runtime = started(None);

    // A processing failure alone must not stop it.
    runtime.push_event(execute_request("x"));
    assert_ne!(drain(&mut runtime), StepResult::Stopped);

    runtime.push_event(Event::ShutdownRequested);
    let result = drain(&mut runtime);

    assert_eq!(result, StepResult::Stopped);
    assert_eq!(runtime.lifecycle(), ExecutionLifecycle::Stopped);
}

/// Test: optional intelligence absent — the whole runtime works with
/// `intelligence = None`.
#[test]
fn runtime_operates_fully_with_no_intelligence_registered() {
    let mut runtime = started(None);

    runtime.push_event(execute_request("x"));
    drain(&mut runtime);

    assert_eq!(runtime.operational_state(), OperationalState::Degraded);
    // Degraded, not stuck: shutdown still works cleanly.
    runtime.push_event(Event::ShutdownRequested);
    assert_eq!(drain(&mut runtime), StepResult::Stopped);
}

/// Test: intelligence proposal — a mock provider proposes `Retry`, and the
/// state machine (not the provider) decides to accept it.
#[test]
fn intelligence_retry_proposal_is_accepted_by_the_state_machine() {
    let mut runtime = started(Some(Arc::new(FixedIntelligence(Proposal::Retry))));

    runtime.push_event(execute_request("x"));
    drain(&mut runtime);
    assert!(runtime.block_for_diagnostics());
    drain(&mut runtime);

    let accepted = runtime
        .transitions()
        .iter()
        .any(|t| t.reason == "ProposalAccepted");
    assert!(
        accepted,
        "expected a ProposalAccepted transition in the history: {:?}",
        runtime.transitions()
    );

    // `AlwaysFailsCapability` fails every time, so the retry the accepted
    // proposal caused fails again too — proving the proposal visibly
    // caused another attempt through the ordinary event pipeline (a fresh
    // ActionFailed -> Recovering -> Degraded cycle), not that it directly
    // flipped operational state.
    assert_eq!(runtime.operational_state(), OperationalState::Degraded);
    assert_eq!(
        runtime.transitions().last().map(|t| t.reason),
        Some("RecoveryExhausted")
    );
}

/// Test: intelligence cannot mutate state. `Intelligence::analyze` takes
/// only an owned `AnalysisContext` and returns owned data — there is no
/// parameter through which it could reach a `Runtime` or `StateMachine` at
/// all. Demonstrate it by calling a provider completely standalone and
/// checking a runtime elsewhere is unaffected.
#[test]
fn intelligence_provider_has_no_access_to_runtime_state() {
    let provider = FixedIntelligence(Proposal::Retry);
    let context = AnalysisContext {
        execution_id: kstest_core::context::ExecutionId("x".into()),
        correlation_id: kstest_core::context::CorrelationId("y".into()),
        attempt: 3,
        last_failure: None,
    };

    let mut runtime = started(None);
    let before = runtime.operational_state();

    // Calling analyze() cannot have touched `runtime` — there is no
    // channel through which it could have.
    let _ = provider.analyze(context);

    assert_eq!(runtime.operational_state(), before);
    runtime.push_event(Event::ShutdownRequested);
    drain(&mut runtime);
}

#[test]
fn fast_path_keeps_processing_while_diagnostic_work_is_pending() {
    let mut runtime = started(Some(Arc::new(NeverIntelligence)));

    runtime.push_event(execute_request("x"));
    drain(&mut runtime);
    assert_eq!(runtime.operational_state(), OperationalState::Degraded);

    let start = Instant::now();
    runtime.push_event(execute_request("y"));
    let result = drain(&mut runtime);
    let elapsed = start.elapsed();

    assert_ne!(result, StepResult::Stopped);
    assert!(
        elapsed < Duration::from_millis(500),
        "fast path took {elapsed:?}, which suggests it waited on the slow path"
    );
}
