# Integrations

An **integration** is an MCP server the agent can call during a run, on top
of whatever `--tools` a capability already allows — e.g. read-only access
to an external system such as Jira or an internal API. It never changes
*what capability* is running (that's still one of implement/review/fix/
explain, see [05-capabilities.md](05-capabilities.md)); it only adds tools.

## Using one

```bash
ksforge implement --change "..." --mcp-config path/to/mcp-config.json
```

`--mcp-config <path>` is passed straight through to `claude -p
--mcp-config <path> --strict-mcp-config`. The file is Claude Code's own MCP
config format (a JSON file describing one or more `mcpServers` entries,
each an MCP stdio/SSE server to spawn) — see Claude Code's own
documentation for the exact shape (`claude --help` for the flag itself).
`--strict-mcp-config` is always added alongside it, so the run only ever
gets the servers named in that file, never anything else picked up from a
user- or project-level Claude Code config (least privilege — see
[09-security.md](09-security.md)).

Without `--mcp-config`, `claude -p` runs exactly as before: no MCP flags at
all.

## Reusing Claude Code's own MCP config format

`palau-test` (`tools/palau-test`), the sibling code-generation tool this
mechanism is modeled on, ships its integrations as compiled npm packages
(`packages/capability-jira`, etc.) inside its own monorepo, each exporting
a small descriptor `{name, description, entry, env}` that its CLI turns
into an `--mcp-config` file at runtime.

ksforge ships as a single distributed binary (see
[01-installation.md](01-installation.md) and
`.github/workflows/release.yml`), so it exposes Claude Code's own
`--mcp-config` flag directly instead of layering a second,
ksforge-specific descriptor format on top of it. Adding an integration for
a given workspace or CI job then needs only a JSON file next to the
workflow that uses it — no ksforge rebuild.

This version keeps the surface intentionally small: ad-hoc `--mcp-config`
paths per invocation, a scoping choice rather than the ceiling of the
mechanism. A later version could add a ksforge-side registry of named
integrations the way `palau-test`'s `--capability <name>` flag works — e.g.
`ksforge integrations add <name>` writing/merging into a shared config
file — if a real project ends up wanting a named, versioned set of these.

## In GitHub Actions

```yaml
- uses: chevp/ksforge@v1
  with:
    change-request: ${{ inputs.change-request }}
    mcp-config: .github/ksforge/mcp-config.json
```

Commit the MCP config file to the repository (or generate it in an earlier
step) rather than inlining secrets into it — env vars a spawned MCP server
needs are whatever that JSON file's own `env` map specifies, or whatever
the workflow step's own `env:` block sets; ksforge does not mediate or
filter environment variables for MCP servers, the same way it does not for
`claude` itself (see "Secrets" in [09-security.md](09-security.md)).
