# Changelog

All notable changes to this project are documented here. Format loosely
follows [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - Unreleased

### Added

- Initial release: `implement`, `review`, `fix`, `explain`, `resume`,
  `status`, `capabilities` commands.
- `ClaudeCodeExecutor`: spawns the Claude Code CLI as a controlled
  subprocess with `--json-schema`-constrained structured output.
- Durable, resumable `Execution` state (`.ksforge/executions/<id>/`) with
  a human-in-the-loop pause/resume protocol — see
  [docs/06-human-in-the-loop.md](docs/06-human-in-the-loop.md).
- `--dry-run` via an isolated temporary workspace copy.
- `--validate` (repeatable, never model-controlled).
- `--format text|json`.
- PR creation via `github::pull_request` (the only module with Git/GitHub
  concepts) behind `--create-pull-request`.
- `action.yml` targeting `workflow_dispatch`, plus CI and
  `linux-x86_64`-only release workflows.

### Known limitations

- Only `linux-x86_64` release binaries are built; other platforms require
  building from source.
- The GitHub Issue/PR comment-driven resume path (`/ksforge choose ...`)
  is documented but not implemented — only `workflow_dispatch`-based
  resume is wired up.
- The Claude Code `--print --output-format json` envelope shape used by
  `ClaudeCodeExecutor` was not independently verified with a live
  `claude -p` call in the environment this was built in — see
  [docs/03-architecture.md](docs/03-architecture.md).
