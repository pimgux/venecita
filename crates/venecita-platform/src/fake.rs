//! An in-memory [`Platform`] for tests.
//!
//! It holds a list of displays and windows, applies commands to them (so `set_frame` really
//! changes the frame), records every command for assertions, and lets tests simulate OS
//! activity (`open_window`, `close_window`, `set_displays`…) which emits the same events a
//! real backend would.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{
    AppIdentity, DisplayInfo, EventSink, Layering, PermissionStatus, Pid, Platform, PlatformError,
    PlatformEvent, Rect, Result, TabBar, WindowId, WindowInfo, WindowKind,
};

/// A command received by the fake platform.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeCommand {
    /// `set_frame` was called.
    SetFrame(WindowId, Rect),
    /// `focus` was called.
    Focus(WindowId),
    /// `set_launch_at_login` was called.
    LaunchAtLogin(bool),
    /// `show_tab_bar` was called.
    ShowTabBar(TabBar),
    /// `hide_tab_bar` was called.
    HideTabBar(String),
    /// `set_layering` was called.
    Layering(Layering),
}

#[derive(Default)]
struct State {
    displays: Vec<DisplayInfo>,
    windows: Vec<WindowInfo>,
    focused: Option<WindowId>,
    commands: Vec<FakeCommand>,
    sink: Option<EventSink>,
    next_id: u64,
    permission_denied: bool,
    launch_at_login: bool,
    tab_bars: BTreeMap<String, TabBar>,
    layering: Option<Layering>,
}

/// In-memory platform. Cheap to clone (clones share state).
#[derive(Clone, Default)]
pub struct FakePlatform {
    state: Arc<Mutex<State>>,
}

