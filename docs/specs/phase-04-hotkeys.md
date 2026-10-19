# Phase 4: Hotkeys and actions

## Goal
Every action can be triggered with a configurable global keyboard shortcut.

## Scope
- The app registers every entry in `[hotkeys]` with the `global-hotkey` crate and
  re-registers them when the config is reloaded.
- The engine implements these actions on the focused window:
  - `move_zone`, `grow` and `shrink` take the window's current zone, or its snapped zone if it
    has none, apply the change, and set the new frame.
  - `move_to_display(next|prev)`: displays are ordered by x, then y. The window keeps the same
    zone in cells, clamped if the target grid is different.
  - `snap_to_grid`: snaps the window to the nearest zone.
  - `pin_here`: snaps the window, then adds a rule (or updates the existing one) for its app
    in the active layout. The change is written back to the TOML with `toml_edit`, which keeps
    the user's comments and formatting.
  - `relayout_all`, `activate_profile`, `next_profile` and `toggle_pause`.
- Default hotkeys (they can be changed and none of them are required):

| Hotkey | Action |
|---|---|
| ctrl+alt+arrows | move_zone |
| ctrl+alt+shift+arrows | grow |
| ctrl+alt+cmd+arrows | shrink |
| ctrl+alt+n / ctrl+alt+b | move_to_display next / prev |
| ctrl+alt+s | snap_to_grid |
| ctrl+alt+p | pin_here |
| ctrl+alt+r | relayout_all |
| ctrl+alt+f | focus_mode_toggle |
| ctrl+alt+] / ctrl+alt+[ | stack_next / stack_prev |
| ctrl+alt+. / ctrl+alt+, | dynamic_next / dynamic_prev |
| ctrl+alt+0 | next_profile |

- If the OS refuses to register a hotkey (because it is already taken), the user gets a
  notification and the error is logged.

## Implementation notes
- The engine works on the focused window's *snapped* zone, so it works even for windows
  placed by hand.
- `move_to_display` keeps the same proportion of the screen (`Zone::rescaled`), so a quarter
  of the 8 × 8 LG stays a quarter of the 8 × 4 laptop.
- `pin_here` updates the engine's in-memory rules immediately and asks the app to persist
  them (`UiRequest::SaveRule`, written with `toml_edit`). The file watcher then reloads an
  identical config, so nothing moves.
- Actions that can't run (no focused window, a display outside the layout) produce a
  `Notice`. The app logs it and plays the system beep.
- Shortcuts are registered with the `global-hotkey` crate; conflicts are listed in the menu.

## Tests
- Each action on the engine, starting from a known state, produces the expected frame or config change.
- `pin_here` on a TOML file full of comments keeps the comments and adds or updates the rule.
- Hotkeys that collide once in canonical form are rejected by validation.

## Manual checklist
- [ ] Every default hotkey works on a TextEdit window.
- [ ] `pin_here`, then closing and reopening the app, brings the window back to the pinned zone.
