# Phase 9: Windows backend

## Goal
The same features on Windows 10 and 11.

## Implementation (`windows` crate)
- Windows are listed with `EnumWindows`, keeping only those that are visible, have no owner,
  and are not tool windows. Cloaked UWP windows are filtered out with
  `DwmGetWindowAttribute(DWMWA_CLOAKED)`.
- The app identity is the exe file name from `QueryFullProcessImageNameW`, with the product
  name from the file's version info as the name.
- Windows are placed with `SetWindowPos`, adjusted for the invisible resize borders reported
  by `DWMWA_EXTENDED_FRAME_BOUNDS` so that gaps line up with pixel accuracy.
- The process declares per-monitor DPI awareness v2 in its manifest.
- Events come from `SetWinEventHook`: `EVENT_OBJECT_SHOW`, `EVENT_OBJECT_DESTROY`,
  `EVENT_SYSTEM_FOREGROUND` and `EVENT_SYSTEM_MOVESIZEEND`. Display changes come from
  `WM_DISPLAYCHANGE` received by a hidden window.
- Displays are listed with `EnumDisplayMonitors` and `GetMonitorInfoW`, and identified by
  name and serial through `DisplayConfigGetDeviceInfo` / EDID.
- The built-in display is detected from its output technology (`DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL`).
- Overlays are layered windows with `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE`.
- Launch at login uses the registry key `HKCU\…\Run`.
- Hotkeys use the `global-hotkey` crate, which is already cross-platform. On Windows, `cmd` maps to the Win key.

## Tests
- The `Platform` contract suite runs in Windows CI.
- `examples/probe.rs` runs on Windows.

## Manual checklist (work setup)
- [ ] Both 27" monitors and the laptop screen are detected and told apart.
- [ ] Window gaps are exact, with no invisible-border offset.
- [ ] Monitors with different DPI scaling (for example 100 % and 150 %) place windows correctly.
