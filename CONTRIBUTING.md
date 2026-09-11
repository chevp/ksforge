# Contributing

1. Read [docs/03-architecture.md](docs/03-architecture.md) first — in
   particular §CnK6mQd/§xjZiT6h of the original design brief this project
   follows: **do not implement a competing coding agent inside ksforge.**
   Repository exploration, file editing, and reasoning belong to Claude
   Code; ksforge orchestrates around it. A PR that adds a parallel
   agent loop, custom tool-calling, or a bespoke prompt-and-parse editing
   protocol is out of scope regardless of how well it works.
2. Same for Git/GitHub concepts: they may only appear in `src/github/`.
   `domain`, `agent`, `application`, `workspace`, and `validation` must
   stay free of `Commit`/`Branch`/`HEAD`/etc.
3. Run the full check from [BUILD.md](BUILD.md)
   (`fmt --check && clippy -D warnings && test --all && build --release`)
   before opening a PR.
4. Add tests with new behavior — see `tests/execution_pipeline.rs` for the
   `MockAgentExecutor` pattern; do not add a test that spawns a real
   `claude` process or spends real API budget.
5. Keep changes scoped. Don't fold in unrelated refactors, comments, or
   new abstractions — see the "no over-engineering" guidance throughout
   [docs/](docs/).

Bug reports and feature requests: open an issue. Security issues: see
[SECURITY.md](SECURITY.md), not a public issue.