impl FakePlatform {
    /// A platform with the given displays and no windows.
    pub fn new(displays: Vec<DisplayInfo>) -> Self {
        let p = Self::default();
        p.lock().displays = displays;
        p
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic in another test thread must not cascade into confusing errors here.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends an event to the subscribed sink (outside the lock, so the sink may call back
    /// into the platform).
    fn emit(&self, event: PlatformEvent) {
        let sink = self.lock().sink.clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }

    /// Simulates an application opening a window. Returns its id.
    pub fn open_window(&self, pid: Pid, app: AppIdentity, title: &str, frame: Rect) -> WindowId {
        self.open_window_of_kind(pid, app, title, frame, WindowKind::Normal)
    }

    /// Like [`FakePlatform::open_window`] with an explicit kind (dialogs, panels…).
    pub fn open_window_of_kind(
        &self,
        pid: Pid,
        app: AppIdentity,
        title: &str,
        frame: Rect,
        kind: WindowKind,
    ) -> WindowId {
        let info = {
            let mut s = self.lock();
            s.next_id += 1;
            let info = WindowInfo {
                id: WindowId(s.next_id),
                pid,
                app,
                title: title.into(),
                frame,
                kind,
            };
            s.windows.push(info.clone());
            info
        };
        let id = info.id;
        self.emit(PlatformEvent::WindowCreated(info));
        id
    }

    /// Simulates the user closing a window.
    pub fn close_window(&self, id: WindowId) {
        {
            let mut s = self.lock();
            s.windows.retain(|w| w.id != id);
            if s.focused == Some(id) {
                s.focused = None;
            }
        }
        self.emit(PlatformEvent::WindowDestroyed { id });
    }

    /// Simulates an application quitting (its windows disappear).
    pub fn quit_app(&self, pid: Pid) {
        {
            let mut s = self.lock();
            s.windows.retain(|w| w.pid != pid);
        }
        self.emit(PlatformEvent::AppTerminated { pid });
    }

    /// Simulates the user focusing a window (Cmd-Tab, click…).
    pub fn user_focus(&self, id: WindowId) {
        self.lock().focused = Some(id);
        self.emit(PlatformEvent::WindowFocused { id });
    }

    /// Simulates the user dragging a window.
    pub fn user_move(&self, id: WindowId, frame: Rect) {
        let found = {
            let mut s = self.lock();
            s.windows
                .iter_mut()
                .find(|w| w.id == id)
                .map(|w| w.frame = frame)
                .is_some()
        };
        if found {
            self.emit(PlatformEvent::WindowMoved { id, frame });
        }
    }

    /// Simulates connecting/disconnecting displays (e.g. closing the laptop lid).
    pub fn set_displays(&self, displays: Vec<DisplayInfo>) {
        self.lock().displays = displays;
        self.emit(PlatformEvent::DisplaysChanged);
    }

    /// Simulates a click on the tab of `id` in a tab bar.
    pub fn click_tab(&self, id: WindowId) {
        self.emit(PlatformEvent::TabClicked { id });
    }

    /// The tab bars currently shown, by id.
    pub fn tab_bars(&self) -> BTreeMap<String, TabBar> {
        self.lock().tab_bars.clone()
    }

    /// The last layering applied with [`Platform::set_layering`].
    pub fn layering(&self) -> Option<Layering> {
        self.lock().layering.clone()
    }

    /// Makes [`Platform::permissions`] report [`PermissionStatus::Denied`].
    pub fn deny_permission(&self) {
        self.lock().permission_denied = true;
    }

    /// Every command received so far.
    pub fn commands(&self) -> Vec<FakeCommand> {
        self.lock().commands.clone()
    }

    /// Returns and clears the recorded commands.
    pub fn take_commands(&self) -> Vec<FakeCommand> {
        std::mem::take(&mut self.lock().commands)
    }

    /// Whether launch at login is currently enabled.
    pub fn launch_at_login(&self) -> bool {
        self.lock().launch_at_login
    }
}

impl Platform for FakePlatform {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(self.lock().displays.clone())
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(self.lock().windows.clone())
    }

    fn window(&self, id: WindowId) -> Result<WindowInfo> {
        self.lock()
            .windows
            .iter()
            .find(|w| w.id == id)
            .cloned()
            .ok_or(PlatformError::WindowNotFound(id))
    }

    fn focused_window(&self) -> Result<Option<WindowId>> {
        Ok(self.lock().focused)
    }

    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()> {
        let mut s = self.lock();
        let w = s
            .windows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or(PlatformError::WindowNotFound(id))?;
        w.frame = frame;
        s.commands.push(FakeCommand::SetFrame(id, frame));
        Ok(())
    }

    fn focus(&self, id: WindowId) -> Result<()> {
        let mut s = self.lock();
        if !s.windows.iter().any(|w| w.id == id) {
            return Err(PlatformError::WindowNotFound(id));
        }
        s.focused = Some(id);
        s.commands.push(FakeCommand::Focus(id));
        Ok(())
    }

    fn start_events(&self, sink: EventSink) -> Result<()> {
        self.lock().sink = Some(sink);
        Ok(())
    }

    fn stop_events(&self) {
        self.lock().sink = None;
    }

    fn permissions(&self, _prompt: bool) -> PermissionStatus {
        if self.lock().permission_denied {
            PermissionStatus::Denied
        } else {
            PermissionStatus::Granted
        }
    }

    fn show_tab_bar(&self, bar: &TabBar) -> Result<()> {
        let mut s = self.lock();
        s.tab_bars.insert(bar.id.clone(), bar.clone());
        s.commands.push(FakeCommand::ShowTabBar(bar.clone()));
        Ok(())
    }

    fn hide_tab_bar(&self, id: &str) -> Result<()> {
        let mut s = self.lock();
        s.tab_bars.remove(id);
        s.commands.push(FakeCommand::HideTabBar(id.to_string()));
        Ok(())
    }

    fn set_layering(&self, layering: &Layering) -> Result<()> {
        let mut s = self.lock();
        s.layering = Some(layering.clone());
        s.commands.push(FakeCommand::Layering(layering.clone()));
        Ok(())
    }

    fn set_launch_at_login(&self, enabled: bool) -> Result<()> {
        let mut s = self.lock();
        s.launch_at_login = enabled;
        s.commands.push(FakeCommand::LaunchAtLogin(enabled));
        Ok(())
    }
}

/// Ready-made display setups used by tests across the workspace.
pub mod setups {
    use super::*;

    /// Builds a display. `frame` is in points; `scale` is the backing scale factor.
    pub fn display(
        handle: &str,
        name: &str,
        builtin: bool,
        frame: Rect,
        scale: f64,
    ) -> DisplayInfo {
        DisplayInfo {
            handle: handle.into(),
            name: name.into(),
            builtin,
            vendor: None,
            model: None,
            serial: None,
            frame,
            // macOS menu bar: 25 points at the top of every display.
            work_area: Rect::new(frame.x, frame.y + 25.0, frame.w, frame.h - 25.0),
            scale,
        }
    }

    /// MacBook built-in panel at the origin (1512 × 982 points, Retina).
    pub fn laptop() -> DisplayInfo {
        display(
            "laptop",
            "Built-in Retina Display",
            true,
            Rect::new(0.0, 0.0, 1512.0, 982.0),
            2.0,
        )
    }

    /// 43" 4K monitor to the right of the laptop (3200 × 1800 points at 2×, as on the author's desk).
    pub fn lg43() -> DisplayInfo {
        display(
            "lg43",
            "LG Monitor",
            false,
            Rect::new(1512.0, -458.0, 3200.0, 1800.0),
            2.0,
        )
    }

