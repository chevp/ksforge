# ksimg

`ksimg` is a reference architecture for turning a user's intent into a
validated, deterministically executed image-processing workflow — without
any single agent generating the executable workflow JSON by hand.

It is a self-contained example, unrelated to `ksforge` itself: no
`ksforge` concepts, dependencies, or assumptions, and it is not part of
`ksforge`'s build. It needs no real image model, no Claude, no OpenAI, and
no network access — every technique is simulated. The point is the
architecture around the techniques, not the techniques themselves.

## The problem

The naive way to let an agent drive a workflow engine looks like this:

```text
Agent
  |
  v
complete workflow JSON
  |
  v
Execution
```

This breaks down in predictable ways:

- The agent has to know every technique's exact input/output wiring.
- The agent has to invent IDs and references and get them right.
- The agent has to get types right by hand, with no compiler and no
  registry to check it against.
- The workflow JSON grows large and fragile.
- Changing a technique's contract means changing the agent's prompt.
- Type and wiring errors surface only at execution time, if at all.

## The architecture

```text
User Intent
    |
    v
Intent Agent
    |
    v
Planning Agent
    |
    v
Logical Workflow
    |
    v
Workflow Builder ---- Technique Registry
    |                 Input/Output Schemas
    |                 Constraints
    v
Executable Workflow
    |
    v
Workflow Validator
    |
    v
State Machine
    |
    v
Deterministic Execution
    |
    +-- Technique A
    +-- Technique B
    +-- Technique C
    |
    v
Result
```

> Agents generate proposals. The runtime owns execution.

The `IntentAgent` and `PlanningAgent` never see a technique ID, a value
reference, or a data type wire-up. They describe a goal and a logical
step order — see `agents::intent` and `agents::planner`. The
`WorkflowBuilder` (`workflow::builder`) is the only thing that resolves
logical step names against the `TechniqueRegistry` (`techniques::registry`)
and produces IDs, references, and a real dependency graph. The
`WorkflowValidator` (`workflow::validator`) is the only gate before
execution: a workflow with any validation error never reaches the
`StateMachine`.

**JSON Schema is a contract/type system, not the workflow itself.**
`schemas/*.json` shows what a technique's real input/output contract could
look like; `techniques::registry::ParameterSchema` is the simplified Rust
stand-in this example actually uses. Nothing here hand-assembles those
schemas into one giant workflow document — the builder does that
resolution structurally, from typed contracts.

## Failure is not termination

```text
Technique failure
      |
      v
State Machine
      |
      v
Recovery
      |
      +-- retry
      +-- retry with alternative parameters
      +-- alternative technique
      +-- replan
      +-- continue degraded
      |
      v
continue execution
```

A technique failure is an `Event`, handled the same way a success is —
see `runtime::state_machine`. It moves the state machine from `Processing`
to `Recovering`, never to a generic `Failed`/`Stopped`. The
`RecoveryAgent` (`agents::recovery`) only ever returns a
`RecoveryProposal`; the `StateMachine` decides whether and how to act on
it. Only an explicit `Event::ShutdownRequested` ends the runtime.

```text
STARTING -> READY
              |  ^
       RunStep|  | StepSucceeded
              v  |
          PROCESSING
              |
      StepFailed
              v
         RECOVERING --RetryRequested--> PROCESSING
              |
   ContinueDegradedRequested
              v
          DEGRADED

any state --ShutdownRequested--> STOPPING --ShutdownCompleted--> STOPPED
```

`READY` is the one stable state. Workflow planning and validation happen
*before* a workflow ever reaches the state machine — the state machine
itself only ever knows about `Processing` one step at a time, which is
why `PLANNING`/`VALIDATING` are not states in it (see CLAUDE.md section
11: this repo's task explicitly allows keeping them outside the execution
state machine).

## Fast path / slow path

```text
Fast: technique execution, validation, state transitions, scheduling
Slow: intent interpretation, planning, diagnostics, recovery proposals, optimization
```

In this example the "slow" agents are cheap deterministic mocks, so the
engine calls them inline rather than off-thread. The boundary they sit
behind — `IntentAgent`, `PlanningAgent`, `RecoveryAgent` traits, never
called by a technique or by the state machine itself — is what would let
a real implementation move them onto a background thread (as
`examples/kstest-core::scheduler` does for its `Analyzer`) without
touching `workflow::builder`, `workflow::validator`, or
`runtime::state_machine`.

