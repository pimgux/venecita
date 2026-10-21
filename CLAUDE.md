# Venecita Dock — notes for Claude

Keyboard-driven window manager (Rust, GPL-3.0) that remembers where each app goes on a
grid per display. macOS and Windows 10/11; Linux later. The user writes in Spanish
(rioplatense); answer in Spanish.

## Layout

- `crates/venecita-core` — pure logic: grid, displays, rules, config (TOML + validation),
  profiles, actions, stacks, the placement `Engine` (`handle(Input) -> Vec<Command>`).
- `crates/venecita-platform` — `Platform` trait; `macos/` (AX, CoreGraphics, overlays),
  `windows/` (EnumWindows, SetWinEventHook, DisplayConfig, layered overlays), `fake.rs`.
- `crates/venecita-app` — `shell.rs` (tray app, shared), thin OS layers `mac.rs` / `win.rs`,
  `driver.rs`, `menu.rs`, `settings/` (view-model `model.rs` + egui `gui/`, run as a separate
  process: `venecita-dock --settings`), Fluent strings in `assets/i18n/{en,es}`.
- `docs/specs/phase-*.md` — the roadmap specs. `docs/ARCHITECTURE.md`, `docs/CONFIG.md`.

## Checks (CI runs them on macOS, Windows and Linux)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- From macOS, the other targets can be checked without linking:
  `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings`
  (and `x86_64-unknown-linux-gnu`).
- MSRV is 1.80: no `&raw mut`, no `Option::is_none_or`.
- Tests run on a Windows-style checkout too (CRLF): don't compare text with `\n` only.
- Desktop contract test on Windows: `VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test windows_desktop`.
- Diagnostics: `cargo run -p venecita-platform --example probe -- list` (also `move <app>`,
  `watch`, `tabbar`).

## Releases

Pushing a tag `v*` runs `.github/workflows/release.yml` and publishes the zipped Windows exe
with its SHA-256 (tags with `-` are pre-releases). Every push to `main` also uploads the exe
as a CI artifact. Commits end with the `Co-Authored-By` line the session asks for.

## Status (2026-10-21)

Done: phases 0–7 and 9 (Windows backend), redesigned settings (Apple style), tray menu with
icons and shortcuts, auto-arrange, dim layer, focus mode. Latest pre-release:
`v0.1.0-alpha.2`.

**Open bug (Windows):** the settings work, but when arranging, every window goes to the
dynamic zone. Most likely the rules were written in a layout that isn't the one in use with
the connected displays (the editor used to open on the first layout). alpha.2 opens the
editor on the layout in use, warns otherwise, and logs for every window which rule matched
or why it went to the dynamic zone. To confirm, look at:

- `%APPDATA%\venecita-dock\config.toml` (the rules and their layout), and
- the latest log in `%LOCALAPPDATA%\venecita-dock\logs\` (`active layout profile=… layout=…`
  and, with alpha.2, one line per window explaining its placement).

Next, after that bug: phase 8 (local API + CLI), phase 10 (packaging: macOS .app, Windows
MSI), creating new shortcuts in Settings, the new tab-bar look on Windows.