    /// Two 27" 1440p monitors side by side above the laptop.
    pub fn work_pair() -> [DisplayInfo; 2] {
        [
            display(
                "dellA",
                "DELL U2723QE",
                false,
                Rect::new(-2560.0, -1440.0, 2560.0, 1440.0),
                2.0,
            ),
            display(
                "dellB",
                "DELL U2723QE",
                false,
                Rect::new(0.0, -1440.0, 2560.0, 1440.0),
                2.0,
            ),
        ]
    }

    /// Home, lid open.
    pub fn home_open() -> Vec<DisplayInfo> {
        vec![lg43(), laptop()]
    }

    /// Home, lid closed.
    pub fn home_closed() -> Vec<DisplayInfo> {
        vec![lg43()]
    }

    /// Work, lid open.
    pub fn work_open() -> Vec<DisplayInfo> {
        let [a, b] = work_pair();
        vec![b, laptop(), a] // deliberately not in left-to-right order
    }

    /// Laptop alone (travelling).
    pub fn laptop_only() -> Vec<DisplayInfo> {
        vec![laptop()]
    }

    /// Work, lid closed.
    pub fn work_closed() -> Vec<DisplayInfo> {
        work_pair().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn chrome() -> AppIdentity {
        AppIdentity::new("com.google.Chrome", "Google Chrome")
    }

    #[test]
    fn passes_the_platform_contract() {
        let p = FakePlatform::new(setups::home_open());
        let a = p.open_window(10, chrome(), "A", Rect::new(10.0, 40.0, 500.0, 400.0));
        p.open_window(
            11,
            AppIdentity::new("com.apple.Terminal", "Terminal"),
            "B",
            Rect::new(0.0, 0.0, 300.0, 200.0),
        );
        p.user_focus(a);
        crate::contract::check_all(&p);
    }

    #[test]
    fn events_are_emitted_for_simulated_activity() {
        let p = FakePlatform::new(setups::home_open());
        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink_events = events.clone();
        p.start_events(Arc::new(move |e| sink_events.lock().unwrap().push(e)))
            .unwrap();

        let id = p.open_window(1, chrome(), "t", Rect::new(0.0, 0.0, 10.0, 10.0));
        p.user_focus(id);
        p.user_move(id, Rect::new(5.0, 5.0, 10.0, 10.0));
        p.set_displays(setups::home_closed());
        p.close_window(id);
        p.quit_app(1);
        p.stop_events();
        p.open_window(2, chrome(), "ignored", Rect::default());

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 6);
        assert!(matches!(events[0], PlatformEvent::WindowCreated(ref w) if w.id == id));
        assert_eq!(events[1], PlatformEvent::WindowFocused { id });
        assert_eq!(
            events[2],
            PlatformEvent::WindowMoved {
                id,
                frame: Rect::new(5.0, 5.0, 10.0, 10.0)
            }
        );
        assert_eq!(events[3], PlatformEvent::DisplaysChanged);
        assert_eq!(events[4], PlatformEvent::WindowDestroyed { id });
        assert_eq!(events[5], PlatformEvent::AppTerminated { pid: 1 });
    }

    #[test]
    fn commands_are_recorded_and_applied() {
        let p = FakePlatform::new(setups::laptop_only());
        let id = p.open_window(1, chrome(), "t", Rect::new(0.0, 0.0, 10.0, 10.0));
        let target = Rect::new(100.0, 100.0, 200.0, 200.0);
        p.set_frame(id, target).unwrap();
        p.focus(id).unwrap();
        assert_eq!(p.window(id).unwrap().frame, target);
        assert_eq!(p.focused_window().unwrap(), Some(id));
        assert_eq!(
            p.take_commands(),
            vec![FakeCommand::SetFrame(id, target), FakeCommand::Focus(id)]
        );
        assert!(p.commands().is_empty());
        assert!(matches!(
            p.set_frame(WindowId(999), target),
            Err(PlatformError::WindowNotFound(_))
        ));
    }

    #[test]
    fn permission_can_be_denied() {
        let p = FakePlatform::default();
        assert_eq!(p.permissions(false), PermissionStatus::Granted);
        p.deny_permission();
        assert_eq!(p.permissions(true), PermissionStatus::Denied);
    }

    #[test]
    fn sink_may_call_back_into_the_platform() {
        // Real engines query the platform from inside the event handler.
        let p = FakePlatform::new(setups::home_open());
        let inner = p.clone();
        p.start_events(Arc::new(move |e| {
            if let PlatformEvent::WindowCreated(w) = e {
                inner
                    .set_frame(w.id, Rect::new(1.0, 2.0, 3.0, 4.0))
                    .unwrap();
            }
        }))
        .unwrap();
        let id = p.open_window(1, chrome(), "t", Rect::default());
        assert_eq!(p.window(id).unwrap().frame, Rect::new(1.0, 2.0, 3.0, 4.0));
    }
}
