# Configuration

Everything ksforge needs is a CLI flag, an environment variable, or (for
Claude Code itself) whatever Claude Code's own configuration already
covers. There is no `ksforge.toml`/`.ksforge.toml` project config file and
no `ksforge config` subsystem in this version — deliberately: §fMSksqK
of the original design brief explicitly says keep executable discovery
minimal, and nothing else here accumulated enough repeated flags across a
real project to justify a config file yet.

## Environment variables

| Variable | Effect |
|---|---|
| `KSFORGE_CLAUDE_PATH` | Same as `--claude-path`; explicit path to the `claude` executable. |
| `ANTHROPIC_API_KEY` (or whatever Claude Code's own auth needs) | Never read by ksforge — stays in the environment for the spawned `claude` process to use. See [09-security.md](09-security.md). |

## Per-run configuration

Everything else is a flag on the command in question — see
[04-cli-reference.md](04-cli-reference.md) for the full table
(`--workspace`, `--model`, `--max-budget-usd`, `--validate`, `--dry-run`,
`--format`, `--create-pull-request`, `--base-branch`, `--mcp-config`) or
`action.yml` for the GitHub Actions equivalents.

`--mcp-config` points at a file, but is not itself a ksforge config file —
that file is Claude Code's own MCP config format, unparsed and unvalidated
by ksforge (see [11-integrations.md](11-integrations.md)); ksforge's own
"no config file" position above still holds.

## If you need project-level defaults today

Wrap ksforge in your own script or Make target, or set the flags directly
in your workflow YAML — both are one honest layer of indirection instead
of a second, ksforge-specific config format to keep in sync with the CLI
flags it would shadow. If a real project ends up wanting persistent
per-repo defaults (a default `--model`, a standing `--validate` list),
that's the concrete signal to add a config file — file an issue with the
repeated flags you're tired of typing.
