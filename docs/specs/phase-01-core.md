# Phase 1: Core domain and configuration

## Goal
All placement logic that does not depend on the OS lives in `venecita-core`, with thorough tests.

## Modules

### `geometry`
- `Rect { x, y, w, h }` (`f64`, logical points, top-left origin).
- Helpers: `right`, `bottom`, `center`, `inset`, `intersection`, `area`, `contains_point`,
  `centered_scaled(factor)`. The last one is used by focus mode.

### `grid`
- `GridSize { cols, rows }`, default 8 × 4, each value between 1 and 32.
- `Zone { col, row, w, h }`, in cells, with `w` and `h` of at least 1.
- `Zone::fits(grid)`.
- `zone_rect(area, grid, zone, gap) -> Rect`. Cell boundaries are `area.x + area.w * i / cols`,
  so zones sit next to each other with no rounding drift. The gap is `g` between windows
  and `g` at the outer edge.
- `snap(area, grid, rect, gap) -> Zone`: rounds each edge to the nearest grid line, keeps
  `w, h ≥ 1`, and clamps to the grid.
- Zone operations (each returns a new zone clamped to the grid):
  - `moved(dir)`: shift one cell.
  - `grown(dir)`: extend the edge on that side.
  - `shrunk(dir)`: pull in the edge on that side, keeping at least one cell.

### `display`
- `DisplayInfo`: what a backend reports. Fields: `handle` (opaque OS id), `name`, `builtin`,
  `vendor`, `model`, `serial`, `frame`, `work_area` and `scale`.
- `DisplayMatcher`: optional `builtin`, `name` (a case-insensitive substring), `vendor`,
  `model`, `serial`, `width` and `height`. Every field that is set must match. An empty
  matcher is invalid.
- `resolve_displays(defs, connected) -> Vec<(def_id, handle)>`: definitions are tried in
  config order, and each takes the left-most unclaimed display it matches.

### `rules`
- `AppIdentity { id, name }`.
- `AppMatcher` (a list of strings) matches without regard to case against either `id` or `name`.
  On Windows a trailing `.exe` is optional, so `chrome` matches `chrome.exe`.

### `profile`
- `select_layout(config, resolved_ids, preferred_profile) -> Option<LayoutRef>` picks the layout
  whose displays are all connected and that uses the most displays. It looks in the preferred
  profile first, then in the other profiles in config order.

### `action`
- `Action`: a serde enum tagged by `action`, in snake_case. Its variants are `move_zone{dir}`,
  `grow{dir}`, `shrink{dir}`, `move_to_display{target}`, `snap_to_grid`, `pin_here`,
  `stack_next`, `stack_prev`, `dynamic_next`, `dynamic_prev`, `focus_mode_toggle`,
  `relayout_all`, `activate_profile{name}`, `next_profile`, `toggle_pause` and `open_settings`.
- `Hotkey` parsing:
  - Modifiers: `ctrl`/`control`, `alt`/`option`/`opt`, `shift`, `cmd`/`command`/`super`/`win`/`meta`.
  - Keys: `a`–`z`, `0`–`9`, `f1`–`f24`, arrows, `enter`, `tab`, `space`, `escape`,
    `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, and the punctuation names.
  - Hotkeys have a canonical string form, so `alt+ctrl+F` and `ctrl+alt+f` are equal.
- `ActionSpec` in TOML is either a string (`"focus_mode_toggle"`) or a table
  (`{ action = "move_zone", dir = "right" }`).

### `config`
- `Config` has these sections: `general` (language, grid, gap, launch_at_login, ignore),
  `focus` (size, dim_opacity), `tabs` (height), `api` (enabled, bind, allow_lan),
  `displays[]`, `profiles[]` (with `layouts[]`, each with `rules[]` and `dynamic_zone`)
  and `hotkeys`.
- Every field has a sensible default, so an empty file is a valid config.
- `Config::from_toml_str` parses and validates the file.
- `validate()` collects **all** errors, each with a readable path, for example
  `profiles[0].layouts[1].rules[2].zone: zone (col 6, w 4) exceeds grid width 8`. Checks:
  - grid limits, the range of the gap, and the ranges of the focus and tabs values;
  - display ids are unique and not empty, and matchers are not empty;
  - profile names are unique and not empty;
  - each layout's display references exist and are not repeated;
  - placements reference displays that are in their layout, and zones fit in the grid;
  - rules list at least one app;
  - hotkeys parse and do not collide once in canonical form, and `activate_profile` names exist;
  - `api.bind` parses, and a non-loopback address requires `allow_lan = true`.
- `default_config_toml()` returns a commented starter file. It is also published as
  `examples/config.toml`.

## Tests
- Unit tests for every function above, including edge cases: a 1 × 1 grid, a zone that
  covers the whole grid, a gap of 0, negative display origins (a monitor to the left of or
  above the primary one), and HiDPI.
- Property tests (`proptest`):
  - `snap(zone_rect(z)) == z` when `gap < cell/2`;
  - `zone_rect` always stays inside the area;
  - zones next to each other leave exactly `gap` between them;
  - `moved`, `grown` and `shrunk` always return a zone that fits.
- Config fixtures: `examples/config.toml` parses with no errors, and a file with many
  mistakes reports every one of them.

## Acceptance criteria
- [ ] `cargo test -p venecita-core` passes with no OS dependencies.
- [ ] The core crate has no `unsafe` code and no `unwrap()` outside tests.
