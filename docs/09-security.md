# Security

## Prompt injection

Repository content — file contents, issue/PR text, comments, the user
story itself — is treated as **data**, never as instructions. The system
prompt ksforge constructs (`application::prompt::system_prompt`) says so
explicitly and is sent as Claude Code's system prompt, which takes
precedence over anything found while it explores the repository. This
matters most in public repositories, where an issue or PR body is
attacker-controlled text that a workflow might feed in as (part of) a
story.

Concretely:

- ksforge's own policy (constraints, capability instructions) is
  assembled server-side (inside ksforge, before the subprocess is spawned)
  and passed via `--append-system-prompt`, not interpolated into the user
  story text where it could be confused with attacker content.
- The story and constraints are explicit, separate structures
  (`ImplementationRequest.story` / `.constraints`) all the way through —
  nothing merges arbitrary repository text into the constraint list.
- For public repositories, require explicit authorization before a
  workflow can modify code or open a PR: gate `workflow_dispatch` (manual,
  already requires repo write access to trigger) rather than wiring
  ksforge to `issues`/`issue_comment` events that fire on
  externally-triggerable content without a maintainer's explicit action in
  between. The two-run resume pattern in
  [07-github-actions.md](07-github-actions.md) is `workflow_dispatch`-only
  for this reason.

## Least privilege

Request `contents: write` + `pull-requests: write` only for workflows that
actually create a PR (`create-pull-request: "true"`); `review`/`explain`
workflows need only `contents: read` (+ `pull-requests: read` if reviewing
a PR's diff). Never `permissions: write-all`. See
[07-github-actions.md](07-github-actions.md) for both shapes.

## Secrets

- ksforge **never reads `ANTHROPIC_API_KEY` itself.** There is no
  `--anthropic-api-key` flag and no code path that touches that
  environment variable — it simply stays in the process environment,
  which the spawned `claude` subprocess inherits automatically (the
  default behavior of `tokio::process::Command`). This is the smallest
  surface area: nothing in ksforge can log, forward, or embed a key it
  never held. Claude Code owns its own authentication entirely, per its
  own docs (`ANTHROPIC_API_KEY`, OAuth, or an enterprise
  gateway/OIDC/workload-identity setup — none of that is ksforge's
  concern).
- ksforge does not log full command lines or environment when a Claude
  Code invocation fails; error messages carry Claude Code's own stderr/
  stdout tail (truncated), not the process environment.
- `github::pull_request` shells out to `gh`, which handles its own token
  (`GH_TOKEN`/`GITHUB_TOKEN` or `gh auth login` state) the same way
  outside of ksforge — no token handling lives in ksforge's own code.

## Command execution

The only user-controlled command execution is `--validate` (see
[08-validation.md](08-validation.md)) and `git`/`gh` invocations inside
`github::pull_request`, both of which run fixed, ksforge-authored argument
lists (not arbitrary strings assembled from model output). Claude Code's
own tool execution (Bash, Edit, etc.) is scoped by `--tools`/
`--permission-mode` per capability (see
[03-architecture.md](03-architecture.md)) — ksforge does not attempt to
re-sandbox what Claude Code already sandboxes; it restricts which tools
are offered at all, then verifies the *result* (the filesystem diff)
rather than policing each tool call.
