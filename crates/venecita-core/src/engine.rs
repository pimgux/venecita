//! The placement engine: Venecita Dock's brain.
//!
//! The engine is a deterministic state machine with no OS access. It receives [`Input`]s
//! (window events, display changes, actions, config reloads) and answers with
//! [`Command`]s (move this window there, focus that one). The application layer feeds it
//! with real platform events and executes the commands; tests feed it with fake ones.
//!
//! ## Rules of placement
//!
//! * A window is placed when it **appears** and on **re-layout** (startup, display
//!   change, profile change, config change, "re-arrange all", resume). The engine never
//!   fights the user: moving a window by hand is remembered as its current frame but not
//!   undone.
//! * Only normal windows are placed (no dialogs, panels, minimized or fullscreen windows),
//!   and never windows of ignored applications.
//! * The first rule of the active layout matching the window's app decides its zone;
//!   windows without a rule go to the layout's dynamic zone (if any).
//! * Windows already at their target (within 1 point) get no command, so re-layout is
//!   idempotent and doesn't flicker.
//!
//! ## Stacks and tab bars
//!
//! Every window Venecita places belongs to the [`Stack`] of its zone. When a stack holds two
//! or more windows, a tab bar (`tabs.height` points tall) is reserved at the top of the
//! zone and the windows are placed below it. After every input the engine compares the
//! tab bars that should exist with the ones already shown and emits
//! [`Command::ShowTabBar`] / [`Command::HideTabBar`] for the differences. A window dragged
//! out of its zone by hand leaves its stack.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::action::{Action, DisplayTarget};
use crate::config::{Config, Layout, Placement, Rule};
use crate::display::{resolve_displays, unused_displays, DisplayInfo, ResolvedDisplay};
use crate::geometry::Rect;
use crate::grid::{snap, zone_rect, GridSize, Zone};
use crate::profile::{fitting_profiles, select_layout, LayoutRef};
use crate::rules::any_app_matches;
use crate::stack::Stack;
use crate::window::{PlatformEvent, WindowId, WindowInfo, WindowKind};

/// Frames closer than this (in points) are considered equal.
const FRAME_TOLERANCE: f64 = 1.0;

/// Something the engine must react to.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// The complete current state of the desktop: sent at startup, after displays
    /// change and after waking from sleep. Triggers a full re-layout.
    Resync {
        /// Connected displays.
        displays: Vec<DisplayInfo>,
        /// All current windows.
        windows: Vec<WindowInfo>,
        /// The focused window, if any.
        focused: Option<WindowId>,
    },
    /// A window event from the platform. `DisplaysChanged` and `SystemWoke` must be
    /// translated into [`Input::Resync`] by the caller (it has to query the platform).
    Platform(PlatformEvent),
    /// An action from a hotkey, the tray, the API or the CLI.
    Action(Action),
    /// The configuration file changed (already validated).
    ConfigChanged(Box<Config>),
}

/// What the application layer must do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Move/resize a window.
    SetFrame(WindowId, Rect),
    /// Focus a window.
    Focus(WindowId),
    /// Open the settings window.
    OpenSettings,
    /// Persist a rule created or changed by "pin here" in the configuration file.
    SaveRule(PinnedRule),
    /// Tell the user why an action did nothing.
    Notice(Notice),
    /// Show a tab bar, or update it if one with the same id is shown.
    ShowTabBar(TabBar),
    /// Hide the tab bar with this id.
    HideTabBar(String),
    /// Re-stack Venecita's overlays around the focused window (see [`Layering`]).
    Layering(Layering),
}

/// How Venecita's overlays are stacked relative to the other windows.
///
/// Sent on every focus change (the newly focused app raises its window above our overlays)
/// and whenever the dimming settings or the pause state change. The backend orders each tab
/// bar just above its zone's active window, so a bar never covers the focused window of
/// another app, and puts the dim layer just below the focused window.
#[derive(Debug, Clone, PartialEq)]
pub struct Layering {
    /// The focused window, if any.
    pub focused: Option<WindowId>,
    /// Opacity of the layer that darkens every window below the focused one, or `None` for
    /// no dimming.
    pub dim: Option<f64>,
}

/// A strip of tabs drawn above a zone holding several windows.
#[derive(Debug, Clone, PartialEq)]
pub struct TabBar {
    /// Stable id of the zone (`"lg43:0,0,4,4"`).
    pub id: String,
    /// Where to draw it (global top-left coordinates).
    pub frame: Rect,
    /// One tab per window, in stack order.
    pub tabs: Vec<Tab>,
}

/// One tab of a [`TabBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// The window it represents.
    pub window: WindowId,
    /// Text shown on the tab.
    pub label: String,
    /// Whether it is the window shown on top.
    pub active: bool,
}

/// Identifies a zone of a display: the key of a stack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ZoneKey {
    /// Display id from the configuration.
    pub display: String,
    /// The zone on that display.
    pub zone: Zone,
}

impl ZoneKey {
    fn of(placement: &Placement) -> Self {
        Self {
            display: placement.display.clone(),
            zone: placement.zone(),
        }
    }
}

