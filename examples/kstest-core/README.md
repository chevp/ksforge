# kstest-core

> `kstest-core` = deterministic execution runtime.
> Intelligence = optional advisory layer.

`kstest-core` is a small, generic, deterministic, continuously running
processing/execution runtime, controlled by an explicit state machine. It
has no dependency on any LLM, agent, prompt, AI API, image processing, Git,
GitHub, or concrete domain — and none of that is needed for it to work.
**The system must be operationally complete without an LLM.**

An optional, generic `Intelligence` interface can be attached later for
diagnostics, planning, debugging, optimization, commissioning assistance,
and recovery suggestions. It is advisory only: it observes and proposes,
it never controls.

```text
Without Intelligence:
    the runtime still connects, processes, retries, degrades, and
    recovers from every known failure class on its own.

With Intelligence:
    the runtime additionally gets a second opinion on failures its
    deterministic policy alone could not resolve — deeper diagnostics,
    debugging, optimization, planning, commissioning assistance, and
    recovery suggestions — but nothing it proposes is ever obeyed
    blindly (see "Proposal, not control" below).
```

## The idea

> The runtime does not stop when processing fails. The state machine
> determines how the system continues.

```text
event
  |
  v
state machine
  |
  v
controlled action
  |
  v
new event
  |
  v
state machine
  |
  v
...
```

A failure is not a reason to unwind the call stack. It is an `Event`, the
same kind of thing a successful result is. The state machine decides what
happens next — retry, degrade, ask for a diagnostic — and the loop keeps
running either way. Only an explicit `Event::ShutdownRequested` ends it.

## Four things that are not the same thing

| Concept | What it answers | Where it lives |
|---|---|---|
| Execution lifecycle | Is the process still running at all? | `lifecycle::ExecutionLifecycle` — only `Stopped` ends it |
| Operational state | What is the runtime doing right now? | `state::OperationalState` (`Ready`, `Processing`, `Recovering`, `Degraded`, ...) |
| Processing result | Did *this one* capability call succeed? | `Event::ActionCompleted` / `Event::ActionFailed` |
| Diagnostic state | What did the slow path learn? | `intelligence::DiagnosticResult` — data only, never authority |

A run can be `Execution = Running` while `State = Recovering`, and later
`Execution = Running` while `State = Ready` — that is expected, not an
error. A processing failure changes *operational state*, never *execution
lifecycle*. Conflating the two is what makes `process(); if failed {
return Err(...) }` the wrong default for a long-running system: it turns
an operational condition into a program termination. There is also
deliberately no generic `Failed` operational state — every failure lands
somewhere specific (`Recovering`, `Degraded`).

## Architecture

```text
                         +---------------------------+
                         | Optional Intelligence     |
                         |                           |
                         | Diagnostics  Planning      |
                         | Debugging    Optimization  |
                         | Commissioning Recovery advice
                         +-------------+--------------+
                                       |
                               observations /
                                 proposals
                                       |
                                       v
+------------------------------------------------------------+
|                       kstest-core                          |
|                                                              |
|  +------------+     +--------------+                        |
|  |   Events   |---->| StateMachine |                        |
|  +------------+     +------+-------+                        |
|                             |                                 |
|                          Actions                              |
|                             |                                 |
|                             v                                 |
|                       +-----------+                           |
|                       | Scheduler |                           |
|                       +-----+-----+                           |
|                             |                                 |
|                  +----------+----------+                      |
|                  v                     v                      |
|             Capability             Resource                   |
|                  |                     |                      |
|                  +----------+----------+                      |
|                             v                                 |
|                       Result/Event                            |
|                             |                                 |
|                             +---------------> Events           |
+------------------------------------------------------------+
                                       |
                                       v
                             Domain Capabilities
                    (ksimg, ksaudio, ksvideo, ks3d, ...)
```

The core knows nothing about any domain. A capability like image
generation is something that *implements* `Capability` and is handed to
`Runtime::new` — the core never contains an `Image` or `Prompt` type.

## Fast path and slow path

The **fast path** (`event`, `state`, `action`, `state_machine`,
`capability`, `resource`, `recovery`, `validation`) is deterministic,
synchronous, and always available: no external services, nothing that can
block the loop. Every known failure class is handled here without any
intelligence:

```text
Timeout / transient failure  -> retry (bounded by RetryPolicy)
Permanent failure            -> skip straight to an operational failure
                                 (no point retrying)
Retry budget exhausted       -> Degraded, request a diagnostic
```

The **slow path** (`intelligence`, `scheduler`'s diagnostic lane) is
optional, may be slow, and reports back as an ordinary event — never by
reaching into runtime state directly. `Runtime::step` only ever checks
whether a diagnostic result has *already* arrived (`try_recv`); it never
blocks waiting for one, which is what keeps the two paths independent. If
no `Intelligence` is registered, `Scheduler` answers a diagnostic request
immediately with a deterministic fallback instead of spawning a thread —
the runtime never blocks on intelligence's absence.

## Proposal, not control

```text
Capability failure
      |
      v
StateMachine (deterministic decision: retry / degrade)
      |
      v
optional Intelligence --------> Proposal (Retry / Degrade / NoAction)
      |                              |
      v                              v
   Action                     Policy / Validation
                                     |
                                     v
                               StateMachine (final decision)
                                     |
                                     v
                                  Action
```

`Intelligence::analyze` takes an owned `AnalysisContext` and returns an
owned `DiagnosticResult` — there is no parameter through which it could
reach a `Runtime` or `StateMachine` at all, so it is structurally
incapable of mutating state (see
`tests/runtime.rs::intelligence_provider_has_no_access_to_runtime_state`).
Its `Proposal` only ever becomes an `Action` if the state machine's own
`accept_proposal` gate agrees (`state_machine.rs`) — the same as any other
event.

## Where the pieces are

| Module | Responsibility |
|---|---|
| `context.rs` | `ExecutionId`, `CorrelationId`, `Metadata`, `ExecutionContext` — technical tracing only, never domain/workflow data |
| `lifecycle.rs` | `ExecutionLifecycle` — separate from operational state |
| `event.rs` | Everything that can happen, including failures |
| `state.rs` | `OperationalState`, `Transition` |
| `state_machine.rs` | The only code allowed to change operational state or lifecycle |
| `action.rs` | The side effects a transition can request |
| `capability.rs` | `Capability` trait, `ActionFailure`, `FailureKind` — a capability executes; it never orchestrates globally |
| `resource.rs` | `Resource` trait — provides external capabilities; never owns runtime state |
| `recovery.rs` | `RetryPolicy`, `decide` — the one deterministic place that decides retry vs. give up |
| `validation.rs` | Deterministic technical validation, independent of any intelligence layer |
| `intelligence.rs` | `Intelligence` trait, `Proposal`, `DiagnosticResult` — advisory only |
| `scheduler.rs` | `WorkClass`, runs diagnostic work off the runtime's thread |
| `observability.rs` | `StateTransitionRecord` — what an intelligence layer would analyze later |
| `runtime.rs` | The event loop |

## Running it

```sh
cargo run
```

```sh
cargo test
```

`src/main.rs` wires up a domain-free `TestProcessingCapability` and a
`MockResource`, and runs through: normal execution; a transient failure
that the deterministic retry policy alone resolves; a failure that
outlasts the policy, degrades, and is resolved by an intelligence proposal
the state machine accepts; a permanent failure intelligence correctly
declines to paper over (stays `Degraded`, proving it does not retry
forever); and an explicit shutdown — from a non-`Ready` state, since only
`ShutdownRequested` is allowed to end the run.
