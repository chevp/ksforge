# ksforge

**ksforge turns change requests into controlled Claude Code workflows.**

```bash
export ANTHROPIC_API_KEY=...

ksforge implement --change "As a user, I want to reset my password via email."
```

ksforge is a small orchestration layer, written in Rust, that:

1. takes a change request,
2. builds a controlled request (capability + constraints + validation
   policy) around it,
3. spawns Claude Code — the actual coding/agent execution engine — to
   explore and edit your workspace,
4. validates the result with commands you control,
5. and reports what changed, durably and resumably enough that a paused
   "I need a human decision" doesn't mean a hung CI job.

```text
ksforge      = orchestration: change request, capability, policy, workflow, PR.
Claude Code  = execution engine: repository exploration, editing, reasoning.
GitHub Actions = automation runtime ksforge targets.
av           = a separate Kosmos artifact-transformation/variation layer,
               independent of this first version.
```

## Quickstart

```bash
ksforge implement --change "As a user, I want to reset my password via email." --dry-run
ksforge implement --change "As a user, I want to reset my password via email."
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
    change-request: ${{ inputs.change-request }}
    capability: implement
    create-pull-request: "true"
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

Ready-to-copy template:
[examples/workflows/change-request-to-ksforge.yml](examples/workflows/change-request-to-ksforge.yml).
See [docs/07-github-actions.md](docs/07-github-actions.md), including the
resumable two-run pattern for when Claude Code needs a decision mid-run.

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
| [12 — Interactive chat](docs/12-interactive-chat.md) | `ksforge chat` / bare `ksforge` — conversational front end, local branch/commit/merge (confirmed) |
| [13 — The model is outside the orchestration](docs/13-model-outside-orchestration.md) | Why the phase loop, tool grants, and "is it done" all live in Rust, not the prompt |

Also: [INSTALL.md](INSTALL.md) (install/global setup), [START.md](START.md) (first run), [BUILD.md](BUILD.md) (build/test/lint loop), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
