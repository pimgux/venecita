# Venecita Dock

> A keyboard-driven window manager that **remembers where every application belongs**.
> macOS first, then Windows and Linux. Written in Rust. Licensed under GPL-3.0.

**Status: early development.** Runs on macOS and Windows 10/11. See [the roadmap](#roadmap).

## Why

Tools like Rectangle let you snap windows quickly but forget the layout as soon as
you close an app. Venecita Dock lets you divide every monitor into a grid
(8 × 4 by default) and assign each application, by its executable or bundle id,
to a rectangle of cells. Each time that app opens, its window goes back to the
same place.

## Features (planned for 1.0)

- **Per-application placement** on a configurable grid (default 8 × 4 per monitor).
- **Profiles** for each desk setup (e.g. *home*: 43" 4K + laptop, *work*: 2 × 27" + laptop),
  detected automatically from the connected monitors. Each profile can have layouts for
  "laptop open" and "laptop closed".
- **Dynamic zone**: windows without a rule are stacked in one zone that has a tab bar.
- **Stacks**: windows that share a zone (for example, 3 Chrome windows) are stacked; you can switch between them with tabs or a hotkey.
- **Extra focus mode**: the active window grows to about 80 % of the screen and everything else is dimmed.
- **Everything has a keyboard shortcut**, and you can rebind all of them.
- **Local API** (HTTP + WebSocket, protected by a token) and a `venecita` CLI, for
  integration with stream decks, macro pads and scripts.
- Menu-bar or tray icon, launch at login, gaps between windows, and a list of ignored apps.

## Local API and CLI

Stream decks, macro pads and scripts can control Venecita Dock through a local, token-protected
API, or with the `venecita` command (`venecita action relayout-all`, `venecita profile activate
Casa`…). See [`docs/API.md`](docs/API.md).

## Configuration

Settings are stored in a human-readable TOML file (see [`examples/config.toml`](examples/config.toml)
and [`docs/CONFIG.md`](docs/CONFIG.md)). You can change it from the settings window or edit it by hand.
Venecita reloads it as soon as you save.

## Installing

Download the latest version from the [Releases](https://github.com/pimgux/venecita/releases):

- **macOS** (11 or later, Intel or Apple Silicon): open `venecita-dock-…-macos.dmg` and drag
  **Venecita Dock** to Applications. On first start, grant the **Accessibility** permission
  (System Settings → Privacy & Security → Accessibility). Until releases are signed with a
  Developer ID, macOS may say it can't check the app: right-click it → *Open* the first time,
  and grant the permission again after updating.
- **Windows** (10 or 11): run `venecita-dock-…-windows-x86_64.msi`. It installs for your user
  only (no administrator rights), adds Venecita Dock to the Start menu and puts the `venecita`
  command on your PATH. A zip with the plain `.exe` files is there too.

## Building

```sh
# Requires Rust (https://rustup.rs)
cargo build --release
cargo test --workspace
```

Run the menu-bar app (it creates `~/Library/Application Support/venecita-dock/config.toml`
on first start):

```sh
cargo run --release -p venecita-app --bin venecita-dock
```

On macOS, Venecita Dock needs the **Accessibility** permission
(System Settings → Privacy & Security → Accessibility).

Packages: `scripts/bundle-macos.sh` builds `dist/Venecita Dock.app` and a DMG
(`UNIVERSAL=1` for Intel + Apple Silicon); `wix/venecita.wxs` is the Windows installer
(WiX 5). Pushing a tag `v*` builds and publishes both.

## Windows

Install it with the MSI from the [Releases](https://github.com/pimgux/venecita/releases)
(or unzip the zip anywhere and run `venecita-dock.exe`). It lives in the notification area
(next to the clock).

- The executable is not signed yet, so SmartScreen may warn the first time:
  choose *More info → Run anyway*.
- The configuration is `%APPDATA%\venecita-dock\config.toml` and the logs are in
  `%LOCALAPPDATA%\venecita-dock\logs\`.
- Apps are identified by their executable name, e.g. `apps = ["code.exe", "firefox.exe"]`
  (the `.exe` is optional) or by their product name (`"Visual Studio Code"`).
- Positions and sizes are in physical pixels, so gaps are exact on monitors with different
  scaling.
- Windows of apps running as administrator can't be moved by a normal app (Windows
  forbids it); run Venecita Dock as administrator too if you need that.
- `cmd` in shortcuts is the Windows key; many `Win+…` combinations are reserved by
  Windows itself.

## Roadmap

| Phase | Scope |
|---|---|
| 0 | Repository bootstrap, CI |
| 1 | Core domain and configuration (OS-independent) |
| 2 | macOS backend |
| 3 | Daemon MVP: automatic placement and profiles |
| 4 | Hotkeys and actions |
| 5 | Stacks, dynamic zone and tab bar |
| 6 | Extra focus mode |
| 7 | Settings GUI (egui) and i18n (EN/ES) |
| 8 | Local API and CLI |
| 9 | Windows backend |
| 10 | Packaging and Linux backend |

Detailed specifications live in [`docs/specs/`](docs/specs/). Architecture overview:
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Resumen en español

Venecita Dock es un gestor de ventanas que **recuerda dónde va cada aplicación**.
Divide cada monitor en una grilla (8 × 4 por defecto) y asigna cada aplicación
(por ejecutable o bundle id) a un rectángulo de celdas. También tiene perfiles para casa y trabajo,
una zona dinámica con pestañas, un modo de foco, atajos de teclado configurables y una
API local con token. Funciona primero en macOS; después llegan Windows y Linux.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions are welcome. Please keep the code clear and well commented.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
