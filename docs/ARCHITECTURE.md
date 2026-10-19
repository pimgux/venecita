# Architecture

Venecita Dock is split so that **almost all behaviour is plain, OS-independent Rust**.
A thin OS layer reports events and carries out commands. That split is what makes the
project testable and portable.

```
             ┌────────────── venecita-app (binary) ───────────────┐
 hotkeys ──► │                                                    │
 tray    ──► │   Action ─┐                                        │
 API/CLI ──► │           ▼                                        │
             │   ┌───────────────┐  Vec<Command>  ┌────────────┐  │
 OS events ─►│   │ Engine (core) │ ─────────────► │  Platform  │──┼──► OS (move window,
             │   └───────────────┘                │  backend   │  │     overlays, focus…)
             │           ▲   Event                └────────────┘  │
             │           └─────────────────────────────┘          │
             └────────────────────────────────────────────────────┘
```

## Crates

| Crate | Depends on | Notes |
|---|---|---|
| `venecita-core` | serde, toml | No OS calls and no async. Contains geometry, the grid, config, rules, profiles, actions, stacks, focus and the engine. |
| `venecita-platform` | core | `Platform` trait plus `macos`, `windows`, `linux` and `fake` backends. Platform-specific code goes behind `#[cfg(target_os)]`. |
| `venecita-api` | core, axum, tokio | REST and WebSocket server. Talks to the engine through channels. |
| `venecita-app` | all | Event loop, tray, global hotkeys, egui settings window and i18n. |
| `venecita-cli` | reqwest | A thin HTTP client for the API. |

## Key concepts

- **Display definition**: a named matcher such as `lg43 = { name = "LG Monitor" }` or
  `laptop = { builtin = true }`. The engine resolves connected displays to definitions.
  Definitions are tried in config order. Each one claims the **left-most** unclaimed display
  it matches, so two identical monitors can still be told apart by their position.
- **Profile → layouts**: a profile (for example "Home") has one or more layouts. Each layout
  lists the displays it needs. Among the layouts whose displays are all connected, the
  engine picks the most specific one (the one that uses the most displays). So
  "Laptop open" (`lg43` + `laptop`) wins over "Laptop closed" (`lg43`) whenever the
  laptop screen is on. A profile chosen by hand takes priority; otherwise the first
  profile in config order with a matching layout is used.
- **Grid and zone**: every display's *work area* (the screen minus the menu bar and dock)
  is divided into `cols × rows` cells. A zone is a rectangle of contiguous cells:
  `{col, row, w, h}`. A gap of `g` pixels leaves `g` between neighbouring windows and
  around the outer edge.
- **Rule**: `apps = [...]` → placement (display + zone). Apps are matched without regard to
  case against the app id (macOS bundle id, Windows exe name, Linux WM_CLASS) or the
  process name.
- **Dynamic zone**: where windows without a rule (or whose display is missing) go.
- **Stack**: every zone keeps an ordered list of the windows it holds. A tab bar is shown
  when it holds more than one.
- **Action**: a single serializable enum. Hotkeys, the tray menu, the API and the CLI all
  go through it.
- **Engine**: `handle(Event) -> Vec<Command>`. It is deterministic, which makes it easy to
  test with scenarios.

## Settings window

The settings window is a **separate process**: the tray runs `venecita-dock --settings`
(or brings the running one to the front). The menu-bar app owns a hand-written
`NSApplication` event loop and eframe needs its own, so the two are not mixed in one process.

- `venecita_app::settings::model::SettingsModel` is a UI-free view-model. The config file's
  `toml_edit` document is the single source of truth, and a validated `Config` is derived
  from it after every edit. Comments survive, and an untouched model saves the same bytes.
- `venecita_app::settings::gui` (egui, macOS) only turns widgets into model calls. It reads
  the connected displays and running apps straight from the OS, so it does not need the
  running app.
- Saving writes `config.toml`; the running app's file watcher reloads it as with hand edits.
- Strings are Fluent files in `crates/venecita-app/assets/i18n/{en,es}/settings.ftl`. A test
  checks that both languages define the same keys. The tray menu keeps its small table in
  `i18n.rs`.

## Coordinates

All geometry in core uses **logical points in global desktop coordinates, origin at the top
left, y pointing down**. Backends convert from native coordinates. For example, macOS Cocoa
puts the origin at the bottom left, and Windows works in physical pixels with per-monitor DPI.

## Files (macOS)

| Path | Contents |
|---|---|
| `~/Library/Application Support/venecita-dock/config.toml` | User configuration |
| `~/Library/Application Support/venecita-dock/token.secret` | API token (0600) |
| `~/Library/Logs/venecita-dock/` | Rotating logs |

On Windows these are stored under `%APPDATA%\venecita-dock\`, and on Linux under `$XDG_CONFIG_HOME/venecita-dock/`.
