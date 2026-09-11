# Architecture

## What ksforge is

```text
ksforge  = orchestration / product layer: change request, capability, policy,
           workflow, durable execution state, GitHub integration.

Claude Code = the default coding/agent execution engine. Owns repository
              exploration, file editing, tool execution, reasoning.

Codex CLI = the OpenAI-backed alternative execution engine, selected via
            `--engine codex`. Same job as Claude Code, spawned instead of it.

GitHub Actions = an automation runtime ksforge targets.
```

Repository exploration, file editing, tool execution, and context
management belong entirely to the spawned CLI — Claude Code by default, or
the Codex CLI via `--engine codex`. ksforge's own job is turning a change
request into a controlled request for that CLI to act on, and turning what
comes back into something trustworthy and resumable.

## Module layering

```text
domain          vocabulary: ChangeRequest, Capability, Constraint,
                 ImplementationRequest, Execution, ExecutionResult, and the
                 phase state machine (domain::workflow::WorkflowState —
                 see "The phase loop" below). No knowledge of Claude Code's
                 CLI flags or of Git.

agent            the execution-engine port (AgentExecutor) and its production
                 implementations — ClaudeCodeExecutor (spawns `claude`) and
                 CodexExecutor (spawns `codex`, the OpenAI Codex CLI),
                 selected via `--engine`.

application      the one shared pipeline (execute::run_phase_loop, used by
                 both execute::run and resume::resume) plus one Capability
                 impl per verb (implement/review/fix/explain) supplying
                 policy: the capability-specific ACT fragment, tool scope,
                 default constraints. Prompt text itself lives outside
                 Rust, under `prompts/` (see "Prompt structure" below);
                 `application::prompt` only assembles it, one function per
                 LLM phase.

workspace        filesystem concerns: resolving the workspace root,
                 dry-run isolation, change detection (content hashing,
                 no Git dependency), Execution persistence under
                 .ksforge/executions/<id>/, and the change request archive
                 (raw change request text as markdown + a JSON record, id
                 `CR-<base62>` — random, no shared counter, so concurrent
                 runs never collide) under .ksforge/change-requests/.

validation       runs user-supplied shell commands after Claude Code
                 reports success, before ksforge trusts the result.

github           the only module that knows about branches, commits, or
                 pull requests — shells out to `git`/`gh`. Never
                 referenced from `domain`.

cli              argument parsing, dispatch, output formatting. The only
                 layer that knows this is a command-line tool.

color            terminal ANSI color for stderr progress/diagnostic output
                 (`--color auto|always|never`, see docs/04-cli-reference.md).
                 Lives at the crate root rather than under `cli` because
                 `agent::claude_code` and `application::execute`'s
                 phase-progress lines use it too, and neither may depend
                 on `cli`.
```

## The phase loop

Every capability run drives `domain::workflow::WorkflowState` through a
fixed sequence — UNDERSTAND, LOCATE, ACT, VALIDATE, REPORT
(`application::execute::run_phase_loop`) — instead of trusting one Claude
Code turn to internally sequence "explore, then edit, then report" on its
own. Each `WorkflowState` variant carries every prior phase's own output,
so e.g. `Act` cannot be constructed without a `Locate` value: invalid
transitions (UNDERSTAND -> ACT, LOCATE -> REPORT, ...) have no code path
that can produce them, not merely a runtime check that rejects them.

```text
UNDERSTAND       LOCATE          ACT              VALIDATE            REPORT
   LLM             LLM           LLM         --validate: host,       host-only
 read-only       read-only   capability's    deterministic, then     ExecutionResult
   tools           tools     tool_policy()   LLM (execute-only)      assembled from
                                              unless --no-validate    prior phases
```

UNDERSTAND and LOCATE always get `ToolPolicy::ReadOnly` — regardless of the
capability, including `implement`/`fix`. ACT gets the capability's own
`tool_policy()` (`ReadWrite` for implement/fix, `ReadOnly` for
review/explain), unchanged from before this phase loop existed. This is
the actual security boundary the previous single-turn design lacked: a
write-capable capability's tool grant used to cover the *entire* turn from
its first token, so nothing stopped the agent from editing files while
nominally still "exploring." Now it structurally cannot, until ACT.

