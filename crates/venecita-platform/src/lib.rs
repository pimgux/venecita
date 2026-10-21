//! Operating system backends for Venecita Dock.
//!
//! The rest of the application only talks to the OS through the [`Platform`] trait:
//! listing displays and windows, moving windows, focusing them and receiving
//! [`PlatformEvent`]s. Each OS has its own implementation:
//!
//! | Backend | Status |
//! |---|---|
//! | [`macos::MacPlatform`] | Accessibility API + CoreGraphics + NSWorkspace |
//! | [`windows::WinPlatform`] | Win32: `EnumWindows`, `SetWinEventHook`, DisplayConfig |
//! | [`linux::X11Platform`] | X11 + EWMH (`_NET_CLIENT_LIST`, `_NET_MOVERESIZE_WINDOW`) + RandR |
//! | [`fake::FakePlatform`] | In-memory, used by tests on every OS |
//!
//! All coordinates follow the core convention: logical points, global desktop
//! coordinates, origin at the top-left of the primary display, y pointing down.

use std::sync::Arc;

pub use venecita_core::display::DisplayInfo;
pub use venecita_core::engine::{Layering, TabBar};
pub use venecita_core::geometry::Rect;
pub use venecita_core::rules::AppIdentity;
pub use venecita_core::window::{Pid, PlatformEvent, WindowId, WindowInfo, WindowKind};

pub mod contract;
mod debounce;
pub mod fake;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

pub use debounce::Debouncer;

/// Receives platform events. It may be called from any thread.
pub type EventSink = Arc<dyn Fn(PlatformEvent) + Send + Sync>;

/// Whether the OS lets Venecita Dock control other applications' windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Everything works.
    Granted,
    /// The user must grant a permission (Accessibility on macOS).
    Denied,
}

/// Errors returned by platform backends.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// The app lacks the permission to control windows.
    #[error("permission denied: grant Venecita Dock access in the system privacy settings")]
    PermissionDenied,
    /// The window no longer exists.
    #[error("window {0} not found")]
    WindowNotFound(WindowId),
    /// An OS call failed.
    #[error("{operation} failed (OS error {code})")]
    Os {
        /// What we were doing.
        operation: &'static str,
        /// The OS error code.
        code: i64,
    },
    /// The operation is not supported on this platform.
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
}

/// Result type of the platform API.
pub type Result<T, E = PlatformError> = std::result::Result<T, E>;

/// Everything Venecita Dock needs from the operating system.
///
/// On macOS the implementation must be created and used on the main thread, and events
/// are only delivered while an `NSApplication` event loop is running.
pub trait Platform {
    /// Currently connected displays.
    fn displays(&self) -> Result<Vec<DisplayInfo>>;

    /// All windows of regular applications, including non-manageable ones (check
    /// [`WindowInfo::is_manageable`]).
    fn windows(&self) -> Result<Vec<WindowInfo>>;

    /// A single window.
    fn window(&self, id: WindowId) -> Result<WindowInfo>;

    /// The window that has keyboard focus, if any.
    fn focused_window(&self) -> Result<Option<WindowId>>;

    /// Moves and resizes a window. The OS may adjust the frame (e.g. minimum sizes).
    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()>;

    /// Raises a window and gives it keyboard focus.
    fn focus(&self, id: WindowId) -> Result<()>;

    /// Starts delivering events to `sink`, replacing any previous sink.
    fn start_events(&self, sink: EventSink) -> Result<()>;

    /// Stops delivering events.
    fn stop_events(&self);

    /// Checks the window control permission. With `prompt = true` the OS may show its
    /// permission dialog.
    fn permissions(&self, prompt: bool) -> PermissionStatus;

    /// Shows a tab bar, or updates the one with the same id. Clicks on its tabs must be
    /// reported as [`PlatformEvent::TabClicked`].
    fn show_tab_bar(&self, _bar: &TabBar) -> Result<()> {
        Ok(())
    }

    /// Hides the tab bar with this id (no-op if it isn't shown).
    fn hide_tab_bar(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    /// Re-stacks the overlays around the focused window: every tab bar just above its
    /// zone's active window, and the dim layer (if any) just below the focused window.
    fn set_layering(&self, _layering: &Layering) -> Result<()> {
        Ok(())
    }

    /// Enables or disables starting at login.
    fn set_launch_at_login(&self, _enabled: bool) -> Result<()> {
        Err(PlatformError::Unsupported("launch at login"))
    }
}
