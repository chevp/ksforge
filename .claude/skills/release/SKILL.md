---
name: release
description: Bump the ksforge version, create an annotated v* tag and push it so release.yml builds and publishes a GitHub release. Use when asked to tag, release or cut a new version.
disable-model-invocation: true
argument-hint: "[patch|minor|major|x.y.z]"
---

# Release ksforge

Pushing a `v*` tag triggers `.github/workflows/release.yml` (test, `cargo build --release --locked`, publish via `softprops/action-gh-release`). The version lives in `Cargo.toml`; `Cargo.lock` is tracked and must match, because CI builds with `--locked`.

`auto-release.yml` already bumps the patch version and tags after every merged PR. Use this skill for manual releases: minor/major bumps, or when auto-release did not run.

## Steps

1. Preconditions — stop and report if any fails:
   - On `main`, up to date: `git fetch origin && git status -sb`.
   - Working tree clean, apart from files unrelated to the release (do not commit them).
   - `cargo test --all` passes.
2. Increment the version. Current: `grep -m1 '^version = ' Cargo.toml`; latest tag: `git tag --sort=-v:refname | head -1`.
   - Argument `patch` (default), `minor`, `major`, or an explicit `x.y.z`.
   - `patch`: `0.4.9` → `0.4.10`; `minor`: `0.4.9` → `0.5.0`; `major`: `0.4.9` → `1.0.0`.
   - New version must be greater than the latest tag and the tag must not exist yet.
3. Write the new version into the first `version = "…"` line of `Cargo.toml`:
   ```sh
   sed -i "0,/^version = \"$CURRENT\"\$/s//version = \"$NEXT\"/" Cargo.toml
   ```
   Then refresh the lock file: `cargo update -p ksforge --offline` (fallback: `cargo check`). `git diff` must show only `Cargo.toml` and `Cargo.lock`.
4. Commit and tag:
   ```sh
   git add Cargo.toml Cargo.lock && git commit -m "Bump version to X.Y.Z" && git tag -a vX.Y.Z -m "ksforge vX.Y.Z"
   ```
5. Confirm with the user before pushing (a tag push publishes a release). Then:
   ```sh
   git push origin main && git push origin vX.Y.Z
   ```
6. Verify: `gh run list --workflow=release.yml --limit 1`, and after success `gh release view vX.Y.Z`.

## Rules

- Never move or delete an existing tag, and never force-push, unless the user explicitly asks.
- Tag format is `vX.Y.Z` (matches the `v*` trigger); the Cargo version has no `v`.
- If `release.yml` fails, report the failing step and re-run it with `gh run rerun <run-id> --failed`; do not retag.
- `release.yml` has no `workflow_dispatch`; a tag that was pushed but never released cannot be published by `gh workflow run`. Report it instead of working around it.
