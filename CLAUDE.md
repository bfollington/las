# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

- Build: `cargo build`
- Run: `cargo run`
- Test all: `cargo test`
- Test single: `cargo test <test_name>`
- Lint: `cargo clippy`
- Format: `cargo fmt`
- Format check: `cargo fmt -- --check`

## Project Overview

Rust binary crate (`las`) using edition 2024. Unix-only by design — commands are
shell scripts executed via `/bin/bash`.

CI (`.github/workflows/ci.yml`) runs on PRs and main pushes: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`. All three must pass
locally before pushing.

## Releasing

Releases are cut from `main`. Binaries are built and attached automatically by
`.github/workflows/release.yml` on any `v*` tag push (macOS arm64/x86_64, Linux
x86_64/aarch64 musl — tar.gz + sha256 each; no Windows, see above).

1. Bump `version` in `Cargo.toml`; `cargo build` to refresh `Cargo.lock`.
   Pre-1.0 versioning: minor bump for features, patch for fixes.
2. Commit as `release: vX.Y.Z`, push `main`, wait for CI green.
3. Create the release — this also creates the tag, which triggers the binary build:
   ```bash
   gh release create vX.Y.Z --target main --title "vX.Y.Z" --notes "..."
   ```
   Write the notes by hand. The workflow's `create-release` job only
   auto-generates notes when no release exists for the tag, so a hand-created
   release is preserved untouched.
4. Verify: watch the run (`gh run list --workflow Release`), then confirm
   `gh release view vX.Y.Z --json assets -q '.assets[].name'` lists 8 assets.
   Optionally download one, `shasum -a 256 -c` it, and run `--version`.
5. `cargo install --path .` so the locally installed binary matches the tag.

## Merging PR stacks

When PR B is based on PR A's branch: merge A **without** deleting its branch,
retarget B (`gh pr edit B --base main`), and only then delete A's branch.
Deleting the branch first closes B permanently — GitHub cannot reopen a PR whose
base ref is gone (this bit us: #2 had to be re-filed as #4).
