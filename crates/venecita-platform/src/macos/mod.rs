//! macOS backend.
//!
//! * Windows are read and moved with the Accessibility API ([`ax`]). This requires the
//!   user to grant the Accessibility permission (System Settings → Privacy & Security →
//!   Accessibility).
//! * Displays come from CoreGraphics + `NSScreen` ([`displays`]).
//! * Events come from `NSWorkspace` notifications, per-application `AXObserver`s and the
//!   CoreGraphics display reconfiguration callback ([`events`]).
//!
//! [`MacPlatform`] must be created and used on the main thread, and events are delivered
//! only while a running **`NSApplication`** event loop (`NSApplication::run`) services the
//! main thread. A bare `CFRunLoopRun` is not enough: macOS does not deliver display
//! reconfiguration callbacks to processes without a running application (verified on
//! real hardware while unplugging a monitor and closing the lid).

pub mod ax;
pub mod displays;
mod events;
pub mod login;
mod overlay;
pub mod windows;

use std::cell::RefCell;
use std::rc::Rc;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSWorkspace};

use self::ax::{attr, Element};
use crate::{
    DisplayInfo, EventSink, Layering, PermissionStatus, Platform, PlatformError, Rect, Result,
    TabBar, WindowId, WindowInfo,
};

/// The macOS implementation of [`Platform`].
pub struct MacPlatform {
    mtm: MainThreadMarker,
    events: RefCell<Option<events::EventHub>>,
    /// Sink also used by the tab bars to report clicks.
    sink: overlay::SharedSink,
    tab_bars: overlay::TabBars,
    dim: overlay::DimLayer,
}

impl MacPlatform {
    /// Creates the backend. Requires the main thread.
    pub fn new(mtm: MainThreadMarker) -> Self {
        let sink: overlay::SharedSink = Rc::new(RefCell::new(None));
        Self {
            mtm,
            events: RefCell::new(None),
            tab_bars: overlay::TabBars::new(mtm, sink.clone()),
            dim: overlay::DimLayer::new(mtm),
            sink,
        }
    }

    fn require_permission(&self) -> Result<()> {
        match self.permissions(false) {
            PermissionStatus::Granted => Ok(()),
            PermissionStatus::Denied => Err(PlatformError::PermissionDenied),
        }
    }

    fn ax_error(
        operation: &'static str,
        err: objc2_application_services::AXError,
    ) -> PlatformError {
        PlatformError::Os {
            operation,
            code: i64::from(err.0),
        }
    }
}

impl Platform for MacPlatform {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        displays::list(self.mtm)
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        self.require_permission()?;
        Ok(windows::regular_apps()
            .iter()
            .flat_map(|app| {
                let pid = app.processIdentifier();
                windows::app_windows(pid, &windows::identity(app))
            })
            .collect())
    }

    fn window(&self, id: WindowId) -> Result<WindowInfo> {
        self.require_permission()?;
        let (pid, _) = windows::split_window_id(id);
        let app = windows::app_by_pid(pid).ok_or(PlatformError::WindowNotFound(id))?;
        let (_, el) = windows::find_window(id).ok_or(PlatformError::WindowNotFound(id))?;
        windows::window_info(pid, &windows::identity(&app), &el)
            .ok_or(PlatformError::WindowNotFound(id))
    }

    fn focused_window(&self) -> Result<Option<WindowId>> {
        self.require_permission()?;
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return Ok(None);
        };
        let pid = app.processIdentifier();
        Ok(Element::application(pid)
            .element(attr::FOCUSED_WINDOW)
            .and_then(|w| w.window_number())
            .map(|n| windows::window_id(pid, n)))
    }

    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()> {
        self.require_permission()?;
        let (app, win) = windows::find_window(id).ok_or(PlatformError::WindowNotFound(id))?;

        // Apps with "enhanced user interface" on animate frame changes; turn it off while
        // we move the window (same trick as Rectangle).
        let enhanced = app.bool(attr::ENHANCED_USER_INTERFACE) == Some(true);
        if enhanced {
            let _ = app.set_bool(attr::ENHANCED_USER_INTERFACE, false);
        }

        // Size → position → size: a window moved to a smaller display may be clamped by
        // the OS before the move, so the size is applied again once it is in place.
        let result = win
            .set_size(frame.w, frame.h)
            .and_then(|_| win.set_position(frame.x, frame.y))
            .and_then(|_| win.set_size(frame.w, frame.h))
            .map_err(|e| Self::ax_error("set window frame", e));

        if enhanced {
            let _ = app.set_bool(attr::ENHANCED_USER_INTERFACE, true);
        }
        result
    }

    fn focus(&self, id: WindowId) -> Result<()> {
        self.require_permission()?;
        let (pid, _) = windows::split_window_id(id);
        let (_, win) = windows::find_window(id).ok_or(PlatformError::WindowNotFound(id))?;
        // A tab can point to a minimized window: bring it back first.
        let _ = win.set_bool(attr::MINIMIZED, false);
        let _ = win.set_bool(attr::MAIN, true);
        win.raise().map_err(|e| Self::ax_error("raise window", e))?;
        if let Some(app) = windows::app_by_pid(pid) {
            app.activateWithOptions(NSApplicationActivationOptions::empty());
        }
        Ok(())
    }

    fn start_events(&self, sink: EventSink) -> Result<()> {
        self.require_permission()?;
        // Drop the previous hub first so its observers are unregistered.
        self.events.borrow_mut().take();
        let hub = events::EventHub::start(self.mtm, sink.clone())?;
        *self.events.borrow_mut() = Some(hub);
        *self.sink.borrow_mut() = Some(sink);
        Ok(())
    }

    fn stop_events(&self) {
        self.events.borrow_mut().take();
        self.sink.borrow_mut().take();
    }

    fn show_tab_bar(&self, bar: &TabBar) -> Result<()> {
        self.tab_bars.show(bar);
        Ok(())
    }

    fn hide_tab_bar(&self, id: &str) -> Result<()> {
        self.tab_bars.hide(id);
        Ok(())
    }

    fn set_layering(&self, layering: &Layering) -> Result<()> {
        // Dim layer first (below the focused window), then the bars above their windows:
        // the focused zone's bar ends up above the dim layer, the others below it.
        match (layering.focused, layering.dim) {
            (Some(focused), Some(opacity)) => self.dim.show(opacity, focused),
            _ => self.dim.hide(),
        }
        self.tab_bars.restack();
        Ok(())
    }

    fn set_launch_at_login(&self, enabled: bool) -> Result<()> {
        login::set_enabled(enabled)
    }

    fn permissions(&self, prompt: bool) -> PermissionStatus {
        if ax::is_trusted(prompt) {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }
}
