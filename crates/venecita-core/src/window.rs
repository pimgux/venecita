//! Windows and the events the operating system reports about them.
//!
//! These types live in the core (not in `venecita-platform`) so the engine can be tested
//! without any OS backend. Platform backends produce them; the engine consumes them.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::rules::AppIdentity;

/// Opaque, platform-specific window identifier. Stable for the lifetime of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowId(pub u64);

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

/// Process identifier of the application owning a window.
pub type Pid = i32;

/// What kind of window this is. Only [`WindowKind::Normal`] windows are managed;
/// the others are reported so tools like the probe can show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    /// A regular document/application window.
    Normal,
    /// A dialog, sheet or alert.
    Dialog,
    /// A floating panel, palette or other special window.
    Floating,
    /// A window in native fullscreen.
    Fullscreen,
    /// A minimized window.
    Minimized,
}

/// A window as reported by the platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Window id.
    pub id: WindowId,
    /// Owning process.
    pub pid: Pid,
    /// Owning application.
    pub app: AppIdentity,
    /// Window title (may be empty).
    pub title: String,
    /// Current frame.
    pub frame: Rect,
    /// Kind of window.
    pub kind: WindowKind,
}

impl WindowInfo {
    /// Returns `true` for windows Venecita Dock should place.
    pub fn is_manageable(&self) -> bool {
        self.kind == WindowKind::Normal
    }
}

/// Something that happened in the window system.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    /// An application started.
    AppLaunched {
        /// Its process id.
        pid: Pid,
        /// Its identity.
        app: AppIdentity,
    },
    /// An application quit; all its windows are gone.
    AppTerminated {
        /// Its process id.
        pid: Pid,
    },
    /// A new window appeared (or an existing one was discovered).
    WindowCreated(WindowInfo),
    /// A window was closed.
    WindowDestroyed {
        /// The window.
        id: WindowId,
    },
    /// A window became the focused one.
    WindowFocused {
        /// The window.
        id: WindowId,
    },
    /// A window was moved or resized (by the user or by us).
    WindowMoved {
        /// The window.
        id: WindowId,
        /// Its new frame.
        frame: Rect,
    },
    /// Displays were connected, disconnected or rearranged (debounced).
    DisplaysChanged,
    /// The computer woke up from sleep.
    SystemWoke,
    /// The user clicked a tab of a tab bar drawn by Venecita.
    TabClicked {
        /// The window the tab represents.
        id: WindowId,
    },
}