## Relationship to `kstest-core`

`examples/kstest-core` demonstrates the same core idea — a continuously
running system whose failures are events handled by an explicit state
machine, never unwinding — for a WebSocket-message-processing domain.
`ksimg` follows the same pattern (`StateMachine::handle` as the only
place allowed to change state, `Action` as the only side effect a
transition may request, a deterministic mock in place of anything slow
or external) but does not depend on it as a library: `kstest-core`'s
`Event`, `Action`, and `MachineState` are concrete types for its own
domain (`Connecting`, `AuthenticationFailed`, `Message`, ...), not a
generic engine `ksimg` could parameterize. Reusing the pattern here
rather than genericizing `kstest-core` keeps both examples small and
independently readable.

## What is not the same thing

```text
Technique       != Workflow           (a contract vs. a graph of calls)
Workflow        != Workspace Layout   (fachliche Logik vs. Präsentation)
Agent           != Executor           (a proposal vs. what actually runs)
Agent           != State Machine      (a proposal vs. the authority)
Failure         != Termination        (an Event vs. ending the runtime)
JSON Schema     != Workflow JSON      (a type system vs. the workflow itself)
```

## No hidden responsibilities

- **Agents** never touch the state machine, never execute a technique
  directly, never hide a retry loop, never store workflow state, never
  end execution.
- **Techniques** (`techniques/*.rs`) never mutate global state, never
  retry themselves, never call an agent, never stop execution — each is
  just `fn execute(inputs) -> Result<Artifact, String>`.
- **`WorkflowBuilder`** never calls an agent and never touches runtime
  state — it is a pure function of a `LogicalWorkflow`, a
  `TechniqueRegistry`, and `Constraints`.
- **`StateMachine`** is the only thing that processes events, decides
  actions, and controls recovery and retry.

## Where the pieces are

| Module | Responsibility |
|---|---|
| `domain` | `DataType`, `Artifact`, `Constraints`, `ProcessingIntent` |
| `techniques::registry` | `Technique` contracts, `TechniqueRegistry` — the source of technical truth |
| `techniques::{txt2txt, txt2img, img2txt, img2img, segmentation, background_removal, validate}` | Simulated technique executors and their contracts |
| `agents::intent` | `IntentAgent` / `MockIntentAgent` — user goal to `ProcessingIntent` |
| `agents::planner` | `PlanningAgent` / `MockPlanningAgent` — intent to `LogicalWorkflow` |
| `agents::recovery` | `RecoveryAgent` / `MockRecoveryAgent` — failure to `RecoveryProposal` |
| `workflow::ir` | `Workflow`, `Step`, `ValueRef` — the executable, declarative representation |
| `workflow::builder` | Resolves a `LogicalWorkflow` into a `Workflow`, deterministically |
| `workflow::validator` | Rejects any `Workflow` that isn't safe to execute |
| `workflow::layout` | Presentation-only node positions — never read by planning, building, validating, or execution |
| `runtime::state_machine` | `MachineState`, `Event`, `Action` — the only code allowed to change state |
| `runtime::engine` | `WorkflowEngine` — drives a validated `Workflow` through the state machine |

## Running it

```sh
cargo run
```

```sh
cargo test
```

`cargo run` walks through five scenarios: `img2img` (transform an image),
background removal (`segmentation` -> `background_removal` -> `validate`),
text-to-image (`txt2txt` -> `txt2img` -> `validate`, the `txt2txt` step
improving the prompt before generation), a failure/recovery demo where
`segmentation` fails once and the `RecoveryAgent`'s `Retry` proposal lets
the state machine recover without stopping the runtime, and a standalone
prompt-improvement request that plans to just `txt2txt`, for direct prompt
improvements ahead of image generation without generating an image.

`cargo test` covers: planning text-to-image (including the `txt2txt`
prompt-improvement step ahead of `txt2img`) and background removal to the
right technique sequence; a standalone prompt-improvement intent planning
to `txt2txt` alone; the validator rejecting a workflow that feeds
an `Image` to a technique that requires `Text`; a failed step landing in
`Recovering` rather than a terminal state; a retry returning to `Ready`;
and an explicit shutdown being the only way to reach `Stopped`.
