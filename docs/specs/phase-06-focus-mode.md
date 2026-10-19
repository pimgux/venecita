# Phase 6: Extra focus mode

## Goal
One hotkey enlarges the window you are working on and dims everything else.

## Behaviour
- `focus_mode_toggle` **on**:
  - the focused window's current frame is saved;
  - it is resized to `work_area.centered_scaled(focus.size)` (0.8 by default) on its own display;
  - a click-through dim layer with opacity `focus.dim_opacity` (0.55 by default) covers every
    display, ordered just below the focused window.
- When the focus changes while focus mode is on, the previous window goes back to its saved
  frame and the new one is enlarged and saved the same way. The dim layer moves below it.
- `focus_mode_toggle` **off** (or pause, a profile change, or a display change): every saved
  frame is restored and the dim layer is removed.
- Focus mode does not change any stack or rule. It is a temporary view.

## macOS overlay
- One borderless `NSWindow` per display, black with alpha, set to `ignoresMouseEvents`.
- It is ordered with `orderWindow:NSWindowBelow relativeTo:<focused windowNumber>`.

## Tests
- `centered_scaled` keeps the rect centered, and the result stays inside the work area.
- An engine scenario with toggle on, a focus change, and toggle off restores every frame exactly.
- A display change while focus mode is on exits the mode cleanly.

## Manual checklist
- [ ] On the 43" monitor, the window grows to 80 % and the rest of the screen is dimmed.
- [ ] Cmd-Tab to another app switches the enlarged window.
- [ ] Turning the mode off puts everything back where it was.
