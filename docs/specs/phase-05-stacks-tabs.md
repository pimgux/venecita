# Phase 5: Stacks, dynamic zone and tab bar

## Goal
Several windows can share one zone, and you can switch between them with tabs or hotkeys.

## Model (`venecita-core::stack`)
- `ZoneKey = (display_id, Zone)`. The dynamic zone also has a key.
- `Stack { windows: Vec<WindowId>, active: usize }`. Windows are added at the end, and a new window becomes active.
- Operations:
  - `push`;
  - `remove` (the active index is fixed up afterwards);
  - `next` and `prev` (they wrap around);
  - `activate(id)`, called when the user focuses a window by other means (Cmd-Tab or a click).
- Invariant: whenever a stack is not empty, `active < len`.

## Behaviour
- Every window in a stack has the same frame. The active one is raised.
- When a stack holds more than one window, a **tab bar** is drawn at the top of the zone with
  `tabs.height` (24 px by default). The windows are then made shorter by that height, so the
  bar never covers them.
- Each tab shows the app icon and a shortened window title. Clicking a tab focuses that
  window. The bar never takes focus away from the app.
- `stack_next` and `stack_prev` act on the stack of the focused window. `dynamic_next` and
  `dynamic_prev` act on the dynamic zone wherever the focus is.
- When a window closes, it leaves its stack. If the stack is down to a single window, the
  bar disappears and that window gets the full height back.

## macOS overlay
- The tab bar is a borderless, non-activating `NSPanel` at floating level that joins all spaces.
  It is drawn by the app's own renderer (`egui` on a separate viewport) or with Core Graphics.
  A spike decides which, keeping whichever is lighter.

## Implementation notes
- Stacks are keyed by `ZoneKey` (display id + zone). Every window Venecita places joins the
  stack of its zone; a window moved by keyboard or dragged out of the zone leaves it.
- After each input, the engine diffs the tab bars that should exist against those
  shown and emits `ShowTabBar` / `HideTabBar`. Bar ids look like `"lg43:0,0,4,4"`.
- The tab height is `tabs.height`, capped at a third of the zone so small zones remain usable.
- macOS: `macos/overlay.rs` uses a non-activating `NSPanel` with an `NSSegmentedControl`.
  Clicks go through the `VenecitaTabTarget` Objective-C class and are reported as
  `PlatformEvent::TabClicked`.
- New action `focus_window { id }` (also used by the API) focuses a window and activates
  it in its stack. Focusing un-minimizes the window.
- While the screens sleep macOS reports **no** active display. Those resyncs are ignored
  so the layout and tab bars survive until wake (found while testing on real hardware).
- `probe tabbar [seconds]` draws a test bar with real windows without moving anything.

## Tests
- Property tests on random sequences of push, remove, next, prev and activate: the
  invariants hold, and no window is ever duplicated or lost.
- Geometry: the bar and the window together fill the zone exactly.
- Engine scenarios: three Chrome windows end up in one stack, and cycling raises the right one.

## Manual checklist
- [ ] Opening three Finder windows into the dynamic zone shows three tabs.
- [ ] Clicking a tab brings that window to the front, and the hotkeys cycle through the windows.
- [ ] Closing windows until one is left removes the bar.
