# Validation

`--validate <command>` (repeatable) runs shell commands, in order, in the
workspace, **after** Claude Code reports `status: completed` and **before**
ksforge trusts that result. The first failing command stops the run and
marks the `Execution` `Failed`.

```bash
ksforge implement --change "..." \
  --validate "cargo check" \
  --validate "cargo test"
```

Design rules that are load-bearing, not incidental:

- **Never model-controlled.** The `AgentOutcome` schema Claude Code
  responds with has no field for validation commands. The only source is
  the CLI flag / action input / project config, i.e. a human or a
  workflow author.
- **Off by default.** An empty `--validate` list runs nothing and always
  "passes" — ksforge does not assume your project has a particular test
  command.
- **Runs in the same working directory Claude Code just used** — the
  isolated temp copy in `--dry-run`, the real workspace otherwise, so a
  `--dry-run` validation failure never touches your actual checkout
  either.
- **Output is captured, not streamed**, and truncated to the last ~4000
  bytes per command in `Execution.result.validation.commands[].output_tail`
  — enough to see why something failed without bloating the persisted
  state file.

Validation commands run via `cmd /C` on Windows and `sh -c` elsewhere —
whatever your project's own commands expect (`cargo test`, `npm test`,
`make check`, ...), not a ksforge-specific runner.