impl fmt::Display for ZoneKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let z = self.zone;
        write!(f, "{}:{},{},{},{}", self.display, z.col, z.row, z.w, z.h)
    }
}

/// A rule to write to the configuration file (result of [`Action::PinHere`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedRule {
    /// Profile containing the layout.
    pub profile: String,
    /// Layout receiving the rule.
    pub layout: String,
    /// Index of the existing rule to update, or `None` to append a new one.
    pub rule_index: Option<usize>,
    /// App id for a new rule.
    pub app_id: String,
    /// Where the app goes.
    pub placement: Placement,
}

/// Why a window action could not be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// There is no focused (normal) window.
    NoFocusedWindow,
    /// No layout fits the connected displays, so there is nowhere to save a rule.
    NoActiveLayout,
    /// The window is on a display that is not part of the active layout.
    DisplayNotInLayout,
    /// There is no stack to cycle through (the window isn't stacked / the dynamic zone is empty).
    NoStack,
}

/// A profile as shown in the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStatus {
    /// Profile name.
    pub name: String,
    /// Whether one of its layouts fits the connected displays.
    pub fits: bool,
    /// Whether it is the profile in use.
    pub active: bool,
}

/// Snapshot of the engine state for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Active profile name (`None` when no layout fits the connected displays).
    pub profile: Option<String>,
    /// Active layout name.
    pub layout: Option<String>,
    /// `true` when the profile was chosen by hand instead of detected.
    pub manual: bool,
    /// `true` while automatic placement is paused.
    pub paused: bool,
    /// Every profile in configuration order.
    pub profiles: Vec<ProfileStatus>,
    /// Number of windows being tracked.
    pub windows: usize,
    /// Names of the connected displays the active layout doesn't use (no definition
    /// matches them, or the layout doesn't list them). Their windows go to the dynamic
    /// zone like any window without a rule.
    pub unused_displays: Vec<String>,
}

/// The placement engine. See the module docs.
#[derive(Debug)]
pub struct Engine {
    config: Config,
    displays: Vec<DisplayInfo>,
    resolved: Vec<ResolvedDisplay>,
    layout: Option<LayoutRef>,
    /// Profile chosen by hand (overrides detection while it fits).
    manual_profile: Option<String>,
    windows: BTreeMap<WindowId, WindowInfo>,
    focused: Option<WindowId>,
    paused: bool,
    /// Windows sharing each zone.
    stacks: BTreeMap<ZoneKey, Stack>,
    /// Tab bars currently shown, by id (to emit only changes).
    bars: BTreeMap<String, TabBar>,
    /// The last [`Command::Layering`] emitted (to emit only changes).
    layering: Option<Layering>,
    /// Extra focus mode, while it is on.
    focus_mode: Option<FocusMode>,
    /// Windows focus mode resized. Their moves are not read as the user dragging them out
    /// of their zone until they are back in it (the platform reports our own resizes, and
    /// a report can arrive after the window was already restored).
    focus_touched: HashSet<WindowId>,
    /// The last frame Venecita gave each window, to tell its own moves (which the macOS
    /// backend reports too) from the user's.
    placed: BTreeMap<WindowId, Rect>,
}

/// State of the extra focus mode.
#[derive(Debug, Clone, Default)]
struct FocusMode {
    /// The window currently enlarged, if the focused one could be.
    enlarged: Option<Enlarged>,
}

/// A window enlarged by focus mode.
#[derive(Debug, Clone, Copy)]
struct Enlarged {
    id: WindowId,
    /// Where it goes back to when it loses the focus or the mode ends.
    saved: Rect,
    /// Its enlarged frame.
    big: Rect,
}

impl Engine {
    /// Creates an engine. Nothing is placed until the first [`Input::Resync`].
    pub fn new(config: Config) -> Self {
        Self {
            config,
            displays: Vec::new(),
            resolved: Vec::new(),
            layout: None,
            manual_profile: None,
            windows: BTreeMap::new(),
            focused: None,
            paused: false,
            stacks: BTreeMap::new(),
            bars: BTreeMap::new(),
            layering: None,
            focus_mode: None,
            focus_touched: HashSet::new(),
            placed: BTreeMap::new(),
        }
    }

    /// The stacks of windows sharing a zone (only those with at least one window).
    pub fn stacks(&self) -> impl Iterator<Item = (&ZoneKey, &Stack)> {
        self.stacks.iter()
    }

    /// The configuration in use.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Windows currently tracked.
    pub fn windows(&self) -> impl Iterator<Item = &WindowInfo> {
        self.windows.values()
    }

    /// The focused window, as last reported.
    pub fn focused(&self) -> Option<WindowId> {
        self.focused
    }

