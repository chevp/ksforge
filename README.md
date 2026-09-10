# ksforge

**ksforge turns user stories into controlled Claude Code workflows.**

```bash
export ANTHROPIC_API_KEY=...

ksforge implement --story "As a user, I want to reset my password via email."
```

ksforge is not a Claude Code clone and not a generic LLM client. It's a
small orchestration layer, written in Rust, that:

1. takes a user story,
2. builds a controlled request (capability + constraints + validation
   policy) around it,
3. spawns Claude Code — the actual coding/agent execution engine — to
   explore and edit your workspace,
4. validates the result with commands you control,
5. and reports what changed, durably and resumably enough that a paused
   "I need a human decision" doesn't mean a hung CI job.

```text
ksforge      = orchestration: story, capability, policy, workflow, PR.
Claude Code  = execution engine: repository exploration, editing, reasoning.
GitHub Actions = automation runtime ksforge targets.
av           = a separate Kosmos artifact-transformation/variation layer,
               not a dependency of this first version.
```

## Quickstart

```bash
ksforge implement --story "As a user, I want to reset my password via email." --dry-run
ksforge implement --story "As a user, I want to reset my password via email."
ksforge review .
ksforge explain src/auth.rs
ksforge status <execution-id>
ksforge resume <execution-id> --decision <option-id>
```

See [docs/02-quickstart.md](docs/02-quickstart.md).

## GitHub Actions

```yaml
- uses: chevp/ksforge@v1
  with:
    story: ${{ inputs.story }}
    capability: implement
    create-pull-request: "true"
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

See [docs/07-github-actions.md](docs/07-github-actions.md), including the
resumable two-run pattern for when Claude Code needs a decision mid-story.

## Documentation

| | |
|---|---|
| [01 — Installation](docs/01-installation.md) | Binary release, build from source, Claude Code prerequisite |
| [02 — Quickstart](docs/02-quickstart.md) | First run, dry-run, the paused/resume flow |
| [03 — Architecture](docs/03-architecture.md) | Layering, the Claude Code subprocess contract, what's verified vs. inferred |
| [04 — CLI reference](docs/04-cli-reference.md) | Every command, flag, exit code, JSON shape |
| [05 — Capabilities](docs/05-capabilities.md) | implement / review / fix / explain, and how to add one |
| [06 — Human-in-the-loop](docs/06-human-in-the-loop.md) | The `Execution` state machine, pause/resume, what does and doesn't survive across CI jobs |
| [07 — GitHub Actions](docs/07-github-actions.md) | `action.yml`, `workflow_dispatch` examples, permissions, PR mode |
| [08 — Validation](docs/08-validation.md) | `--validate`, and why it's never model-controlled |
| [09 — Security](docs/09-security.md) | Prompt injection posture, least privilege, secret handling |
| [10 — Configuration](docs/10-configuration.md) | Environment variables and why there's no config file (yet) |
| [11 — Integrations](docs/11-integrations.md) | Optional MCP servers via `--mcp-config`, and how that differs from a capability |

Also: [INSTALL.md](INSTALL.md) (install/global setup), [START.md](START.md) (first run), [BUILD.md](BUILD.md) (build/test/lint loop), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
