# Build

Requires a recent stable Rust toolchain (built and tested against
`cargo`/`rustc` 1.95).

```bash
cargo build            # debug
cargo build --release  # what the release workflow ships
```

## Before you call a change done

This is exactly what `.github/workflows/ci.yml` runs on every push/PR —
run it locally first:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

All four must pass clean. `cargo test --all` covers unit tests (in
`src/**`, next to the code they test) and the integration suite in
`tests/` — CLI-level black-box tests via `assert_cmd`
(`tests/cli_basic.rs`) and pipeline-level tests against the library crate
using `ksforge::agent::MockAgentExecutor` instead of a real `claude`
process or Anthropic API call (`tests/execution_pipeline.rs`). No test in
this repository spends real API budget or requires `ANTHROPIC_API_KEY` to
be set.

## Project layout

`src/lib.rs` is the library crate (`domain`, `agent`, `application`,
`workspace`, `validation`, `github`, `cli`); `src/main.rs` is a two-line
binary that calls `ksforge::cli::run()`. This split exists so integration
tests can exercise the pipeline directly instead of only through spawned
subprocesses — see [docs/03-architecture.md](docs/03-architecture.md) for
the module layering.

## Releasing

Automatic: `.github/workflows/auto-release.yml` bumps `Cargo.toml`'s patch
version, tags, and pushes after every PR merged into `main`. That tag push
is what triggers `.github/workflows/release.yml`, which runs the same test
suite, builds a `linux-x86_64` release binary, and attaches it plus a
`.sha256` checksum to a GitHub Release. Other platforms aren't built yet —
see [docs/01-installation.md](docs/01-installation.md).

**Don't manually bump `Cargo.toml`'s version in a PR** — `auto-release.yml`
always applies its own patch bump on top of whatever is already there
after merge, so a version bumped inside the PR itself just means the
auto-bump lands on an already-bumped number. A PR that itself needs a
minor/major bump (a real feature, not just a fix) should bump it in the
PR, and the auto-release patch bump on top is harmless there too (e.g.
`0.2.0` merges, then auto-bumps to `0.2.1`) — just don't bump patch
yourself expecting to control the exact number, since the workflow does
that unconditionally on every merge regardless.
