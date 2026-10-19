# Phase 3: Daemon MVP (automatic placement and profiles)

## Goal
A menu-bar app that places windows according to the config and switches profiles on its own.

## Engine (in `venecita-core::engine`)
- `Engine::new(config)` and `Engine::handle(event) -> Vec<Command>`.
- It keeps this state: connected displays, resolved display ids, the active profile and
  layout, a manual profile override, the tracked windows (`WindowId → AppIdentity, zone ref`),
  and a pause flag.
- How each event is handled:
  - `DisplaysChanged`: resolve displays again, select the layout again, then re-layout every window.
  - `WindowCreated`:
    - an ignored app or a non-normal window is not touched;
    - a window whose app has a rule goes to the rule's zone;
    - any other window goes to the dynamic zone, if one is defined; otherwise it is left alone.
  - `WindowDestroyed`: the window is forgotten.
  - `Action(RelayoutAll)`, `Action(ActivateProfile)` and `Action(NextProfile)` re-layout
    every window.
- A rule whose display is not in the current layout falls back to the dynamic zone.
- `Command::SetFrame` is skipped when the window is already within 1 pt of its target, so
  windows do not flicker.

## App
- Tray menu items:
  - "Profile: Home – Laptop open" (disabled; shows the current profile and layout);
  - the list of profiles, to switch by hand ("Automatic" turns the override off);
  - "Re-arrange all";
  - "Pause";
  - "Open config file";
  - "Reload config";
  - "Quit".
- The config is reloaded whenever the file changes (`notify`, debounced). If the new file is
  invalid, the previous config is kept and an error notification is shown.
- Logs are written with `tracing` to a rotating file. Use `RUST_LOG` to change the log level.
- Launch at login uses `SMAppService.mainApp` on macOS 13+. During development it falls back
  to a LaunchAgent plist.
- The app runs as an accessory, so it has a menu-bar icon but no Dock icon.

## Tests (engine scenarios with `FakePlatform` fixtures: home and work, lid open and closed)
- "Home, lid closed, Chrome opens": Chrome goes to `lg43` columns 0–3.
- "Home, lid opens": the layout changes to "Laptop open" and windows are re-laid out.
- "A window without a rule": it goes to the dynamic zone.
- "An ignored app": it is never moved.
- "A rule's display is missing": the window goes to the dynamic zone.
- "Pause": no commands are emitted.
- "A window already in place": no `SetFrame` is emitted.
- An invalid config reload keeps the previous config.

## Implementation notes
- `venecita_core::engine::Engine` is the pure state machine; `venecita_app::driver::Driver`
  connects it to any `Platform` (scenario tests in `crates/venecita-app/tests/scenarios.rs`).
- Window types and `PlatformEvent` moved to `venecita_core::window` so the engine needs no OS crate.
- Displays can override the grid (`[[displays]] grid = { cols = 8, rows = 8 }`).
- Launch at login uses a LaunchAgent (`~/Library/LaunchAgents/io.github.venecita-dock.plist`)
  until signed bundles exist (phase 10).
- Run it: `cargo run -p venecita-app --bin venecita-dock` (logs: `~/Library/Logs/venecita-dock/`).

## Manual checklist
- [ ] With your home setup, opening each configured app puts it in its zone.
- [ ] Closing the laptop lid re-arranges the windows onto the 43" monitor.
- [ ] At work, the two 27" monitors are told apart correctly.
- [ ] Editing the TOML file applies the change without restarting the app.