    /// Why a window is (or isn't) where it is, for the logs: the rule it matched, the
    /// dynamic zone, or why Venecita leaves it alone.
    pub fn explain(&self, id: WindowId) -> String {
        let Some(w) = self.windows.get(&id) else {
            return "unknown window".into();
        };
        if self.paused {
            return "paused".into();
        }
        if w.kind != WindowKind::Normal {
            return format!("left alone ({:?})", w.kind);
        }
        if any_app_matches(&self.config.general.ignore, &w.app) {
            return "left alone (ignored app)".into();
        }
        let Some(layout) = self.active_layout() else {
            return "no layout fits the connected displays".into();
        };
        let zone = |p: &Placement| format!("{} {},{} {}x{}", p.display, p.col, p.row, p.w, p.h);
        match layout
            .rules
            .iter()
            .position(|r| any_app_matches(&r.apps, &w.app))
        {
            Some(i) => {
                let rule = &layout.rules[i];
                if self.display(&rule.zone.display).is_some() {
                    format!(
                        "rule {} of layout `{}` → {}",
                        i + 1,
                        layout.name,
                        zone(&rule.zone)
                    )
                } else {
                    format!(
                        "rule {} of layout `{}` points to display `{}`, which isn't connected → dynamic zone",
                        i + 1,
                        layout.name,
                        rule.zone.display
                    )
                }
            }
            None => match &layout.dynamic_zone {
                Some(dz) => format!(
                    "no rule of layout `{}` names this app → dynamic zone {}",
                    layout.name,
                    zone(dz)
                ),
                None => format!(
                    "no rule of layout `{}` names this app; no dynamic zone",
                    layout.name
                ),
            },
        }
    }

    /// Whether a reported move of `id` to `frame` was made by the user: the window is one
    /// Venecita arranges, it isn't where Venecita last put it, and focus mode isn't
    /// handling it. Used for automatic arranging (`general.auto_arrange`).
    pub fn is_user_move(&self, id: WindowId, frame: Rect) -> bool {
        let Some(window) = self.windows.get(&id) else {
            return false;
        };
        if self.focus_touched.contains(&id) || self.assignment(window).is_none() {
            return false;
        }
        !self
            .placed
            .get(&id)
            .is_some_and(|p| p.approx_eq(&frame, 2.0))
    }

    /// Processes one input and returns the commands to execute.
    pub fn handle(&mut self, input: Input) -> Vec<Command> {
        // A focus change always re-stacks: the app that got the focus raised its window
        // above our overlays, even if it is the window we already thought was focused.
        let focus_event = matches!(
            input,
            Input::Platform(PlatformEvent::WindowFocused { .. }) | Input::Resync { .. }
        );
        let mut commands = self.handle_input(input);
        self.keep_enlarged(&mut commands);
        for c in &commands {
            if let Command::SetFrame(id, frame) = c {
                self.placed.insert(*id, *frame);
            }
        }
        commands.extend(self.sync_tab_bars());
        commands.extend(self.sync_layering(focus_event));
        commands
    }

    /// Emits [`Command::Layering`] if the desired stacking changed (or `force`).
    fn sync_layering(&mut self, force: bool) -> Option<Command> {
        let dim = &self.config.dim;
        let opacity = if self.focus_mode.is_some() {
            Some(self.config.focus.dim_opacity)
        } else {
            (dim.enabled && !self.paused).then_some(dim.opacity)
        };
        let layering = Layering {
            focused: self.focused,
            dim: opacity.filter(|_| self.focused.is_some()),
        };
        if !force && self.layering.as_ref() == Some(&layering) {
            return None;
        }
        self.layering = Some(layering.clone());
        Some(Command::Layering(layering))
    }

    fn handle_input(&mut self, input: Input) -> Vec<Command> {
        match input {
            Input::Resync {
                displays,
                windows,
                focused,
            } => {
                self.displays = displays;
                self.windows = windows.into_iter().map(|w| (w.id, w)).collect();
                self.focused = focused;
                let mut commands = self.exit_focus_mode();
                self.refresh_layout();
                commands.extend(self.relayout_all());
                commands
            }
            Input::Platform(event) => self.on_platform_event(event),
            Input::Action(action) => self.on_action(action),
            Input::ConfigChanged(config) => {
                let mut commands = self.exit_focus_mode();
                self.config = *config;
                if let Some(name) = &self.manual_profile {
                    if self.config.profile(name).is_none() {
                        self.manual_profile = None;
                    }
                }
                self.refresh_layout();
                commands.extend(self.relayout_all());
                commands
            }
        }
    }

