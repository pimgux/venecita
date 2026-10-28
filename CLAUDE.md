# Venecita Dock — notes for Claude

Keyboard-driven window manager (Rust, GPL-3.0) that remembers where each app goes on a
grid per display. macOS and Windows 10/11; Linux later. The user writes in Spanish
(rioplatense); answer in Spanish.

## Layout

- `crates/venecita-core` — pure logic: grid, displays, rules, config (TOML + validation),
  profiles, actions, stacks, the placement `Engine` (`handle(Input) -> Vec<Command>`).
- `crates/venecita-platform` — `Platform` trait; `macos/` (AX, CoreGraphics, overlays),
  `windows/` (EnumWindows, SetWinEventHook, DisplayConfig, layered overlays), `linux/`
  (x11rb: EWMH, RandR, an event thread), `fake.rs`.
- `crates/venecita-api` — `paths`, `token` and (feature `server`) the axum HTTP/WebSocket
  server; the app answers its `Call`s from the UI thread (`venecita-app/src/api.rs`).
- `crates/venecita-cli` — the `venecita` command (clap + ureq, no server dependencies).
- `crates/venecita-app` — `shell.rs` (tray app, shared), thin OS layers `mac.rs` / `win.rs`,
  `driver.rs`, `menu.rs`, `settings/` (view-model `model.rs` + egui `gui/`, run as a separate
  process: `venecita-dock --settings`), Fluent strings in `assets/i18n/{en,es}`.
- `docs/specs/phase-*.md` — the roadmap specs. `docs/ARCHITECTURE.md`, `docs/CONFIG.md`.

## Checks (the "Verificación" workflow runs them on macOS, Windows and Linux)

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

Pushing a tag `v*` runs the "Publicar versión" workflow (`.github/workflows/release.yml`) and publishes the zipped Windows exe
with its SHA-256 (tags with `-` are pre-releases). Every push to `main` also uploads the exe
as a CI artifact. Commits end with the `Co-Authored-By` line the session asks for.

## Status (2026-10-28)

Done: phases 0–9 (8 = local API + `venecita` CLI, see `docs/API.md`; 9 = Windows backend), redesigned settings (Apple style), tray menu with
icons and shortcuts, auto-arrange, dim layer, focus mode, copying rules between layouts,
security fixes. Latest release: `v0.1.3` (`v0.1.1` was tagged but never published: its macOS
job was cancelled; the workflow now publishes when Windows succeeds even if macOS fails).

New in 0.1.3 (user feedback, 2026-10-28): connected displays the layout doesn't use are
reported (log + "Add to this layout" in the editor) and the starter config has `external_2`;
focus mode ends when any other window gets the focus; minimized/hidden windows leave their
stack and tab bar, no tab bars in focus mode; first run is paused until a layout has a rule;
the local API is off by default and a start failure shows in the tray; issue templates.
Still to do from that feedback, in order: zone editor edges/overlaps (#11), per-display grid
default (#3), Stage Manager warning (#10), onboarding wizard, monitor arrangement in the
editor (#8), uninstall docs (#6), update check (#5). `main` stays unprotected for now.

Fixed on Windows (2026-10-22), confirmed on a real desk (laptop + 2 Dell):

- The "every window goes to the dynamic zone" bug was a config where no layout fit the
  connected displays. `resolve_displays` now matches definitions that name a monitor before
  generic ones (`builtin = false`), the Displays tab no longer offers a second definition for
  a display already defined, and the model refuses to delete a display or untick it from a
  layout while rules still use it (those rules would be invisible in the editor and the
  config unsaveable). "Merge into…" moves the layouts and rules of one display to another.
- A profile is greyed out in the tray when none of its layouts fits: look at the
  `active layout` line and the `no layout fits` lines in `%LOCALAPPDATA%\venecita-dock\logs\`.
- Apps that own every form by a hidden window (Delphi: MobaXterm) are listed: an owned window
  counts if it has `WS_EX_APPWINDOW` (`windows/window.rs`, `eligible`).

Build gotcha: linking fails (`LNK1117 /MANIFESTINPUT`) when the checkout path contains a
comma, as the OneDrive one does (`DIUSFRAMI,S.A`). Build with
`CARGO_TARGET_DIR=C:\Users\mfperez\cargo-target\venecita`; CI is not affected.

Packaging (phase 10, first half): `scripts/bundle-macos.sh` (universal .app + DMG, ad-hoc
signed unless `MACOS_SIGN_IDENTITY`), `wix/venecita.wxs` (per-user MSI, WiX 5), app icon from
`icons::app_icon_rgba` (`cargo run -p venecita-app --example app-icons`).

Also done: new shortcuts from Settings, Windows 11–style tab bars, and the Linux X11 backend
(`venecita-platform::linux::X11Platform`, EWMH + RandR; contract test `x11_desktop` runs in
CI under Xvfb + Openbox). Not yet on Linux: the tray app (tray-icon needs GTK), tab bars and
the dim layer.

Next: forcing a layout by hand (today only the profile can be chosen; the layout always
follows the displays), a Linux tray app if wanted, Developer ID signing / notarization.
