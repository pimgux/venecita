# Contributing to Venecita Dock

Thanks for helping! A few ground rules keep the project easy to work on.

## Workflow

1. Open an issue first to discuss large changes.
2. Write tests before or together with the code. Put platform-independent logic in
   `venecita-core`, where it can be tested without a real OS.
3. Before you push, run:
   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## Code style

- Write code, comments and docs in English. User-facing strings go through i18n (EN/ES).
- Every public item has a doc comment. Explain *why*, not only *what*.
- Prefer small, pure functions. OS calls belong only in `venecita-platform`.
- Avoid `unwrap()` outside tests. Return errors with context instead.

## Crate map

| Crate | Responsibility |
|---|---|
| `venecita-core` | Geometry, grid, rules, profiles, config, actions and the engine (pure logic) |
| `venecita-platform` | `Platform` trait plus the macOS, Windows, Linux and fake backends |
| `venecita-api` | Local HTTP and WebSocket API |
| `venecita-app` | The desktop app: tray, hotkeys, overlays and settings GUI |
| `venecita-cli` | The `venecita` command line client |

## Platform tests

Tests that need a real window server live in `crates/venecita-platform/tests/` with
`harness = false`, because macOS requires the main thread. They are skipped unless you opt in:
`VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test macos_desktop`.
Also follow the manual checklist in the matching `docs/specs/phase-XX` file.

By contributing, you agree that your contributions are licensed under GPL-3.0-or-later.