    fn on_platform_event(&mut self, event: PlatformEvent) -> Vec<Command> {
        match event {
            PlatformEvent::WindowCreated(info) => {
                let id = info.id;
                let now_normal = info.kind == WindowKind::Normal;
                // The backend reports a window again when it starts observing an app, and
                // whenever the window gets the focus. Place it the first time, or when it
                // just became a normal window (it appeared as a splash screen or a dialog,
                // or was minimized); otherwise leave it where the user put it.
                let place = match self.windows.insert(id, info) {
                    None => true,
                    Some(old) => now_normal && old.kind != WindowKind::Normal,
                };
                if place {
                    self.place_window(id)
                } else {
                    Vec::new()
                }
            }
            PlatformEvent::WindowDestroyed { id } => self.forget(&[id]),
            PlatformEvent::AppTerminated { pid } => {
                let gone: Vec<WindowId> = self
                    .windows
                    .values()
                    .filter(|w| w.pid == pid)
                    .map(|w| w.id)
                    .collect();
                self.forget(&gone)
            }
            PlatformEvent::WindowFocused { id } => {
                self.focused = Some(id);
                if let Some(key) = self.stack_of(id) {
                    if let Some(stack) = self.stacks.get_mut(&key) {
                        stack.activate(id);
                    }
                }
                self.follow_focus()
            }
            PlatformEvent::WindowMoved { id, frame } => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.frame = frame;
                }
                if self.focus_touched.contains(&id) {
                    let enlarged = self.enlarged().is_some_and(|e| e.id == id);
                    let (cx, cy) = frame.center();
                    let back_home = self
                        .stack_of(id)
                        .and_then(|key| self.key_rect(&key))
                        .map_or(true, |r| r.contains_point(cx, cy));
                    if enlarged || !back_home {
                        return Vec::new();
                    }
                    self.focus_touched.remove(&id);
                }
                // Dragged out of its zone by hand: it leaves the stack.
                let left_zone = self.stack_of(id).filter(|key| {
                    let (cx, cy) = frame.center();
                    self.key_rect(key)
                        .is_some_and(|r| !r.contains_point(cx, cy))
                });
                match left_zone {
                    Some(key) => {
                        self.unassign(id);
                        self.place_stack(&key)
                    }
                    None => Vec::new(),
                }
            }
            PlatformEvent::TabClicked { id } => self.focus_window(id),
            // Handled by the caller via `Input::Resync`; nothing to do with app launches.
            PlatformEvent::DisplaysChanged
            | PlatformEvent::SystemWoke
            | PlatformEvent::AppLaunched { .. } => Vec::new(),
        }
    }

    fn on_action(&mut self, action: Action) -> Vec<Command> {
        // Everything that arranges windows ends focus mode first, so it works on the
        // windows' real places. Focus changes and stack cycling keep it on.
        let keeps_focus_mode = matches!(
            action,
            Action::FocusModeToggle
                | Action::StackNext
                | Action::StackPrev
                | Action::DynamicNext
                | Action::DynamicPrev
                | Action::FocusWindow { .. }
                | Action::OpenSettings
        );
        if !keeps_focus_mode && self.focus_mode.is_some() {
            let mut commands = self.exit_focus_mode();
            commands.extend(self.on_action(action));
            return commands;
        }
        match action {
            Action::RelayoutAll => self.relayout_all(),
            Action::ActivateProfile { name } => match self.config.profile(&name) {
                Some(p) => {
                    self.manual_profile = Some(p.name.clone());
                    self.refresh_layout();
                    self.relayout_all()
                }
                None => Vec::new(),
            },
            Action::NextProfile => {
                let connected = self.connected_ids();
                let fitting = fitting_profiles(&self.config.profiles, &connected);
                if fitting.is_empty() {
                    return Vec::new();
                }
                let current = self.layout.map(|l| l.profile);
                let pos = current.and_then(|c| fitting.iter().position(|&p| p == c));
                let next = fitting[pos.map_or(0, |p| (p + 1) % fitting.len())];
                self.manual_profile = Some(self.config.profiles[next].name.clone());
                self.refresh_layout();
                self.relayout_all()
            }
            Action::AutoProfile => {
                self.manual_profile = None;
                self.refresh_layout();
                self.relayout_all()
            }
            Action::TogglePause => {
                self.paused = !self.paused;
                if self.paused {
                    Vec::new()
                } else {
                    self.relayout_all()
                }
            }
            Action::OpenSettings => vec![Command::OpenSettings],
            Action::MoveZone { dir } => self.edit_focused_zone(|z, g| z.moved(dir, g)),
            Action::Grow { dir } => self.edit_focused_zone(|z, g| z.grown(dir, g)),
            Action::Shrink { dir } => self.edit_focused_zone(|z, g| z.shrunk(dir, g)),
            Action::SnapToGrid => self.edit_focused_zone(|z, _| z),
            Action::MoveToDisplay { target } => self.move_focused_to_display(target),
            Action::PinHere => self.pin_focused(),
            Action::StackNext => self.cycle_focused_stack(true),
            Action::StackPrev => self.cycle_focused_stack(false),
            Action::DynamicNext => self.cycle_dynamic(true),
            Action::DynamicPrev => self.cycle_dynamic(false),
            Action::FocusWindow { id } => self.focus_window(id),
            Action::FocusModeToggle => self.toggle_focus_mode(),
        }
    }

    /// Ids of the display definitions currently connected.
    fn connected_ids(&self) -> HashSet<&str> {
        self.resolved.iter().map(|r| r.def_id.as_str()).collect()
    }

    /// Re-resolves displays and selects the layout.
    fn refresh_layout(&mut self) {
        self.resolved = resolve_displays(&self.config.displays, &self.displays);
        let connected: HashSet<&str> = self.resolved.iter().map(|r| r.def_id.as_str()).collect();
        self.layout = select_layout(
            &self.config.profiles,
            &connected,
            self.manual_profile.as_deref(),
        );
    }

    /// The active layout, if any.
    fn active_layout(&self) -> Option<&Layout> {
        self.layout
            .and_then(|r| r.get(&self.config.profiles))
            .map(|(_, l)| l)
    }

    /// The connected display a definition id resolved to.
    fn display(&self, def_id: &str) -> Option<&DisplayInfo> {
        let handle = &self.resolved.iter().find(|r| r.def_id == def_id)?.handle;
        self.displays.iter().find(|d| &d.handle == handle)
    }

    /// The zone a window belongs to, or `None` if Venecita must leave it alone.
    fn assignment(&self, window: &WindowInfo) -> Option<ZoneKey> {
        if self.paused
            || window.kind != WindowKind::Normal
            || any_app_matches(&self.config.general.ignore, &window.app)
        {
            return None;
        }
        let layout = self.active_layout()?;
        let connected = |p: &&Placement| self.display(&p.display).is_some();
        layout
            .rules
            .iter()
            .find(|r| any_app_matches(&r.apps, &window.app))
            .map(|r| &r.zone)
            .filter(connected)
            .or_else(|| layout.dynamic_zone.as_ref().filter(connected))
            .map(ZoneKey::of)
    }

    /// The dynamic zone of the active layout.
    fn dynamic_key(&self) -> Option<ZoneKey> {
        self.active_layout()?.dynamic_zone.as_ref().map(ZoneKey::of)
    }

    /// Pixel rectangle of a whole zone.
    fn key_rect(&self, key: &ZoneKey) -> Option<Rect> {
        let display = self.display(&key.display)?;
        let grid = self.config.grid_for(&key.display);
        Some(zone_rect(
            display.work_area,
            grid,
            key.zone,
            self.config.general.gap,
        ))
    }

    /// Height of the tab bar, limited so a window always keeps most of a tiny zone.
    fn tab_height(&self, zone: &Rect) -> f64 {
        self.config.tabs.height.min(zone.h / 3.0)
    }

    /// Where the windows of a stack go: the whole zone, or below the tab bar when stacked.
    fn window_rect(&self, key: &ZoneKey, stack_len: usize) -> Option<Rect> {
        let r = self.key_rect(key)?;
        Some(if stack_len > 1 {
            r.inset(0.0, self.tab_height(&r), 0.0, 0.0)
        } else {
            r
        })
    }

    /// The stack a window belongs to.
    fn stack_of(&self, id: WindowId) -> Option<ZoneKey> {
        self.stacks
            .iter()
            .find(|(_, s)| s.contains(id))
            .map(|(k, _)| k.clone())
    }

    /// Removes a window from its stack (dropping empty stacks). Returns the old key.
    fn unassign(&mut self, id: WindowId) -> Option<ZoneKey> {
        let key = self.stack_of(id)?;
        if let Some(stack) = self.stacks.get_mut(&key) {
            stack.remove(id);
            if stack.is_empty() {
                self.stacks.remove(&key);
            }
        }
        Some(key)
    }

    /// Puts a window in the stack of `key` (leaving its previous stack). Returns the keys
    /// whose stacks changed.
    fn assign(&mut self, id: WindowId, key: ZoneKey) -> Vec<ZoneKey> {
        if self.stacks.get(&key).is_some_and(|s| s.contains(id)) {
            return Vec::new();
        }
        let mut changed: Vec<ZoneKey> = self.unassign(id).into_iter().collect();
        self.stacks.entry(key.clone()).or_default().push(id);
        changed.push(key);
        changed
    }

    /// Moves a window to `rect` unless it is already there.
    fn set_frame(&mut self, id: WindowId, rect: Rect) -> Option<Command> {
        let w = self.windows.get_mut(&id)?;
        if w.frame.approx_eq(&rect, FRAME_TOLERANCE) {
            return None;
        }
        w.frame = rect;
        Some(Command::SetFrame(id, rect))
    }

    /// Places every window of a stack (they all share the same frame).
    fn place_stack(&mut self, key: &ZoneKey) -> Vec<Command> {
        let Some(members) = self.stacks.get(key).map(|s| s.windows().to_vec()) else {
            return Vec::new();
        };
        let Some(rect) = self.window_rect(key, members.len()) else {
            return Vec::new();
        };
        members
            .into_iter()
            .filter_map(|id| self.set_frame(id, rect))
            .collect()
    }

    /// Places one (new) window and re-places the stacks it changed.
    fn place_window(&mut self, id: WindowId) -> Vec<Command> {
        let Some(window) = self.windows.get(&id) else {
            return Vec::new();
        };
        let changed = match self.assignment(window) {
            Some(key) => self.assign(id, key),
            None => Vec::new(),
        };
        changed.iter().flat_map(|k| self.place_stack(k)).collect()
    }

    /// Forgets closed windows and re-places the stacks they leave.
    fn forget(&mut self, ids: &[WindowId]) -> Vec<Command> {
        let mut changed = Vec::new();
        for &id in ids {
            self.windows.remove(&id);
            self.focus_touched.remove(&id);
            self.placed.remove(&id);
            if let Some(mode) = &mut self.focus_mode {
                if mode.enlarged.is_some_and(|e| e.id == id) {
                    mode.enlarged = None;
                }
            }
            if self.focused == Some(id) {
                self.focused = None;
            }
            changed.extend(self.unassign(id));
        }
        changed.iter().flat_map(|k| self.place_stack(k)).collect()
    }

    /// Focuses a window, activating it in its stack.
    fn focus_window(&mut self, id: WindowId) -> Vec<Command> {
        if !self.windows.contains_key(&id) {
            return Vec::new();
        }
        if let Some(key) = self.stack_of(id) {
            if let Some(stack) = self.stacks.get_mut(&key) {
                stack.activate(id);
            }
        }
        self.focused = Some(id);
        vec![Command::Focus(id)]
    }

    /// Shows the next/previous window of a stack.
    fn cycle(&mut self, key: &ZoneKey, forward: bool) -> Vec<Command> {
        let next = self.stacks.get_mut(key).and_then(|s| {
            if s.len() < 2 {
                None
            } else if forward {
                s.activate_next()
            } else {
                s.activate_prev()
            }
        });
        match next {
            Some(id) => {
                self.focused = Some(id);
                vec![Command::Focus(id)]
            }
            None => vec![Command::Notice(Notice::NoStack)],
        }
    }

    /// `stack_next` / `stack_prev`: cycle the focused window's stack.
    fn cycle_focused_stack(&mut self, forward: bool) -> Vec<Command> {
        match self.focused.and_then(|id| self.stack_of(id)) {
            Some(key) => self.cycle(&key, forward),
            None => vec![Command::Notice(Notice::NoStack)],
        }
    }

    /// `dynamic_next` / `dynamic_prev`: cycle the dynamic zone, wherever the focus is.
    fn cycle_dynamic(&mut self, forward: bool) -> Vec<Command> {
        match self.dynamic_key() {
            Some(key) => self.cycle(&key, forward),
            None => vec![Command::Notice(Notice::NoStack)],
        }
    }

    /// The tab bars that should be visible now.
    fn desired_tab_bars(&self) -> BTreeMap<String, TabBar> {
        let mut bars = BTreeMap::new();
        for (key, stack) in &self.stacks {
            if stack.len() < 2 {
                continue;
            }
            let Some(zone) = self.key_rect(key) else {
                continue;
            };
            let frame = Rect::new(zone.x, zone.y, zone.w, self.tab_height(&zone));
            let active = stack.active();
            let tabs = stack
                .windows()
                .iter()
                .filter_map(|id| self.windows.get(id))
                .map(|w| Tab {
                    window: w.id,
                    label: tab_label(w),
                    active: Some(w.id) == active,
                })
                .collect();
            let id = key.to_string();
            bars.insert(id.clone(), TabBar { id, frame, tabs });
        }
        bars
    }

    /// Emits the commands that turn the visible tab bars into the desired ones.
    fn sync_tab_bars(&mut self) -> Vec<Command> {
        let desired = self.desired_tab_bars();
        let mut commands: Vec<Command> = self
            .bars
            .keys()
            .filter(|id| !desired.contains_key(*id))
            .map(|id| Command::HideTabBar(id.clone()))
            .collect();
        for (id, bar) in &desired {
            if self.bars.get(id) != Some(bar) {
                commands.push(Command::ShowTabBar(bar.clone()));
            }
        }
        self.bars = desired;
        commands
    }

    // --- Window actions (phase 4) ------------------------------------------------------

    /// The focused window, if it is one Venecita may move.
    fn focused_window(&self) -> Option<&WindowInfo> {
        let w = self.windows.get(&self.focused?)?;
        (w.kind == WindowKind::Normal).then_some(w)
    }

    /// The display a frame is on: the one containing its center, else the one it overlaps most.
    fn display_of(&self, frame: &Rect) -> Option<&DisplayInfo> {
        let (cx, cy) = frame.center();
        self.displays
            .iter()
            .find(|d| d.frame.contains_point(cx, cy))
            .or_else(|| {
                self.displays.iter().max_by(|a, b| {
                    let area =
                        |d: &DisplayInfo| d.frame.intersection(frame).map_or(0.0, |r| r.area());
                    area(a).total_cmp(&area(b))
                })
            })
    }

    /// Config id of a connected display, if it matched a definition.
    fn def_id_of(&self, display: &DisplayInfo) -> Option<&str> {
        self.resolved
            .iter()
            .find(|r| r.handle == display.handle)
            .map(|r| r.def_id.as_str())
    }

    /// Grid of a connected display (its own grid, or `general.grid` if unknown).
    fn grid_of(&self, display: &DisplayInfo) -> GridSize {
        self.def_id_of(display)
            .map_or(self.config.general.grid, |id| self.config.grid_for(id))
    }

    /// Display, grid and snapped zone of the focused window.
    fn focused_zone(&self) -> Option<(WindowId, DisplayInfo, GridSize, Zone)> {
        let w = self.focused_window()?;
        let display = self.display_of(&w.frame)?.clone();
        let grid = self.grid_of(&display);
        let zone = snap(display.work_area, grid, w.frame, self.config.general.gap);
        Some((w.id, display, grid, zone))
    }

    /// Moves a window to a zone of a display.
    fn put(
        &mut self,
        id: WindowId,
        display: &DisplayInfo,
        grid: GridSize,
        zone: Zone,
    ) -> Vec<Command> {
        let rect = zone_rect(display.work_area, grid, zone, self.config.general.gap);
        // A window moved by keyboard leaves its stack; the windows left behind are re-placed.
        let mut commands: Vec<Command> = match self.unassign(id) {
            Some(old) => self.place_stack(&old),
            None => Vec::new(),
        };
        commands.extend(self.set_frame(id, rect));
        commands
    }

    /// Applies `edit` to the focused window's current zone (move, grow, shrink, snap).
    fn edit_focused_zone(&mut self, edit: impl FnOnce(Zone, GridSize) -> Zone) -> Vec<Command> {
        let Some((id, display, grid, zone)) = self.focused_zone() else {
            return vec![Command::Notice(Notice::NoFocusedWindow)];
        };
        let new_zone = edit(zone, grid);
        self.put(id, &display, grid, new_zone)
    }

    /// Sends the focused window to the next/previous display (left to right), keeping the
    /// same proportion of the screen.
    fn move_focused_to_display(&mut self, target: DisplayTarget) -> Vec<Command> {
        let Some((id, display, grid, zone)) = self.focused_zone() else {
            return vec![Command::Notice(Notice::NoFocusedWindow)];
        };
        let mut order: Vec<&DisplayInfo> = self.displays.iter().collect();
        order.sort_by(|a, b| {
            a.frame
                .x
                .total_cmp(&b.frame.x)
                .then(a.frame.y.total_cmp(&b.frame.y))
        });
        let Some(pos) = order.iter().position(|d| d.handle == display.handle) else {
            return Vec::new();
        };
        let n = order.len();
        let next = match target {
            DisplayTarget::Next => (pos + 1) % n,
            DisplayTarget::Prev => (pos + n - 1) % n,
        };
        let dest = order[next].clone();
        let dest_grid = self.grid_of(&dest);
        self.put(id, &dest, dest_grid, zone.rescaled(grid, dest_grid))
    }

    /// Snaps the focused window and remembers that zone for its app in the active layout.
    fn pin_focused(&mut self) -> Vec<Command> {
        let Some((id, display, _, zone)) = self.focused_zone() else {
            return vec![Command::Notice(Notice::NoFocusedWindow)];
        };
        let Some(layout_ref) = self.layout else {
            return vec![Command::Notice(Notice::NoActiveLayout)];
        };
        let Some(def_id) = self.def_id_of(&display).map(str::to_string) else {
            return vec![Command::Notice(Notice::DisplayNotInLayout)];
        };
        let app = self.windows[&id].app.clone();
        let placement = Placement::new(&def_id, zone);

        let profile = &mut self.config.profiles[layout_ref.profile];
        let profile_name = profile.name.clone();
        let layout = &mut profile.layouts[layout_ref.layout];
        if !layout.displays.contains(&def_id) {
            return vec![Command::Notice(Notice::DisplayNotInLayout)];
        }
        // Update the app's existing rule, or add one, in memory right away…
        let rule_index = layout
            .rules
            .iter()
            .position(|r| any_app_matches(&r.apps, &app));
        match rule_index {
            Some(i) => layout.rules[i].zone = placement.clone(),
            None => layout.rules.push(Rule {
                apps: vec![app.id.clone()],
                zone: placement.clone(),
            }),
        }
        let pinned = PinnedRule {
            profile: profile_name,
            layout: layout.name.clone(),
            rule_index,
            app_id: app.id,
            placement,
        };
        // …move the window into the stack of its new zone, and ask the app layer to save
        // the rule in the file.
        let mut commands = self.place_window(id);
        commands.push(Command::SaveRule(pinned));
        commands
    }

    /// Re-assigns every tracked window to its zone and places every stack (in a fixed
    /// order, so output is deterministic). Windows keep their position in stacks they stay in.
    fn relayout_all(&mut self) -> Vec<Command> {
        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        for id in ids {
            match self.windows.get(&id).and_then(|w| self.assignment(w)) {
                Some(key) => {
                    self.assign(id, key);
                }
                None => {
                    self.unassign(id);
                }
            }
        }
        // The focused window is the one on top of its stack.
        if let Some(focused) = self.focused {
            for stack in self.stacks.values_mut() {
                stack.activate(focused);
            }
        }
        let keys: Vec<ZoneKey> = self.stacks.keys().cloned().collect();
        keys.iter().flat_map(|k| self.place_stack(k)).collect()
    }

    // --- Extra focus mode (phase 6) -----------------------------------------------------

    /// Whether extra focus mode is on.
    pub fn focus_mode(&self) -> bool {
        self.focus_mode.is_some()
    }

    fn enlarged(&self) -> Option<Enlarged> {
        self.focus_mode.as_ref().and_then(|m| m.enlarged)
    }

    /// `focus_mode_toggle`.
    fn toggle_focus_mode(&mut self) -> Vec<Command> {
        if self.focus_mode.is_some() {
            return self.exit_focus_mode();
        }
        if self.paused {
            return Vec::new();
        }
        if self.focused_window().is_none() {
            return vec![Command::Notice(Notice::NoFocusedWindow)];
        }
        self.focus_mode = Some(FocusMode::default());
        self.enlarge_focused()
    }

    /// Ends focus mode, putting the enlarged window back.
    fn exit_focus_mode(&mut self) -> Vec<Command> {
        if self.focus_mode.is_none() {
            return Vec::new();
        }
        let commands = self.restore_enlarged();
        self.focus_mode = None;
        commands
    }

    /// The focus moved while focus mode is on: the previous window goes back, the new one
    /// is enlarged.
    fn follow_focus(&mut self) -> Vec<Command> {
        if self.focus_mode.is_none() || self.enlarged().map(|e| e.id) == self.focused {
            return Vec::new();
        }
        let mut commands = self.restore_enlarged();
        commands.extend(self.enlarge_focused());
        commands
    }

    fn restore_enlarged(&mut self) -> Vec<Command> {
        let Some(e) = self.focus_mode.as_mut().and_then(|m| m.enlarged.take()) else {
            return Vec::new();
        };
        if let Some(w) = self.windows.get_mut(&e.id) {
            w.frame = e.saved;
            vec![Command::SetFrame(e.id, e.saved)]
        } else {
            Vec::new()
        }
    }

    /// Enlarges the focused window to `focus.size` of its display's work area, centered.
    /// Windows Venecita must not move (ignored apps, dialogs) stay as they are; the rest
    /// of the screen is dimmed all the same.
    fn enlarge_focused(&mut self) -> Vec<Command> {
        let Some(w) = self.focused_window() else {
            return Vec::new();
        };
        if any_app_matches(&self.config.general.ignore, &w.app) {
            return Vec::new();
        }
        let Some(display) = self.display_of(&w.frame) else {
            return Vec::new();
        };
        let big = display.work_area.centered_scaled(self.config.focus.size);
        let e = Enlarged {
            id: w.id,
            saved: w.frame,
            big,
        };
        if let Some(w) = self.windows.get_mut(&e.id) {
            w.frame = big;
        }
        self.focus_touched.insert(e.id);
        if let Some(mode) = &mut self.focus_mode {
            mode.enlarged = Some(e);
        }
        vec![Command::SetFrame(e.id, big)]
    }

    /// Keeps the enlarged window enlarged when its stack is re-placed while focus mode is
    /// on (e.g. another window joined its zone): the new place becomes where it returns to.
    fn keep_enlarged(&mut self, commands: &mut Vec<Command>) {
        let Some(e) = self.focus_mode.as_mut().and_then(|m| m.enlarged.as_mut()) else {
            return;
        };
        commands.retain(|c| match c {
            Command::SetFrame(id, rect) if *id == e.id && *rect != e.big => {
                e.saved = *rect;
                false
            }
            _ => true,
        });
    }

    /// A snapshot for the tray menu, the API, etc.
    pub fn status(&self) -> Status {
        let connected = self.connected_ids();
        let fitting = fitting_profiles(&self.config.profiles, &connected);
        let active = self.layout.map(|l| l.profile);
        let names = self.layout.and_then(|r| r.get(&self.config.profiles));
        Status {
            profile: names.map(|(p, _)| p.name.clone()),
            layout: names.map(|(_, l)| l.name.clone()),
            manual: self.manual_profile.is_some(),
            paused: self.paused,
            profiles: self
                .config
                .profiles
                .iter()
                .enumerate()
                .map(|(i, p)| ProfileStatus {
                    name: p.name.clone(),
                    fits: fitting.contains(&i),
                    active: active == Some(i),
                })
                .collect(),
            windows: self.windows.len(),
            unused_displays: unused_displays(
                &self.resolved,
                self.active_layout().map_or(&[][..], |l| &l.displays),
                &self.displays,
            )
            .into_iter()
            .map(|d| d.name.clone())
            .collect(),
        }
    }
}

/// Text of a tab: the app name, plus the window title when there is one.
fn tab_label(w: &WindowInfo) -> String {
    const MAX: usize = 60;
    let label = if w.title.trim().is_empty() || w.title == w.app.name {
        w.app.name.clone()
    } else {
        format!("{} — {}", w.app.name, w.title.trim())
    };
    if label.chars().count() > MAX {
        let cut: String = label.chars().take(MAX - 1).collect();
        format!("{cut}…")
    } else {
        label
    }
}
