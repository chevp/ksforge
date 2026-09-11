# CLI reference

```text
ksforge                                                                   [chat options]
ksforge chat                                                              [chat options]
ksforge implement --change "..." | --change @<path>  [options]
ksforge review     --change "..." | --change @<path>  [options]
ksforge fix        --change "..." | --change @<path>  [options]
ksforge explain    --change "..." | --change @<path>  [options]
ksforge txt2img    --change "..." | --change @<path>  [options]
ksforge img2img    --change "..." | --change @<path>  [options]
ksforge test       --change "..." | --change @<path>  [options]
ksforge resume <execution-id> --decision <option-id> [--decided-by <who>] [options]
ksforge status [<execution-id>] [--workspace <path>] [--format text|json]
ksforge cancel <execution-id> [--reason <text>] [--workspace <path>] [--format text|json]
ksforge post-report <execution-id> --pr <number> [--workspace <path>]
ksforge handle-comment [--execution-id <id>] --comment-id <id> --commenter <login> --body <text> [--workspace <path>]
ksforge coordinate --change "..." | --change @<path>  [--workspace <path>] [--engine ...] [--model ...] [--format text|json]
ksforge capabilities
```

`ksforge status` with no `<execution-id>` is a workspace-wide overview
instead of one execution's detail: every git repo found under `--workspace`
(a single repo, or a plain folder holding several — see
[12-interactive-chat.md](12-interactive-chat.md)), each one's branch/dirty
state, and its active (`running`/`waiting_for_human`) executions — the
at-a-glance view for orchestrating many concurrently running repos.