VALIDATE is two independent layers, both against the real filesystem
result of ACT, not the model's own claim about it. Layer 1: the user's
exact `--validate` commands, if given, run deterministically via
`validation::run` — no agent turn, no model judgment, unchanged from the
original design. Layer 2: unless `--no-validate`, a further LLM turn
(`PermissionMode::ExecuteOnly` — Bash allowed, Edit/Write not even offered
as tools) works out *what* actually needs checking for this specific
change and does it — see `prompts/phases/validate.md`. Only the fact that
this turn runs is host-controlled; ksforge does not itself decide *how* to
validate a project (deliberately: hardcoding a command per project type
does not scale across a workspace with many different kinds of repos).
`application::execute` diffs the workspace before/after this turn and
fails the run if anything changed — the `ExecuteOnly` restriction is
enforced twice, once by the executor's own permissions and once
deterministically by ksforge, since executor-side enforcement alone is
never fully trusted (same principle as the ACT-phase `ChangeScope` check).
REPORT runs no agent turn: plain Rust code assembling `ExecutionResult`
from what UNDERSTAND/LOCATE/ACT/VALIDATE already produced.

A human gate (`waiting_for_human`) can be raised on any LLM phase turn
when the capability supports it (`supports_human_interaction()`, unchanged
gate). `Execution.workflow` records exactly which phase paused, so
`resume()` re-enters that same phase — not the start of the run — passing
the decision back to it (`application::resume::resume`, via
`application::execute::run_phase_loop` with a decision suffix appended to
that phase's prompt, and `--resume <session-id>` for conversation
continuity across the hop).

## Prompt structure

Modeled on the sibling tool `tools/palau-test` (Core + phase prompts,
compiled in rather than read from disk — see below for why): a stable,
capability-independent Core, plus one prompt per LLM phase, assembled by
`application::prompt` (`for_understand`/`for_locate`/`for_act`) instead of
one function building a single mega-prompt.

```text
prompts/
  system-prompt.md              Core (§dNtuHSf/§w9XFYHn/§eK8ihEp/§5xd9ep5/
                                 §CrgIKI1/§czrqTgL/§aUIxFPP/§kNp69Vp):
                                 identity, authority/prompt-injection
                                 posture, the phase loop as a structural
                                 fact rather than a rule to follow, the
                                 absolute "never" list. Capability-
                                 independent and loaded on every LLM turn.
  phases/
    understand.md                §NNbADAM: determine what the change
                                  request needs and its rough scope.
    locate.md                    §Ijk08ZT: what to inspect — build/test
                                  tooling, existing code, tests,
                                  conventions, config, docs.
    act.md                       §ENxR14E/§Lfxkhkv/§hQnJPKM: prefer
                                  existing code, code quality, the agent's
                                  own build/test/lint pass (only ACT ever
                                  has Bash). Loaded together with the
                                  capability's own fragment below.
  fragments/
    human-in-the-loop.md         §RCOjEeb/§7XOwIrp: the waiting_for_human
                                  protocol — appended to every LLM turn
                                  (Understand/Locate/Act) only when
                                  Capability::supports_human_interaction()
                                  is true (review/explain never see it).
    schema-reminder.md            §EFNZe8X: a short "respond matching the
                                  schema" reminder shared by all three LLM
                                  phases — the shape itself is enforced by
                                  `--json-schema`, unchanged mechanism.
  capabilities/
    implement.md, review.md,
    fix.md, explain.md          One capability-specific instruction
                                 fragment each, loaded only on the ACT
                                 turn; `Capability::prompt_fragment` is
                                 just `include_str!` of its own file.
  coordinator/
    system-prompt.md            A separate Core for `ksforge coordinate`
                                 (`application::coordinate`) — its own
                                 role (analyze overlap between concurrent
                                 executions, never implement anything),
                                 not one more capability fragment, so it
                                 shares neither this Core nor the phase
                                 prompts above with implement/review/
                                 fix/explain.
```

