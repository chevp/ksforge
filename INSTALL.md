# Install

ksforge is a single native binary with no runtime dependencies of its own.
It does not bundle Claude Code — install that separately
(`npm install -g @anthropic-ai/claude-code`, or see
[Claude Code's own docs](https://docs.claude.com/claude-code)) and make
sure `claude` is on your `PATH`, or pass `--claude-path`.

**Yes, it's globally installable** — on any platform Rust supports,
regardless of whether a prebuilt release exists for it. Pick one:

## 1. `cargo install` (recommended, all platforms — Windows, macOS, Linux)

Requires a recent stable Rust toolchain (built and tested against 1.95;
install via [rustup](https://rustup.rs) if you don't have one).

```bash
git clone https://github.com/chevp/ksforge.git
cd ksforge
cargo install --path .
```

This builds a release binary and copies it into cargo's own bin directory
(`~/.cargo/bin` on Linux/macOS, `%USERPROFILE%\.cargo\bin` on Windows),
which `rustup`'s installer already adds to `PATH` — so `ksforge` works
from any directory afterwards, no manual `PATH` edit or `sudo` needed.

```powershell
ksforge --version
```

To update after pulling new commits: re-run `cargo install --path .` from
the repo (add `--force` if cargo complains a binary by that name already
exists). To remove: `cargo uninstall ksforge`.

## 2. Prebuilt release binary (Linux x86_64 only, for now)

```bash
curl -sSfL https://github.com/chevp/ksforge/releases/latest/download/ksforge-linux-x86_64.tar.gz \
  | tar -xz
sudo install -m 0755 ksforge /usr/local/bin/ksforge
ksforge --version
```

**Other platforms (macOS, Windows, Linux ARM64) don't have a prebuilt
release yet** — `.github/workflows/release.yml` only produces
`linux-x86_64` today, structured so adding a platform is one matrix entry,
not a rewrite. Until then, use option 1 or 3 on those platforms.

## 3. Build from source without installing globally

```bash
git clone https://github.com/chevp/ksforge.git
cd ksforge
cargo build --release
./target/release/ksforge --version      # target\release\ksforge.exe on Windows
```

Copy the resulting binary onto your own `PATH` by hand if you want a
global command without using `cargo install`.

See [BUILD.md](BUILD.md) for the full build/test/lint loop this project
expects before a change is considered done.

## Claude Code authentication

ksforge does not talk to the Anthropic API itself — it spawns `claude` as
a subprocess and lets Claude Code handle authentication, model access, and
tool execution. Set `ANTHROPIC_API_KEY` (or however you've configured
Claude Code auth) in the environment ksforge runs in; ksforge never reads
or touches that variable itself, it simply stays in the process
environment and Claude Code's own subprocess inherits it. See
[docs/09-security.md](docs/09-security.md).

## Next

[START.md](START.md) for the first run, or the full
[docs/01-installation.md](docs/01-installation.md) /
[docs/02-quickstart.md](docs/02-quickstart.md) for more depth.
