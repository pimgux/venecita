//! Connects the [`Engine`] to a [`Platform`].
//!
//! The driver translates platform events into engine inputs (querying the platform for
//! a full [`Input::Resync`] when displays change or the computer wakes up), executes the
//! engine's commands on the platform and hands back the commands the UI must handle.
//! It is generic over [`Platform`], so the exact same code runs against macOS and against
//! the in-memory fake in tests.

use tracing::{debug, info, warn};
use venecita_core::engine::{Command, Engine, Input, Notice, PinnedRule, Status};
use venecita_core::{Action, Config};
use venecita_platform::{Platform, PlatformEvent, Result};

/// Commands the platform can't execute and the UI must handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiRequest {
    /// Show the settings window.
    OpenSettings,
    /// Save a pinned rule in the configuration file.
    SaveRule(PinnedRule),
    /// Tell the user why an action did nothing.
    Notice(Notice),
}

/// Engine + platform.
pub struct Driver<P: Platform> {
    platform: P,
    engine: Engine,
}

impl<P: Platform> Driver<P> {
    /// Creates the driver. Call [`Driver::resync`] once the platform is ready.
    pub fn new(platform: P, config: Config) -> Self {
        Self {
            platform,
            engine: Engine::new(config),
        }
    }

    /// The platform.
    pub fn platform(&self) -> &P {
        &self.platform
    }

    /// The engine (read-only).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Engine status for the UI.
    pub fn status(&self) -> Status {
        self.engine.status()
    }

    /// Reads the whole desktop from the platform and re-lays out every window.
    pub fn resync(&mut self) -> Result<Vec<UiRequest>> {
        let displays = self.platform.displays()?;
        // While the screens sleep macOS reports no active display at all. Re-laying out then
        // would drop the layout and shuffle windows on wake: keep the current state instead.
        if displays.is_empty() {
            info!("no active display (screens asleep?): keeping the current layout");
            return Ok(Vec::new());
        }
        let windows = self.platform.windows()?;
        let focused = self.platform.focused_window()?;
        info!(displays = displays.len(), windows = windows.len(), "resync");
        let commands = self.engine.handle(Input::Resync {
            displays,
            windows,
            focused,
        });
        let requests = self.execute(commands);
        let s = self.engine.status();
        info!(profile = ?s.profile, layout = ?s.layout, "active layout");
        // Where every window went and why (which rule matched, or the dynamic zone).
        for w in self.engine.windows() {
            // No window titles: they can hold private text and logs get shared.
            info!(app = %w.app.id, name = %w.app.name, "{}", self.engine.explain(w.id));
        }
        Ok(requests)
    }

    /// Handles one platform event.
    pub fn on_event(&mut self, event: PlatformEvent) -> Vec<UiRequest> {
        debug!(?event, "platform event");
        match event {
            PlatformEvent::DisplaysChanged | PlatformEvent::SystemWoke => {
                self.resync().unwrap_or_else(|e| {
                    warn!(error = %e, "resync failed");
                    Vec::new()
                })
            }
            other => {
                let commands = self.engine.handle(Input::Platform(other));
                self.execute(commands)
            }
        }
    }

    /// Runs an action (from a hotkey, the tray, the API…).
    pub fn on_action(&mut self, action: Action) -> Vec<UiRequest> {
        info!(?action, "action");
        let commands = self.engine.handle(Input::Action(action));
        self.execute(commands)
    }

    /// Applies a new (already validated) configuration.
    pub fn reload_config(&mut self, config: Config) -> Vec<UiRequest> {
        info!("configuration reloaded");
        let commands = self.engine.handle(Input::ConfigChanged(Box::new(config)));
        self.execute(commands)
    }

    /// Executes engine commands; returns the ones meant for the UI.
    fn execute(&mut self, commands: Vec<Command>) -> Vec<UiRequest> {
        let mut ui = Vec::new();
        for command in commands {
            match command {
                Command::SetFrame(id, frame) => {
                    if let Err(e) = self.platform.set_frame(id, frame) {
                        // Windows can close between the event and the command: not fatal.
                        debug!(%id, error = %e, "set_frame failed");
                    }
                }
                Command::Focus(id) => {
                    if let Err(e) = self.platform.focus(id) {
                        debug!(%id, error = %e, "focus failed");
                    }
                }
                Command::OpenSettings => ui.push(UiRequest::OpenSettings),
                Command::SaveRule(rule) => ui.push(UiRequest::SaveRule(rule)),
                Command::Notice(notice) => ui.push(UiRequest::Notice(notice)),
                Command::ShowTabBar(bar) => {
                    if let Err(e) = self.platform.show_tab_bar(&bar) {
                        warn!(id = %bar.id, error = %e, "cannot show tab bar");
                    }
                }
                Command::Layering(layering) => {
                    if let Err(e) = self.platform.set_layering(&layering) {
                        warn!(error = %e, "cannot re-stack the overlays");
                    }
                }
                Command::HideTabBar(id) => {
                    if let Err(e) = self.platform.hide_tab_bar(&id) {
                        warn!(%id, error = %e, "cannot hide tab bar");
                    }
                }
            }
        }
        ui
    }
}
