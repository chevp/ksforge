---
name: add-technique
description: Add a new image-processing technique (contract + executor) to the ksimg TechniqueRegistry. Use when a change request asks for a new operation such as blur, upscale or colorize.
---

# Add a technique

A technique is a contract (`Technique`) plus an executor (`TechniqueExecutor`). The registry is the only source of truth for what exists; agents only propose operation names.

## Steps

1. Create `src/techniques/<name>.rs`, modeled on `background_removal.rs`:
   - `pub struct <Name>;` implementing `TechniqueExecutor::execute(&self, inputs, attempt) -> Result<Artifact, String>`.
   - `pub fn contract() -> Technique` with `id`, `input_types`, `output_types`, `parameters`, `metadata`.
2. In `src/techniques/mod.rs`: add `pub mod <name>;` and one `registry.register(<name>::contract(), Box::new(<name>::<Name>));` in `default_registry()`.
3. Only if a new data type is required: extend `DataType` in `src/domain/types.rs`. Prefer the existing four (`Text`, `Image`, `Mask`, `ValidatedImage`).
4. Only if the request says the planner should use it: add the mapping in `src/agents/planner.rs`.
5. Add a test in `tests/workflow_tests.rs` (planning/validation of a workflow that uses the technique).

## Rules

- The executor never holds global state, never retries, never calls an agent, never stops execution. Failure is `Err(String)`; recovery is the state machine's job.
- Do not touch `workflow/builder.rs`, `workflow/validator.rs`, `workflow/layout.rs` or `runtime/state_machine.rs` for a new technique. The builder and validator work from the contract alone.
- Simulated only: no network, no real model, no new dependencies. `description` on `Artifact` stands in for payload.
- Contracts must be honest: the validator rejects wiring by `input_types`/`output_types`.

## Validate

```sh
cargo test
cargo run
```

Both must pass without warnings introduced by the change.
