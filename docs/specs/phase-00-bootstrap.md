# Phase 0: Bootstrap

## Goal
Set up the repository so that every later phase starts with formatting, linting, tests and CI already in place.

## Deliverables
- A Cargo workspace with five crates: `venecita-core`, `venecita-platform`, `venecita-api`,
  `venecita-app` and `venecita-cli` (the binary is called `venecita`).
- Shared settings in `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]`
  (`missing_docs` warns, and clippy runs in CI with `-D warnings`).
- `LICENSE` (the GPL-3.0 text), `README.md` (English plus a Spanish summary), `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, `.editorconfig`, `rustfmt.toml` and `.gitignore`.
- `docs/ARCHITECTURE.md` and one spec per phase in `docs/specs/`.
- GitHub Actions: `fmt` runs on Linux; `clippy` and `test` run on macOS, Windows and Linux.

## Acceptance criteria
- [ ] `cargo build --workspace` succeeds.
- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
      and `cargo test --workspace` pass locally.
- [ ] CI passes on all three operating systems (once the repository is pushed).