These are embedded into the binary at compile time via `include_str!`
(`application::prompt.rs`), not read from disk at runtime — unlike
`palau-test`, ksforge ships as a single binary with no accompanying files
(see `.github/workflows/release.yml`), so runtime file reads would break
for anyone using the release tarball. Editing a prompt still just means
editing a `.md` file; it takes effect on the next `cargo build`.

One deliberate looseness: `domain::capability::ExecutionContext` (which a
`Capability::execute` takes) holds an `Arc<dyn AgentExecutor>` — so `domain`
does depend on the `agent` port. Pure hexagonal layering would put that
composition type in `application` instead, but for a CLI this size that
indirection didn't earn its keep; the trait it depends on is a port
(interface), not a concrete implementation, which is the distinction that
actually matters here.

## The Claude Code subprocess contract

Every LLM phase turn — UNDERSTAND, LOCATE, ACT, and VALIDATE when
`agent_review` is on, each one a separate subprocess invocation (see "The
phase loop" above) — spawns: `claude -p
--output-format stream-json --verbose --json-schema <schema> --tools
<policy> --permission-mode <mode> --permission-prompts none
[--allowedTools Bash,PowerShell] --model <name> [--append-system-prompt]
[--max-budget-usd] [--resume <session-id>] [--mcp-config <path>
--strict-mcp-config] "<prompt>"`, with the workspace (or its isolated
dry-run copy) as the working directory — the same working directory
across every turn of one `Execution`.

- **`--tools`**: UNDERSTAND and LOCATE always get `Read,Grep,Glob` only,
  for every capability, including `implement`/`fix`. ACT gets the
  capability's own policy: `Read,Grep,Glob` for `review`/`explain`, the
  default set (no `--tools` flag) for `implement`/`fix`. VALIDATE always
  gets `Read,Grep,Glob,Bash,PowerShell` — execution allowed, but Edit/Write
  are never in the list, regardless of capability (`application::execute::
  validate_tools_and_permission`).
- **`--model`**: always passed — ksforge defaults it to `sonnet` itself
  (`--model` CLI default / `model` action input default) rather than
  leaving it unset and deferring to Claude Code's own default, so ksforge's
  cost/behavior doesn't shift silently if that default ever changes.
- **`--permission-mode` / `--permission-prompts none`**: never blocks
  waiting for an interactive answer nobody can give in CI —
  `acceptEdits` for write-capable capabilities, `plan` for read-only ones,
  `default` (plus the `--allowedTools` pre-approval below) for VALIDATE,
  and any prompt that would still require a human is auto-denied rather
  than hanging the process.
- **`--allowedTools Bash,PowerShell`**: added alongside `acceptEdits` (the
  ACT turn) and alongside VALIDATE's `default` mode. `acceptEdits`
  pre-approves Edit/Write-family tools but *not* the shell tool —
  confirmed against a real run where an `implement` turn needing `cargo
  build`/`cargo test` for `prompts/phases/act.md`'s mandatory validation
  step had that shell call auto-denied (nobody to answer the prompt), and
  the agent correctly refused to claim `completed` without a validation
  run it couldn't execute, rather than fabricating one. Both `Bash` and
  `PowerShell` are pre-approved — confirmed against a real run on Windows
  that Claude Code invokes shell commands via a distinct `PowerShell` tool
  there, not `Bash`, so allowing only `Bash` left every shell call
  auto-denied on Windows the same way. This still stops short of
  `--permission-mode bypassPermissions` (Claude Code's own docs:
  "recommended only for sandboxes with no internet access" — too broad for
  ksforge's typical CI runner, which does have internet access). The Codex
  CLI has no equivalent gap: its `workspace-write` sandbox already covers
  shell execution the same way it covers file edits (see
  `agent::codex::CodexExecutor`'s own doc comment) — but also has no way to
  permit shell without also permitting edits, unlike Claude Code's
  `--tools`, so under `--engine codex` the VALIDATE turn's Edit/Write
  restriction relies entirely on `application::execute`'s post-turn
  workspace diff, not on the executor's own sandbox.
- **`--json-schema`**: constrains every phase turn to the same flat
  `AgentOutcome` shape (`status`, `title?`, `summary`, `changed_files?`,
  `scope?`/`relevant_files?`/`existing_abstractions?`/`existing_tests?`/
  `conventions?` — only the fields the current phase's prompt asks for are
  populated — `question?`, `options?`, `failure_reason?`, ...), `status`
  one of `completed` / `waiting_for_human` / `failed` — see
  [06-human-in-the-loop.md](06-human-in-the-loop.md) for why this is the
  mechanism for "Claude Code needs a decision," not a heuristic text
  convention.
- **`--mcp-config` / `--strict-mcp-config`**: only added when `--mcp-config
  <path>` was passed to ksforge (see docs/11-integrations.md); the path is
  passed through unmodified — ksforge does not parse or validate that
  file's contents, it's Claude Code's own schema, not a ksforge one.

- **`--output-format stream-json` / `--verbose`**: `claude_code.rs` reads
  Claude Code's stdout one NDJSON line at a time as the subprocess runs
  (rather than buffering the whole turn and parsing one envelope at the
  end, as the earlier `--output-format json` did) and prints a short
  progress line per interesting event — which tool is being called and
  with what (`Bash: cargo test`, `Edit: src/lib.rs`, ...), plus any
  free-text commentary — to stderr as it happens. This is what lets a long
  ACT turn show what it's actually doing instead of going silent until it
  finishes. `--verbose` is required by Claude Code whenever `-p` and
  `--output-format stream-json` are combined; without it the process
  refuses to start. The turn's final result is still the `type: "result"`
  event at the end of the stream, structurally the same
  `{"result": "...", "session_id": "...", "is_error": false, ...}`
  envelope the old `--output-format json` mode returned directly.

**What's verified vs. inferred**: the flags above were confirmed against a
real `claude --help` on the machine this was built on. The `--print
--output-format json` **envelope shape** (`{"result": "...", "session_id":
"...", "is_error": false, ...}`) was independently confirmed with a live
`ksforge implement` run in this environment, including the specific case
this phase loop newly depends on: a run that paused with `waiting_for_human`
during UNDERSTAND, resumed via `ksforge resume`, and continued through
LOCATE and ACT via `--resume <session-id>` (a 2-hop chain, not previously
exercised when a paused turn was always the *last* turn of a run) all the
way to a real `completed` REPORT with files written to disk.

The `stream-json` NDJSON event shapes above (`system`/`assistant`/
`user`/`result`, and the `message.content` block shapes within them) are
*not yet* independently re-confirmed the same way — they come from
general Claude Code CLI knowledge, not a captured live transcript from
this exact installed version. `claude_code.rs`'s parsing is deliberately
tolerant of anything unrecognized (an unknown event `type`, a missing
field, a block shape it doesn't special-case) so a mismatch here degrades
to "no progress line printed for that event," never a parse failure of
the turn itself — the final `result` event is all `execute` actually
depends on to produce an `AgentResult`.

## Change detection has no Git dependency

`workspace::snapshot` content-hashes every file under the workspace root
before and after the ACT turn (the only phase that can write) and diffs
the two maps. This is deliberate: Claude Code's own report of what it
changed (`AgentOutcome.changed_files`) is treated as advisory only, never
as ground truth — the filesystem is the source of truth for "what
changed," and `domain::workflow::ActionKind` (Modify/Findings/Explain/
NoChange) is derived from this diff plus the capability id, never
self-reported by the model either. It also means ksforge works in a plain
directory with no `.git` at all.

## `av`

The broader Kosmos ecosystem has a separate artifact-transformation/
variation concept (`av`). ksforge does not depend on it — no such crate
was available to integrate against for this first version — but nothing
here forecloses it: `Artifact`-shaped concepts stay confined to
`ExecutionResult.changed_files` (plain paths) rather than a richer
artifact/lineage model that would need reworking later.