`--change` takes the change request text directly, or `@<path>` to read it
from a file instead (the same convention `curl`/`gh` use for "this value,
or a file holding it") — one flag either way, not a second
`--change-file`.

`--color auto|always|never` (default `auto`) is global — it works before or
after the subcommand, e.g. both `ksforge --color always implement ...` and
`ksforge implement --color always ...`. `auto` colors stderr's
phase-progress lines (`== Understand: ... ==`, `[claude] -> Read: ...`,
...) — never stdout, so `--format json`/`--format text` output is never
polluted — when stderr is a real terminal, or when `GITHUB_ACTIONS=true`
(GitHub's log viewer renders ANSI codes even though a step's own stderr
isn't a tty). `NO_COLOR` disables unconditionally; `FORCE_COLOR`/
`CLICOLOR_FORCE` force it on. See `crate::color` for the exact precedence.

`coordinate` reports how a new change request relates to the workspace's
other active (`running`/`waiting_for_human`) executions — overlap risk,
dependencies, a recommendation — without implementing anything itself
(`prompts/coordinator/system-prompt.md`). Read-only, and it never starts,
resumes, or blocks an `Execution`; it always exits `0` on a successful
analysis regardless of what it recommends — a caller that wants to gate on
the recommendation reads `proceed`/`classification` from `--format json`.
Deliberately narrower than the capability commands above: no
`--dry-run`/`--validate`/`--create-pull-request`/`--push-to-branch`/
`--mcp-config`, since this command never writes.

`post-report`/`handle-comment` are the GitHub comment round-trip (section
13-17 of the human-in-the-loop spec — see
[06-human-in-the-loop.md](06-human-in-the-loop.md) and
[07-github-actions.md](07-github-actions.md)). `handle-comment` recognizes
two comment shapes and never itself acts on either:
- `/ksforge choose <option>` (requires `--execution-id`): validates it
  against that execution's open gate and prints the option id — chain
  with `ksforge resume`.
- `/ksforge implement <change-request>` / `/ksforge fix <change-request>`
  (no `--execution-id`): prints the capability (stdout) and the change
  request (`$GITHUB_OUTPUT`'s `change_request` key) — chain with `ksforge
  implement`/`ksforge fix --push-to-branch`, see "Comment-driven
  follow-up" in [07-github-actions.md](07-github-actions.md).

`ksforge chat` — also what bare `ksforge` (no subcommand) runs, with every
flag at its default — is a conversational front end over the same
capability pipeline: free text (or a `review:`/`fix:`/`explain:`-prefixed
line) runs one `Execution` per turn, waiting-for-human gates are answered
inline, and a completed turn with changes is branched, committed, and
optionally merged into `--base-branch` locally (no `gh`, no push, no PR).
See [12-interactive-chat.md](12-interactive-chat.md) for the full session
flow and its own option table (`--workspace`, `--engine`, `--model`,
`--max-budget-usd`, `--claude-path`/`--codex-path`, `--dry-run`,
`--validate`, `--base-branch`, `--mcp-config` — a subset of the flags
below; chat has no `--format`/`--create-pull-request`/`--push-to-branch`).
`ksforge chat` requires `--workspace` to be a git repository, unlike the
capability commands below.

## Options shared by `implement`/`review`/`fix`/`explain`/`txt2img`/`img2img`/`test`/`resume`

| Flag | Default | Meaning |
|---|---|---|
| `--workspace <path>` | `.` | Workspace root. No GitHub token needed locally. |
| `--engine claude\|codex` | `claude` | Which coding agent CLI to spawn: Claude Code, or the OpenAI Codex CLI. |
| `--model <name>` | `sonnet` for `--engine claude`; Codex's own default for `--engine codex` | Alias (`sonnet`) or full name (`claude-sonnet-5`) for Claude Code, or a Codex model name. For `claude`, overrides ksforge's own default, not just Claude Code's. |
| `--max-budget-usd <n>` | none | Passed through to Claude Code's own budget cap. Claude Code only — `--engine codex` with this set fails the run (no Codex CLI equivalent). |
| `--claude-path <path>` | PATH lookup | Also settable via `KSFORGE_CLAUDE_PATH`. |
| `--codex-path <path>` | PATH lookup | Also settable via `KSFORGE_CODEX_PATH`. Only used with `--engine codex`. |
| `--dry-run` | off | Isolated temp copy; never writes to the real workspace. |
| `--validate <cmd>` | none | Repeatable. Runs after success, before trusting the result. |
| `--format text\|json` | `text` | `json` mode: stdout is exactly one JSON object. |
| `--create-pull-request` | off | Opens a PR via `gh` if the run completed with changes. Conflicts with `--push-to-branch`. |
| `--base-branch <name>` | `main` | Base branch for `--create-pull-request`. |
| `--push-to-branch` | off | Commits and pushes to the already-checked-out branch instead of opening a new PR — for a comment-driven follow-up run against an existing PR's branch. Conflicts with `--create-pull-request`. |
| `--mcp-config <path>` | none | Passed through to Claude Code as `--mcp-config <path> --strict-mcp-config` — see [11-integrations.md](11-integrations.md). Claude Code only — `--engine codex` with this set fails the run (no Codex CLI equivalent). |

## Waiting for human input: console vs. CI

`implement`/`fix`/`test`/`resume` (any capability with
`supports_human_interaction()`) behave differently depending on whether
stdin *and* stdout are both real terminals
(`std::io::IsTerminal`/`cli::output::is_interactive`):

- **Interactive console**: on `waiting_for_human`, ksforge prints the
  question/options as usual, then prompts right there (`Decision: `),
  validates the answer against the offered option ids (re-prompting on a
  bad one), and resumes immediately in the same process — repeating for as
  many gates as the run raises. Ctrl-D/Ctrl-Z (EOF) leaves the gate open
  instead, resumable later the normal way.
- **Anything else (a GitHub Actions run, a piped/redirected invocation,
  `> file`, ...)**: unchanged — the run exits `6` and stays paused. A CI
  runner's stdio is never a tty, so a workflow always takes this path, and
  the input for the resumed run comes from a PR comment instead (see
  [06-human-in-the-loop.md](06-human-in-the-loop.md) and
  [07-github-actions.md](07-github-actions.md) for that round trip).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Completed successfully. |
| `1` | General/application failure (workspace error, GitHub integration error, unknown execution id, cancelled). |
| `2` | Invalid CLI usage or configuration (e.g. `--change` missing, or its `@<path>` file unreadable). |
| `3` | The selected `--engine` CLI is unavailable, or its execution failed/produced unparseable output. |
| `4` | A `--validate` command failed. |
| `5` | Reserved for policy/safety-violation failures (not currently raised by any built-in capability, but part of the stable taxonomy — see `KsforgeError::PolicyViolation`). |
| `6` | The execution paused with `status: waiting_for_human` — not an error. Resume it with `ksforge resume <id> --decision <option-id>`. |

## `--format json` shape

```json
{
  "execution_id": "ksf_2f8b6a1c9d3e4f5a8b7c6d5e4f3a2b1c",
  "capability": "implement",
  "status": "completed",
  "pending_question": null,
  "result": {
    "success": true,
    "title": "Add password reset via email",
    "summary": "Implemented password reset via email.",
    "changed_files": ["src/auth.rs", "src/password_reset.rs"],
    "validation": { "passed": true, "commands": [] }
  },
  "artifacts": ["src/auth.rs", "src/password_reset.rs"]
}
```

`status: "waiting_for_human"` has `result: null` and a populated
`pending_question: {execution_id, question, options: [{id, label}],
required}`. Diagnostics and progress notices always go to stderr, in both
`text` and `json` mode.
