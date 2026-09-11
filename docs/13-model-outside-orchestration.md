# The model is outside the orchestration

**The phases are fixed. The tools per phase are fixed. The model fills in
the reasoning inside a boundary it cannot move.**

```bash
ksforge implement --change "As a user, I want to reset my password via email."
```

Nothing here asks Claude Code to plan its own workflow. `ksforge` already
decided the workflow — UNDERSTAND, then LOCATE, then ACT, then VALIDATE,
then REPORT — before the first token is generated. The model's job is to be
good at each phase, not to decide whether there should be five phases, three,
or one.

## The problem this avoids

A single, continuous agent turn that explores, edits, validates, and reports
all in one breath has no hard boundary between "still reading" and "now
writing." `ksforge` used to work that way. It doesn't anymore, because that
design had a real failure mode, not a hypothetical one:

> This is the actual security boundary the previous single-turn design
> lacked: a write-capable capability's tool grant used to cover the *entire*
> turn from its first token, so nothing stopped the agent from editing files
> while nominally still "exploring." Now it structurally cannot, until ACT.
> — [docs/03-architecture.md](03-architecture.md)

The fix was not a better prompt telling the model to explore first. It was
removing the model's ability to reach for `Edit`/`Write`/`Bash` at all during
UNDERSTAND and LOCATE — a Rust-level tool grant per subprocess, not a request.

## A fixed phase loop, not a model-planned one

```text
UNDERSTAND       LOCATE          ACT              VALIDATE           REPORT
   LLM             LLM           LLM              host-only         host-only
 read-only       read-only   capability's      validation::run,    ExecutionResult
   tools           tools     tool_policy()       deterministic     assembled from
                                                                    prior phases
```

Each phase is its own subprocess invocation of `claude` (or `codex` with
`--engine codex`) — the model never sees "here is your full plan, run it end
to end." It sees one phase's prompt, answers it, and the process exits.
`ksforge` decides what happens next.

The sequence is enforced by the type system, not by convention:
`domain::workflow::WorkflowState::Act` cannot be constructed without an
existing `Locate` value. There is no code path — not a missing check, an
absent one — that lets a run skip from UNDERSTAND straight to ACT. See
[03-architecture.md](03-architecture.md#the-phase-loop).

## Three questions ksforge never asks the model

**"Is your change valid?"** — Never model-controlled. The `AgentOutcome`
schema the model responds with has no field for validation commands. The
only source is `--validate`, supplied by a human or workflow author, run
*after* the model reports `completed`, before ksforge trusts that report.
See [08-validation.md](08-validation.md).

**"What did you change?"** — Not taken on the model's word.
`AgentOutcome.changed_files` is treated as advisory only. The actual answer
comes from content-hashing every file under the workspace root before and
after the ACT turn and diffing the two maps — the filesystem, not the
model's self-report, is ground truth. See
[03-architecture.md](03-architecture.md#change-detection-has-no-git-dependency).

**"Are you done?"** — REPORT runs no agent turn at all. It is plain Rust
code that assembles the final result from what UNDERSTAND, LOCATE, ACT, and
VALIDATE already produced. It never re-asks the model what happened, because
by that point the filesystem diff and the validation exit codes already
answer it more reliably than another turn would.

## A run, traced

```text
$ ksforge implement --change "..."
→ UNDERSTAND  claude -p --tools Read,Grep,Glob --json-schema <outcome>
  ✓ status: completed  (scope only — no files touched, no Bash)
→ LOCATE      claude -p --tools Read,Grep,Glob --resume <session-id>
  ✓ status: completed  (relevant_files, existing_abstractions, conventions)
→ ACT         claude -p --permission-mode acceptEdits --allowedTools Bash,PowerShell \
                --resume <session-id>
  ✓ status: completed  (changed_files reported — advisory)
→ VALIDATE    host-only: run each --validate command against the real diff
  ✓ cargo check   ✓ cargo test
→ REPORT      host-only: ExecutionResult assembled, no model call
  ✓ Execution <id> completed
```

Illustrative trace assembled from the documented subprocess contract in
[03-architecture.md](03-architecture.md#the-claude-code-subprocess-contract),
not a captured live run — the flags and phase order are real; the specific
session output above is not.

## Where this differs from "the model is the runtime"

Some agent-orchestration tools put the sequencing itself inside the model's
reach at runtime — declare a contract, hand it to a harness, and let "the
model plus your harness" work out the steps on each run. That trades a fixed
host-enforced loop for flexibility: no Rust state machine to extend when a
new phase shape is needed.

`ksforge` makes the opposite trade deliberately. The phase sequence, the
tool grant per phase, and the definition of "done" all live in Rust, compiled
into the binary, unreachable from inside a prompt or a change request. A
change request can steer *what* the model does inside ACT. It cannot make
ACT run before LOCATE, grant itself Bash during UNDERSTAND, or skip
VALIDATE — there is no field in the schema, no flag, no phrasing that
reaches that far. See [09-security.md](09-security.md) for the same
posture applied to prompt injection: repository content, including the
change request itself, is data the model reasons over, never instructions
that reconfigure how it's run.

## Why this is the trade ksforge makes

Not because a model-driven orchestrator is wrong in general — it's a
different point in the design space, optimized for flexibility across
workflow shapes the author didn't anticipate. ksforge optimizes for the
opposite: a small, fixed set of capabilities (`implement`/`review`/`fix`/
`explain`, see [05-capabilities.md](05-capabilities.md)) run unattended in
CI, where "the model decided to do something slightly different this run"
is a cost, not a feature. Fixing the loop in Rust is what makes the
`waiting_for_human` pause-and-resume mechanism
([06-human-in-the-loop.md](06-human-in-the-loop.md)) reliable across a
process boundary: resuming means re-entering a known `WorkflowState`
variant, not re-deriving what the model was doing when it stopped.
