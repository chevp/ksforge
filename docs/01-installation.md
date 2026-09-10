# Installation

ksforge is a single static-ish binary. It does not bundle Claude Code —
install that separately (`npm install -g @anthropic-ai/claude-code`, or see
[Claude Code's own docs](https://docs.claude.com/claude-code)) and make sure
`claude` is on your `PATH` or pass `--claude-path`.

## From a release (recommended)

Linux x86_64 binaries are published on every tagged release:

```bash
curl -sSfL https://github.com/chevp/ksforge/releases/latest/download/ksforge-linux-x86_64.tar.gz \
  | tar -xz
sudo install -m 0755 ksforge /usr/local/bin/ksforge
ksforge --version
```

**Other platforms (macOS, Windows, Linux ARM64) are not built yet.** The
release workflow (`.github/workflows/release.yml`) only produces
`linux-x86_64` today; it's structured so adding a platform is one matrix
entry, not a rewrite — see the comment at the top of that file. Until then,
build from source on those platforms (below).

## From source

Requires a recent stable Rust toolchain (built and tested against 1.95).

```bash
git clone https://github.com/chevp/ksforge.git
cd ksforge
cargo build --release
./target/release/ksforge --version
```

See [BUILD.md](../BUILD.md) at the repo root for the full build/test/lint
loop this project expects before a change is considered done.

## Claude Code

ksforge does not talk to the Anthropic API itself — it spawns `claude` as a
subprocess and lets Claude Code handle authentication, model access, and
tool execution. Set `ANTHROPIC_API_KEY` (or however you've configured
Claude Code auth) in the environment ksforge runs in; ksforge never reads
or touches that variable itself, it simply stays in the process environment
and Claude Code's own subprocess inherits it. See
[09-security.md](09-security.md).
