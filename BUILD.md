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

Push a tag matching `v*` (e.g. `v0.1.0`). `.github/workflows/release.yml`
runs the same test suite, builds a `linux-x86_64` release binary, and
attaches it plus a `.sha256` checksum to a GitHub Release. Other platforms
aren't built yet — see [docs/01-installation.md](docs/01-installation.md).
