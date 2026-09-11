# Security

## Prompt injection

Repository content — file contents, issue/PR text, comments, the change
request itself — is treated as **data**, never as instructions. The system
prompt ksforge constructs (`application::prompt::system_prompt`) says so
explicitly and is sent as Claude Code's system prompt, which takes
precedence over anything found while it explores the repository. This
matters most in public repositories, where an issue or PR body is
attacker-controlled text that a workflow might feed in as (part of) a
change request.

Concretely:

- ksforge's own policy (constraints, capability instructions) is
  assembled server-side (inside ksforge, before the subprocess is spawned)
  and passed via `--append-system-prompt`, not interpolated into the
  change request text where it could be confused with attacker content.
- The change request and constraints are explicit, separate structures
  (`ImplementationRequest.change_request` / `.constraints`) all the way
  through — nothing merges arbitrary repository text into the constraint
  list.
- For public repositories, require explicit authorization before a
  workflow can modify code or open a PR. The primary run (`ksforge
  implement ...`) should stay `workflow_dispatch`-gated (manual, already
  requires repo write access to trigger) — the two-run resume pattern in
  [07-github-actions.md](07-github-actions.md) does this.
- A **paused** execution's decision may additionally be resumed via
  `issue_comment` (`/ksforge choose <option>`, §pewY5yG of the
  human-in-the-loop spec), which *does* fire on externally-triggerable
  content — this is deliberately allowed, but only with defense in depth,
  since a single check either side could omit is not enough on its own:
  1. **Workflow-level filter**: the example workflow in
     [07-github-actions.md](07-github-actions.md) gates its job on
     `github.event.comment.author_association` (`OWNER`/`MEMBER`/
     `COLLABORATOR` only) — a fast, cheap rejection of most noise.
  2. **ksforge's own re-check**: `ksforge handle-comment` independently
     calls `gh api repos/{owner}/{repo}/collaborators/{login}/permission`
     (`github::decision::authorize_commenter`) and only accepts `admin`/
     `write`. This does not trust the workflow's own `if:` — a commenter
     whose association changed, or a misconfigured `if:`, is still caught
     here. A failed/absent permission lookup is treated as unauthorized,
     never as an error that lets the decision through.
  3. **Gate/option validation**: the comment must name a currently-open
     gate's execution and one of its actual options — an arbitrary comment
     that merely resembles the command is rejected (§pewY5yG: "never
     resume solely because a comment contains text resembling a decision").
  4. **Idempotency**: the GitHub comment's numeric id is recorded once
     acted on (`ExecutionStore::mark_event_processed`), so a duplicate
     webhook delivery for the same comment is a no-op, not a second resume
     (§DlruVSP).

  What a `/ksforge choose` comment can *never* do: start a new execution,
  choose an arbitrary capability, or bypass `--validate`/`--create-pull-request`
  policy — it only supplies the `option` id `ksforge resume` needs, on an
  execution and gate that already exist.

- A `/ksforge implement <change-request>` or `/ksforge fix <change-request>`
  PR comment (see
  "Comment-driven follow-up" in [07-github-actions.md](07-github-actions.md))
  is a materially bigger trust step than `/ksforge choose`: it starts a
  brand new, write-capable, billed run from **arbitrary comment text**,
  not a pick among options the agent itself already offered. The same
  defense-in-depth list applies (workflow-level `author_association`
  filter, `authorize_commenter` re-check, per-comment idempotency), but
  here the authorization check is the *only* thing standing between an
  externally-triggerable event and a real agent run with write access —
  there is no bounded option set to also validate against, unlike
  `/ksforge choose`. This is why `parse_follow_up_command`
  (`github::decision`) requires the trigger to be the whole of its own
  comment line rather than a substring anywhere, and why the
  authorization check runs *before* anything is executed, not after.
  What it still cannot do: the resulting run goes through the exact same
  system prompt, authority hierarchy, and "repository content is data,
  not instructions" posture as any other run (see "Prompt injection"
  above) — a comment is no more able to override ksforge's own policy
  than a `--change` argument is.

## Least privilege

Request `contents: write` + `pull-requests: write` only for workflows that
actually create a PR (`create-pull-request: "true"`); `review`/`explain`
workflows need only `contents: read` (+ `pull-requests: read` if reviewing
a PR's diff). Never `permissions: write-all`. See
[07-github-actions.md](07-github-actions.md) for both shapes, including
the separate repo-level "Allow GitHub Actions to create and approve pull
requests" setting that YAML-level `permissions:` cannot substitute for.

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

## Integrations (`--mcp-config`)

`--mcp-config <path>` (see [11-integrations.md](11-integrations.md)) is
always paired with `--strict-mcp-config`, so a run only ever gets the MCP
servers explicitly named in that file — never anything a user- or
project-level Claude Code config might otherwise contribute. ksforge does
not parse, validate, or filter the file's contents (including any `env`
map inside it); that trust boundary is the same one that already applies
to the workspace and change request text the file lives next to, not a new one.

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
