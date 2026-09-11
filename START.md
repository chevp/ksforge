# Start

Assumes ksforge and `claude` are both installed and on your `PATH` — see
[INSTALL.md](INSTALL.md) if not.

```bash
export ANTHROPIC_API_KEY=...   # or however Claude Code auth is configured

cd your-project
ksforge implement --change "As a user, I want to reset my password via email."
```

What happens:

1. ksforge resolves your current directory as the workspace and starts a
   new `Execution`.
2. It spawns `claude -p` with a system prompt carrying ksforge's policy
   (constraints, capability instructions — see
   [docs/03-architecture.md](docs/03-architecture.md#prompt-structure))
   and a user prompt carrying your change request, and lets Claude Code
   explore and edit the workspace directly.
3. Claude Code either finishes (ksforge then diffs the workspace to see
   what actually changed) or reports it needs a decision.
4. If you passed `--validate <cmd>`, ksforge runs those commands before
   trusting the result.
5. ksforge prints what changed and exits with a status-specific code.

## Try it safely first

```bash
ksforge implement --change "..." --dry-run
```

Runs against an isolated temporary copy of your workspace — nothing is
written back to the real one.

## The other capabilities

```bash
ksforge review .                       # findings only, never writes
ksforge explain src/auth.rs            # explanation only, never writes
ksforge fix --change "Login times out after 30s under load"
ksforge capabilities                   # list what's available
```

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

The process exits (code `6`) rather than waiting — come back whenever and
run the `resume` command it printed.

## Next

[docs/02-quickstart.md](docs/02-quickstart.md) for the full walkthrough,
[docs/04-cli-reference.md](docs/04-cli-reference.md) for every flag and
exit code, or [docs/07-github-actions.md](docs/07-github-actions.md) to
run this in CI instead of locally.
