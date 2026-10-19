//! Windows backend (Windows 10 and 11).
//!
//! * Windows are listed with `EnumWindows` and moved with `SetWindowPos`, compensating the
//!   invisible resize borders ([`window`]).
//! * Displays come from `EnumDisplayMonitors` and DisplayConfig ([`displays`]).
//! * Events come from `SetWinEventHook` and a hidden window ([`events`]).
//!
//! Geometry is in physical pixels of the virtual desktop; the process must be per-monitor
//! DPI aware (v2), which the app declares in its manifest.
//!
//! [`WinPlatform`] must be created and used on one thread, which must run a Win32 message
//! loop: hooks and overlays deliver their callbacks through it.

mod displays;
mod events;
mod login;
mod overlay;
mod util;
mod window;

use std::cell::RefCell;
use std::rc::Rc;

use util::{hwnd, window_id};

use crate::{
    DisplayInfo, EventSink, Layering, PermissionStatus, Platform, PlatformError, Rect, Result,
    TabBar, WindowId, WindowInfo,
};

/// The Windows implementation of [`Platform`].
pub struct WinPlatform {
    events: RefCell<Option<events::EventHub>>,
    /// Sink also used by the tab bars to report clicks.
    sink: overlay::SharedSink,
    tab_bars: overlay::TabBars,
    dim: overlay::DimLayer,
}

impl Default for WinPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl WinPlatform {
    /// Creates the backend on the current (UI) thread.
    pub fn new() -> Self {
        let sink: overlay::SharedSink = Rc::new(RefCell::new(None));
        Self {
            events: RefCell::new(None),
            tab_bars: overlay::TabBars::new(sink.clone()),
            dim: overlay::DimLayer::new(),
            sink,
        }
    }
}

impl Platform for WinPlatform {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        displays::list()
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(window::handles()
            .into_iter()
            .filter_map(window::info)
            .collect())
    }

    fn window(&self, id: WindowId) -> Result<WindowInfo> {
        window::info(hwnd(id)).ok_or(PlatformError::WindowNotFound(id))
    }

    fn focused_window(&self) -> Result<Option<WindowId>> {
        Ok(window::foreground()
            .filter(|&h| window::eligible(h))
            .map(window_id))
    }

    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()> {
        window::set_frame(id, frame)
    }

    fn focus(&self, id: WindowId) -> Result<()> {
        window::focus(id)
    }

    fn start_events(&self, sink: EventSink) -> Result<()> {
        // Drop the previous hub first so its hooks are removed.
        self.events.borrow_mut().take();
        let hub = events::EventHub::start(sink.clone())?;
        *self.events.borrow_mut() = Some(hub);
        *self.sink.borrow_mut() = Some(sink);
        Ok(())
    }

    fn stop_events(&self) {
        self.events.borrow_mut().take();
        self.sink.borrow_mut().take();
    }

    /// Windows needs no special permission. (Windows of elevated apps can't be moved by a
    /// normal process; those moves fail individually with "access denied".)
    fn permissions(&self, _prompt: bool) -> PermissionStatus {
        PermissionStatus::Granted
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
        login::set(enabled)
    }
}
