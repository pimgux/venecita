# Phase 10: Packaging, release and Linux

## Packaging
- macOS: a universal `.app` bundle with `cargo-bundle` and a DMG. It is signed with a
  Developer ID and notarized when the credentials are configured in CI.
  `Info.plist` includes `LSUIElement` and the text for the Accessibility usage description.
- Windows: an MSI built with `cargo-wix`. It is signed when a certificate is available.
- GitHub Releases are triggered by tags (`v*`), and checksums are published with each release.
- Homebrew cask and winget manifests are added later.

## Linux backend
- X11 first, using `x11rb` and the EWMH hints `_NET_CLIENT_LIST`, `_NET_ACTIVE_WINDOW`,
  `_NET_MOVERESIZE_WINDOW` and `_NET_WM_WINDOW_TYPE`. `WM_CLASS` is the app id, and displays
  come from RandR.
- Wayland lets clients position windows only through compositor-specific APIs. The docs will
  explain this limitation, and backends for specific compositors (Sway IPC, KWin scripts,
  Hyprland IPC) can be added later.

## Acceptance criteria
- [ ] Pushing a tag produces downloadable artifacts for macOS and Windows.
- [ ] The X11 backend passes the contract tests (CI runs them under Xvfb with a simple window manager).
