# Configuration reference

The settings file is `config.toml`. On macOS it lives in `~/Library/Application Support/venecita-dock/`.
Venecita reloads it as soon as you save it. Every section is optional, so an empty file is valid.
Unknown keys are rejected, which catches typos. If something is wrong, Venecita lists **every**
problem at once, each with its location (for example `profiles[0].layouts[1].rules[2].zone`).

For a complete setup, see [`examples/config.toml`](../examples/config.toml).

## `[general]`

| Key | Default | Description |
|---|---|---|
| `language` | `"auto"` | `"auto"`, `"en"` or `"es"` |
| `grid` | `{ cols = 8, rows = 4 }` | Grid used on every display (each value 1–32) |
| `gap` | `8` | Points between windows and around the screen edges (0–200) |
| `launch_at_login` | `false` | Start Venecita Dock when you log in |
| `ignore` | `[]` | Apps that Venecita never moves |

## `[focus]`

| Key | Default | Description |
|---|---|---|
| `size` | `0.8` | Fraction of the screen the focused window takes (0.3–1.0) |
| `dim_opacity` | `0.55` | How dark the rest of the screen gets (0–1) |

## `[tabs]`

| Key | Default | Description |
|---|---|---|
| `height` | `24` | Height of the tab bar shown on zones that hold several windows (16–64) |

## `[dim]`

| Key | Default | Description |
|---|---|---|
| `enabled` | `false` | Darken every window except the focused one, so it stands out |
| `opacity` | `0.3` | How dark the other windows get (0.05–0.9) |

The menu bar and the Dock are never dimmed. The focused window's tab bar stays bright.

## `[api]`

| Key | Default | Description |
|---|---|---|
| `enabled` | `true` | Run the local API |
| `bind` | `"127.0.0.1:47800"` | Address and port |
| `allow_lan` | `false` | Must be `true` to listen on an address other than loopback |

## `[[displays]]`

```toml
[[displays]]
id = "lg43"                       # short name used by layouts and rules
match = { name = "LG Monitor" }   # every key you set must match
```

`match` accepts these keys:
- `builtin` (the laptop screen);
- `name` (any part of the monitor's name, ignoring case);
- `vendor`, `model` and `serial` (EDID numbers);
- `width` and `height` (resolution in pixels, as printed by the probe: points × scale. On a scaled 4K mode this can differ from the native 3840 × 2160).

Each display can also have its own `grid`, which replaces `general.grid` for zones on that display:

```toml
[[displays]]
id = "lg43"
match = { name = "LG Monitor" }
grid = { cols = 8, rows = 8 }    # a 43" screen deserves a finer grid
```

If two definitions match the same kind of monitor, they are assigned **left to right**. The
definition listed first gets the left-most monitor.

## `[[profiles]]` and `[[profiles.layouts]]`

```toml
[[profiles]]
name = "Casa"

  [[profiles.layouts]]
  name = "Notebook abierta"
  displays = ["lg43", "laptop"]
  dynamic_zone = { display = "laptop", col = 0, row = 0, w = 8, h = 4 }

    [[profiles.layouts.rules]]
    apps = ["com.google.Chrome", "chrome.exe"]
    zone = { display = "lg43", col = 0, row = 0, w = 4, h = 4 }
```

- A layout **fits** when every display it lists is connected. Among the layouts that fit, the
  one that uses the most displays is chosen. This is how "laptop open" and "laptop closed"
  work.
- The profile you choose by hand from the tray, a hotkey or the API takes priority, as long as
  it fits. Otherwise the first profile in the file that fits is used.
- Zones: `col` and `row` give the top-left cell, counting from 0; `w` and `h` give the size in
  cells. The zone must fit inside the grid.
- `dynamic_zone` is where windows go when they have no rule, or when the display in their
  rule is not connected.

### Identifying apps

`apps` entries are compared without regard to case against:

| OS | App id | Example |
|---|---|---|
| macOS | Bundle identifier | `com.google.Chrome` |
| Windows | Executable name (the `.exe` is optional) | `chrome.exe` |
| Linux | `WM_CLASS` | `google-chrome` |

They are also compared against the app's name, for example `Google Chrome`. To find a bundle
id on macOS, run `osascript -e 'id of app "Google Chrome"'`.

## `[hotkeys]`

```toml
[hotkeys]
"ctrl+alt+f" = "focus_mode_toggle"
"ctrl+alt+left" = { action = "move_zone", dir = "left" }
```

Hotkeys are written as modifiers plus a key:

- **Modifiers**: `ctrl`, `alt` (or `option`), `shift`, `cmd` (also `super` or `win`).
- **Keys**: `a`–`z`, `0`–`9`, `f1`–`f24`, `left`, `right`, `up`, `down`, `enter`, `tab`,
  `space`, `escape`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `minus`,
  `equal`, `comma`, `period`, `slash`, `backslash`, `semicolon`, `quote`, `backquote`,
  `bracketleft` and `bracketright`.

Every hotkey needs at least one modifier, except the function keys. If the table is missing,
the default hotkeys are used. If it is present, only the hotkeys it lists are active.

| Action | Parameters | What it does |
|---|---|---|
| `move_zone` | `dir` | Move the window one cell |
| `grow` / `shrink` | `dir` | Move the edge on that side out or in by one cell |
| `move_to_display` | `target` = `next` / `prev` | Send the window to another monitor |
| `snap_to_grid` | | Snap the window to the nearest zone |
| `pin_here` | | Snap the window and remember the zone for its app |
| `stack_next` / `stack_prev` | | Cycle the windows that share a zone |
| `dynamic_next` / `dynamic_prev` | | Cycle the windows in the dynamic zone |
| `focus_mode_toggle` | | Turn extra focus mode on or off |
| `relayout_all` | | Put every window back in its place |
| `activate_profile` | `name` | Force a profile |
| `next_profile` | | Cycle through the profiles that fit |
| `auto_profile` | | Go back to automatic profile detection |
| `toggle_pause` | | Pause or resume automatic placement |
| `open_settings` | | Open the settings window |
