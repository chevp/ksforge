# Architecture

## What ksforge is, and isn't

```text
ksforge  = orchestration / product layer: story, capability, policy,
           workflow, durable execution state, GitHub integration.

Claude Code = the default coding/agent execution engine. Owns repository
              exploration, file editing, tool execution, reasoning.

Codex CLI = the OpenAI-backed alternative execution engine, selected via
            `--engine codex`. Same job as Claude Code, spawned instead of it.

GitHub Actions = an automation runtime ksforge targets. Not the domain.
```

ksforge does not implement a coding agent, a repository indexer, a
tool-calling loop, or a context manager. All of that is the spawned CLI's
job — Claude Code by default, or the Codex CLI via `--engine codex`.
ksforge's job is turning a user story into a controlled request for that
CLI to act on, and turning what comes back into something trustworthy and
resumable.

## Module layering

```text
domain          vocabulary: UserStory, Capability, Constraint,
                 ImplementationRequest, Execution, ExecutionResult.
                 No knowledge of Claude Code's CLI flags or of Git.

agent            the execution-engine port (AgentExecutor) and its production
                 implementations — ClaudeCodeExecutor (spawns `claude`) and
                 CodexExecutor (spawns `codex`, the OpenAI Codex CLI),
                 selected via `--engine`.

application      the one shared pipeline (execute::run, resume::resume)
                 plus one Capability impl per verb (implement/review/
                 fix/explain) supplying policy: prompts, tool scope,
                 default constraints. Prompt text itself lives outside
                 Rust, under `prompts/` (see "Prompt structure" below);
                 `application::prompt` only assembles it.

workspace        filesystem concerns: resolving the workspace root,
                 dry-run isolation, change detection (content hashing,
                 no Git dependency), Execution persistence under
                 .ksforge/executions/<id>/, and the story archive
                 (raw story text as markdown + a JSON record, id
                 `US-<base62>` — random, no shared counter, so concurrent
                 runs never collide) under .ksforge/user-stories/.

validation       runs user-supplied shell commands after Claude Code
                 reports success, before ksforge trusts the result.

github           the only module that knows about branches, commits, or
                 pull requests — shells out to `git`/`gh`. Never
                 referenced from `domain`.

cli              argument parsing, dispatch, output formatting. The only
                 layer that knows this is a command-line tool.
```

## Prompt structure

Modeled on the sibling tool `tools/palau-test` (Core + policies, compiled
in rather than read from disk — see below for why): a stable,
capability-independent Core plus depth split into policy files, loaded
together on every run by `application::prompt::build`.

```text
prompts/
  system-prompt.md              Core, §1-§8: identity, authority/prompt-
                                 injection posture, the UNDERSTAND->LOCATE->
                                 CHANGE->VALIDATE->REPORT loop, the absolute
                                 "never" list. Capability-independent and
                                 always loaded first.
  policies/
    repository-analysis.md      §10-§11: what to inspect, existing code
                                 over new code.
    validation-and-output.md    §20-§22: the validation workflow and the
                                 exact JSON output schema.
  fragments/
    human-in-the-loop.md        §23: the waiting_for_human protocol —
                                 appended only when
                                 Capability::supports_human_interaction()
                                 is true (review/explain never see it).
  capabilities/
    implement.md, review.md,
    fix.md, explain.md          One capability-specific instruction
                                 fragment each; `Capability::prompt_fragment`
                                 is just `include_str!` of its own file.
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

Every agent turn: `claude -p --output-format json --json-schema <schema>
--tools <policy> --permission-mode <mode> --permission-prompts none
--model <name> [--append-system-prompt] [--max-budget-usd] [--resume
<session-id>] [--mcp-config <path> --strict-mcp-config] "<prompt>"`, with
the workspace (or its isolated dry-run copy) as the working directory.

- **`--tools`**: `review`/`explain` get `Read,Grep,Glob` only; `implement`/
  `fix` get the default set (no `--tools` flag passed).
- **`--model`**: always passed — ksforge defaults it to `sonnet` itself
  (`--model` CLI default / `model` action input default) rather than
  leaving it unset and deferring to Claude Code's own default, so ksforge's
  cost/behavior doesn't shift silently if that default ever changes.
- **`--permission-mode` / `--permission-prompts none`**: never blocks
  waiting for an interactive answer nobody can give in CI —
  `acceptEdits` for write-capable capabilities, `plan` for read-only ones,
  and any prompt that would still require a human is auto-denied rather
  than hanging the process.
- **`--json-schema`**: constrains Claude Code's final turn to a flat
  `{status, title?, summary, changed_files?, question?, options?,
  failure_reason?}` shape (`status` one of `completed` /
  `waiting_for_human` / `failed`) — see
  [06-human-in-the-loop.md](06-human-in-the-loop.md) for why this is the
  mechanism for "Claude Code needs a decision," not a heuristic text
  convention.
- **`--mcp-config` / `--strict-mcp-config`**: only added when `--mcp-config
  <path>` was passed to ksforge (see docs/11-integrations.md); the path is
  passed through unmodified — ksforge does not parse or validate that
  file's contents, it's Claude Code's own schema, not a ksforge one.

**What's verified vs. inferred**: the flags above were confirmed against a
real `claude --help` on the machine this was built on. The `--print
--output-format json` **envelope shape** (`{"result": "...", "session_id":
"...", "is_error": false, ...}`) was *not* independently confirmed with a
live `claude -p` invocation in this environment (that would spend real API
budget from inside an unattended build) — it matches Claude Code's
documented convention, and `ClaudeCodeExecutor`'s envelope parser ignores
unknown fields so a version drift there degrades gracefully rather than
hard-failing, but do one real smoke test (`ksforge implement --story "..."
--dry-run` against a throwaway repo) before trusting this in CI.

## Change detection has no Git dependency

`workspace::snapshot` content-hashes every file under the workspace root
before and after a Claude Code turn and diffs the two maps. This is
deliberate: Claude Code's own report of what it changed
(`AgentOutcome.changed_files`) is treated as advisory only, never as
ground truth — the filesystem is the source of truth for "what changed."
It also means ksforge works in a plain directory with no `.git` at all.

## `av`

The broader Kosmos ecosystem has a separate artifact-transformation/
variation concept (`av`). ksforge does not depend on it — no such crate
was available to integrate against for this first version — but nothing
here forecloses it: `Artifact`-shaped concepts stay confined to
`ExecutionResult.changed_files` (plain paths) rather than a richer
artifact/lineage model that would need reworking later.
