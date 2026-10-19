# Phase 2: macOS backend

## Goal
Implement the `Platform` trait for macOS, together with a `FakePlatform` that tests can use on any OS.

## The `Platform` trait (in `venecita-platform`)
```rust
pub trait Platform {
    fn displays(&self) -> Result<Vec<DisplayInfo>>;
    fn windows(&self) -> Result<Vec<WindowInfo>>;        // only manageable windows
    fn focused_window(&self) -> Result<Option<WindowId>>;
    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()>;
    fn focus(&self, id: WindowId) -> Result<()>;
    fn raise(&self, id: WindowId) -> Result<()>;
    fn subscribe(&self, sink: Box<dyn Fn(PlatformEvent) + Send>) -> Result<Subscription>;
    fn overlays(&self) -> &dyn Overlays;                  // dim layer, tab bars (phases 5 and 6)
    fn set_launch_at_login(&self, enabled: bool) -> Result<()>;
    fn permissions(&self) -> PermissionStatus;            // Accessibility on macOS
}
```
- `WindowInfo { id, app: AppIdentity, pid, title, frame, kind }`. The `kind` is `Normal`,
  `Dialog`, `Floating` or `Fullscreen`; only `Normal` windows are managed.
- `PlatformEvent` is one of `WindowCreated`, `WindowDestroyed`, `WindowFocused`,
  `WindowMoved`, `AppLaunched`, `AppTerminated`, `DisplaysChanged` or `SystemWoke`.

## macOS implementation
- Windows are handled through the Accessibility API:
  - `AXUIElementCreateApplication(pid)` and `kAXWindowsAttribute` list them;
  - `kAXPositionAttribute` and `kAXSizeAttribute` move and resize them;
  - `kAXRaiseAction` and `NSRunningApplication.activate` bring them to the front.
- A stable window id comes from `_AXUIElementGetWindow` (a private but widely used API; yabai,
  AeroSpace and Rectangle all rely on it). If that call fails, the id falls back to the
  `CFHash` of the element plus the pid.
- The app identity is the `bundleIdentifier` of `NSRunningApplication`, with `localizedName`
  as the name.
- Only apps whose activation policy is `regular` are managed.
- A window counts as a dialog or floating when its subrole is anything other than
  `AXStandardWindow`.
- Events:
  - `NSWorkspace` notifications for app launch, termination and wake from sleep;
  - one `AXObserver` per app for `kAXWindowCreatedNotification`,
    `kAXUIElementDestroyedNotification`, `kAXFocusedWindowChangedNotification` and
    `kAXWindowMovedNotification`;
  - `CGDisplayRegisterReconfigurationCallback` for display changes, debounced by 500 ms
    because closing the lid fires several events.
- Displays:
  - `CGGetActiveDisplayList` lists them;
  - `CGDisplayIsBuiltin`, `CGDisplayVendorNumber`, `CGDisplayModelNumber` and
    `CGDisplaySerialNumber` identify them;
  - the name is `NSScreen.localizedName`;
  - `frame` and `visibleFrame` are converted to top-left coordinates.
- Permissions: `AXIsProcessTrustedWithOptions` with the prompt option. If permission is
  missing, the app shows how to grant it.

## `FakePlatform`
An in-memory platform with a configurable list of displays and windows. It records every
command it receives so tests can assert on them, and its event injection methods are public
so tests can simulate the OS.

## Tests
- A contract test suite, generic over `Platform`, runs against `FakePlatform` on every OS.
- `examples/probe.rs` (macOS) prints the displays and windows, and moves the frontmost
  TextEdit window into the left half.
- Tests that need the real macOS window server live in `tests/macos_desktop.rs` (`harness = false`, because macOS needs the main thread) and run with `VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test macos_desktop`.

## Manual checklist
- [x] The probe lists every monitor with the right name and the built-in flag.
- [x] The probe moves a window (Firefox on the 4K LG) to exactly the requested frame.
- [x] Opening an app, creating, focusing, moving and closing windows, and quitting an app print the matching events.
- [x] Unplugging a monitor or closing the lid prints `DisplaysChanged` after a 500 ms quiet period.

## Findings from real hardware (2026-10-18)
- Display reconfiguration callbacks are **only delivered to a running `NSApplication`**.
  With a bare `CFRunLoopRun` nothing arrives. The app (phase 3) and the probe's `watch`
  both run `NSApplication::run` as an accessory (menu-bar) app.
- A lid close or monitor unplug can produce two bursts about 2 s apart, so two
  `DisplaysChanged` events. The engine must make re-layout idempotent.
- Moving the Dock or menu bar fires only AppKit's "screen parameters changed" (the work
  area changes, not the displays). It is also reported as `DisplaysChanged` because the
  zones must be recomputed.
- AX repeats identical move/resize notifications; duplicates are filtered per window.
- Quitting an app with Cmd-Q produces `AppTerminated` but no per-window `WindowDestroyed`.
  The engine must drop every window of that pid.
- TextEdit's "Open" panel reports itself as a standard window, so it is placed like any other.
