---
name: add-capability
description: Add a new Capability implementation to kstest-core, or a test capability for a failure scenario. Use when a change request needs a new kind of work handed to the Runtime, or a new failure/recovery test case.
---

# Add a capability

A capability executes one piece of domain work and reports the outcome. It never decides retry, never touches runtime state, never stops the runtime.

## Steps

1. Implement `Capability` (`src/capability.rs`) on a new struct:
   - `id(&self) -> CapabilityId` — a unique `CapabilityId("<kebab-name>")`.
   - `execute(&self, request: CapabilityRequest) -> CapabilityResult`, returning `Success(String)` or `Failure(ActionFailure { reason, kind })`.
2. Classify each failure with the right `FailureKind`:
   - `Transient` — retried by `RetryPolicy`.
   - `ResourceUnavailable` — retried, waiting on a resource.
   - `Permanent` — skips retries, goes straight to an operational failure.
3. Place it by purpose:
   - Demo path: next to `TestProcessingCapability` in `src/main.rs`, handed to `Runtime::new(...)` as `Box<dyn Capability>`.
   - Test scenario: in `tests/runtime.rs`, modeled on `AlwaysFailsCapability`, plus one `#[test]` that drives it through `started(...)` / `drain(...)`.
4. Domain types (images, prompts, ...) stay out of `src/`. The core must not know any domain.

## Rules

- Do not add a `Failed` operational state. Every failure lands in a specific state (`Recovering`, `Degraded`).
- Only `state_machine.rs` changes operational state or lifecycle. Only `Event::ShutdownRequested` ends the run.
- No LLM/agent/network dependency in the core. The runtime must stay fully operational with no `Intelligence` registered.
- `Intelligence` is advisory: it returns a `Proposal`, and `accept_proposal` in the state machine decides.
- No new dependencies.

## Validate

```sh
cargo test
cargo run
```

`cargo run` must still show all five scenarios ending in the expected states.
