# CLI reference

```text
ksforge implement --change-request "..." | --change-request-file <path>  [options]
ksforge review     --change-request "..." | --change-request-file <path>  [options]
ksforge fix        --change-request "..." | --change-request-file <path>  [options]
ksforge explain    --change-request "..." | --change-request-file <path>  [options]
ksforge resume <execution-id> --decision <option-id> [--decided-by <who>] [options]
ksforge status <execution-id> [--workspace <path>] [--format text|json]
ksforge cancel <execution-id> [--reason <text>] [--workspace <path>] [--format text|json]
ksforge post-report <execution-id> --pr <number> [--workspace <path>]
ksforge handle-comment [--execution-id <id>] --comment-id <id> --commenter <login> --body <text> [--workspace <path>]
ksforge coordinate --change-request "..." | --change-request-file <path>  [--workspace <path>] [--engine ...] [--model ...] [--format text|json]
ksforge capabilities
```

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

There is today no `ksforge chat`, `ksforge agent`, or `ksforge ask` — this
has deliberately not been a chatbot wrapper so far. An interactive chat
mode (bare `ksforge`, no subcommand) is proposed but not yet implemented —
see [12-interactive-chat.md](12-interactive-chat.md) for the design.

## Options shared by `implement`/`review`/`fix`/`explain`/`resume`

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

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Completed successfully. |
| `1` | General/application failure (workspace error, GitHub integration error, unknown execution id, cancelled). |
| `2` | Invalid CLI usage or configuration (e.g. neither `--change-request` nor `--change-request-file`). |
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
