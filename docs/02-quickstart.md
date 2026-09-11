# Quickstart

```bash
export ANTHROPIC_API_KEY=...   # or however Claude Code auth is configured

cd your-project
ksforge implement --change-request "As a user, I want to reset my password via email."
```

What happens:

1. ksforge resolves your current directory as the workspace and starts a
   new `Execution`.
2. It spawns `claude -p` with a system prompt carrying ksforge's policy
   (constraints, capability instructions) and a user prompt carrying your
   change request, and lets Claude Code explore and edit the workspace
   directly.
3. Claude Code either finishes (and ksforge diffs the workspace to see what
   changed) or reports it needs a decision — see below.
4. If you passed `--validate`, ksforge runs those commands before trusting
   the result.
5. ksforge prints what changed and exits with a status-specific code (see
   [04-cli-reference.md](04-cli-reference.md)).

## Dry run

```bash
ksforge implement --change-request "..." --dry-run
```

Runs Claude Code against an isolated temporary copy of your workspace.
Nothing is written back — see [08-validation.md](08-validation.md) and
[03-architecture.md](03-architecture.md) for exactly what "isolated" means.

## When it needs a decision

```text
Execution: ksf_2f8b6a1c9d3e4f5a8b7c6d5e4f3a2b1c

Status:
  Waiting for human input

Question:
  Which authentication mechanism should be implemented?

Options:
  oauth2 - OAuth 2
  jwt - JWT

Resume:
  ksforge resume ksf_2f8b6a1c9d3e4f5a8b7c6d5e4f3a2b1c --decision <option-id>
```

The process exits (code `6`) rather than waiting — nothing is blocked on
you answering right now. Come back whenever and:

```bash
ksforge resume ksf_2f8b6a1c9d3e4f5a8b7c6d5e4f3a2b1c --decision oauth2
```

Full depth on this in [06-human-in-the-loop.md](06-human-in-the-loop.md).

## Other capabilities

```bash
ksforge review .                       # findings only, never writes
ksforge explain src/auth.rs            # explanation only, never writes
ksforge fix --change-request "Login times out after 30s under load"
ksforge coordinate --change-request "..." # overlap/conflict risk vs. active executions, never writes
ksforge capabilities                   # list what's available
```

## Interactive chat (proposed)

Running bare `ksforge` (no subcommand) inside a local git repository is
designed to start a conversational session over this same pipeline —
branching, committing, and merging change requests back to `main` locally,
confirmed before each merge. Not implemented yet; see
[12-interactive-chat.md](12-interactive-chat.md) for the design.

## In GitHub Actions

See [07-github-actions.md](07-github-actions.md) — the short version:

```yaml
- uses: chevp/ksforge@v1
  with:
    change-request: ${{ inputs.change-request }}
```
