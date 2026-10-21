//! The tray application, shared by every OS: tray icon and menu, global shortcuts,
//! config file watching, the settings window and the [`Driver`].
//!
//! What differs between operating systems (the event loop, running code on the UI thread,
//! opening files…) lives in the `os` module (`mac.rs` or `win.rs`).
//!
//! ## Threading
//!
//! The platform backend and the tray must be used on the UI (main) thread, but events
//! arrive from several threads (display debouncer, file watcher, menu clicks). Everything
//! is funnelled to the UI thread with [`post`], which schedules a closure there through
//! [`os::run_on_main`]; the closure receives the single [`App`] instance kept in a
//! thread-local.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tracing::{error, info, warn};
use tray_icon::menu::accelerator::Accelerator;
use tray_icon::menu::{
    CheckMenuItem, IconMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use venecita_api::{Call, Events, Handler, Query, Server};
use venecita_app::config_file;
use venecita_app::driver::{Driver, UiRequest};
use venecita_app::hotkeys::Bindings;
use venecita_app::i18n::Lang;
use venecita_app::icons::{icon_rgba, IconKind};
use venecita_app::menu::{self, AppFlags, MenuCommand, MenuEntry};
use venecita_app::{paths, token};
use venecita_core::{Action, Hotkey};
use venecita_platform::{Debouncer, PermissionStatus, Platform, PlatformEvent};

use crate::os;

/// Command-line flag that starts the settings window instead of the tray app.
pub const SETTINGS_FLAG: &str = "--settings";

/// Everything the running application owns. Lives on the UI thread only.
struct App {
    driver: Driver<os::Native>,
    tray: TrayIcon,
    lang: Lang,
    os_language: Option<String>,
    config_path: PathBuf,
    flags: AppFlags,
    /// Set once events are flowing (permission granted).
    started: bool,
    /// Shared with the permission polling thread.
    permission_granted: Arc<AtomicBool>,
    /// Keeps the config file watcher alive.
    _watcher: Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>,
    /// Registers global shortcuts with the OS (UI thread only).
    hotkey_manager: Option<GlobalHotKeyManager>,
    /// Currently registered shortcuts and what they do.
    hotkeys_registered: Vec<HotKey>,
    hotkey_actions: HashMap<u32, Action>,
    /// The settings window process, while it runs (see [`SETTINGS_FLAG`]).
    settings_window: Option<std::process::Child>,
    /// Fires once the user stops moving windows (automatic arranging).
    auto_arrange: Debouncer,
    /// When automatic arranging last ran: our own moves right after it are ignored.
    last_auto_arrange: Option<Instant>,
    /// The local API server, while `[api]` enables it.
    api: Option<Server>,
    /// What the running server was started with, to restart it only when that changes.
    api_settings: Option<(SocketAddr, bool, String)>,
    /// Events for the API's WebSocket clients.
    api_events: Events,
    /// The state last sent to them, to send only changes.
    last_state: Option<serde_json::Value>,
}

/// How long windows must stay still before automatic arranging puts them back.
const AUTO_ARRANGE_DELAY: Duration = Duration::from_millis(700);
/// Moves reported this soon after automatic arranging are Venecita's own.
const AUTO_ARRANGE_COOLDOWN: Duration = Duration::from_millis(1500);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` with the app on the UI thread, from any thread.
fn post(f: impl FnOnce(&mut App) + Send + 'static) {
    os::run_on_main(Box::new(move || {
        APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().as_mut() {
                f(app);
            }
        });
    }));
}

/// Entry point of the settings window process (`venecita-dock --settings`).
pub fn run_settings() {
    let _log_guard = init_logging();
    let config_path = paths::config_file();
    // Make sure there is a file to edit, even if the settings open before the app ran.
    let loaded = config_file::load_or_create(&config_path);
    if let Some(e) = &loaded.error {
        warn!("{e}");
    }
    if let Err(e) = venecita_app::settings::gui::run(config_path, os::os_language()) {
        error!(error = %e, "settings window failed");
    }
}

/// Entry point of the tray app.
pub fn run() {
    let _log_guard = init_logging();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Venecita Dock starting"
    );
    let ui = os::init();

    let config_path = paths::config_file();
    let loaded = config_file::load_or_create(&config_path);
    if loaded.created {
        info!(path = %config_path.display(), "created starter configuration");
    }
    if let Some(e) = &loaded.error {
        error!("{e}");
    }

    let os_language = os::os_language();
    let lang = Lang::resolve(loaded.config.general.language, os_language.as_deref());
    let launch_at_login = loaded.config.general.launch_at_login;
    let auto_arrange = loaded.config.general.auto_arrange;

    let tray = TrayIconBuilder::new()
        .with_icon(tray_image(os::tray_icon_color()))
        .with_icon_as_template(true)
        .with_tooltip("Venecita Dock")
        .with_menu(Box::new(Menu::new()))
        .build()
        .expect("cannot create the tray icon");

    let app = App {
        driver: Driver::new(os::new_platform(&ui), loaded.config),
        tray,
        lang,
        os_language,
        config_path,
        flags: AppFlags {
            permission_missing: false,
            config_error: loaded.error.is_some(),
            launch_at_login,
            auto_arrange,
            hotkey_conflicts: Vec::new(),
        },
        started: false,
        permission_granted: Arc::new(AtomicBool::new(false)),
        _watcher: None,
        hotkey_manager: GlobalHotKeyManager::new()
            .map_err(|e| error!(error = %e, "global shortcuts unavailable"))
            .ok(),
        hotkeys_registered: Vec::new(),
        settings_window: None,
        hotkey_actions: HashMap::new(),
        auto_arrange: Debouncer::new(AUTO_ARRANGE_DELAY, || post(|app| app.auto_arrange_now())),
        last_auto_arrange: None,
        api: None,
        api_settings: None,
        api_events: Events::new(),
        last_state: None,
    };
    APP.with(|cell| *cell.borrow_mut() = Some(app));

    GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            post(move |app| app.on_hotkey(event.id));
        }
    }));

    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let id = event.id.0.clone();
        post(move |app| {
            if let Some(command) = MenuCommand::from_id(&id) {
                app.on_menu(command);
            }
        });
    }));

    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            app.watch_config();
            app.register_hotkeys();
            app.apply_launch_at_login();
            app.try_start(true);
            app.apply_api();
            app.refresh_menu();
        }
    });

    os::run_loop(ui);
}

impl App {
    /// Starts receiving events once the window control permission is granted.
    fn try_start(&mut self, prompt: bool) {
        if self.started {
            return;
        }
        if self.driver.platform().permissions(prompt) == PermissionStatus::Denied {
            if !self.flags.permission_missing {
                warn!("window control permission missing; waiting for it");
                self.flags.permission_missing = true;
                self.poll_permission();
            }
            return;
        }
        self.flags.permission_missing = false;
        self.permission_granted.store(true, Ordering::SeqCst);

        let sink = Arc::new(|event: PlatformEvent| post(move |app| app.on_event(event)));
        if let Err(e) = self.driver.platform().start_events(sink) {
            error!(error = %e, "cannot start platform events");
            return;
        }
        self.started = true;
        let requests = self.driver.resync().unwrap_or_else(|e| {
            error!(error = %e, "initial resync failed");
            Vec::new()
        });
        self.handle_ui(requests);
        info!("running");
    }

    /// Re-checks the permission every 2 s until granted.
    fn poll_permission(&self) {
        let granted = self.permission_granted.clone();
        std::thread::spawn(move || {
            while !granted.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(2));
                post(|app| {
                    app.try_start(false);
                    app.refresh_menu();
                });
            }
        });
    }

    fn on_event(&mut self, event: PlatformEvent) {
        if let PlatformEvent::WindowMoved { id, frame } = &event {
            if self.wants_auto_arrange(*id, *frame) {
                self.auto_arrange.trigger();
            }
        }
        let focused = match &event {
            PlatformEvent::WindowFocused { id } => Some(*id),
            _ => None,
        };
        let affects_menu = matches!(
            event,
            PlatformEvent::DisplaysChanged | PlatformEvent::SystemWoke
        );
        let requests = self.driver.on_event(event);
        self.handle_ui(requests);
        if affects_menu {
            self.refresh_menu();
        }
        if let (Some(id), Some(_)) = (focused, &self.api) {
            self.api_events
                .publish(venecita_app::api::focus_event(self.driver.engine(), id));
        }
        self.publish_state();
    }

    /// Starts, restarts or stops the local API to match `[api]`. The token is created on
    /// first use (`token.secret` next to the config file).
    fn apply_api(&mut self) {
        let cfg = self.driver.engine().config().api.clone();
        if !cfg.enabled {
            if self.api.take().is_some() {
                info!("API stopped");
            }
            self.api_settings = None;
            return;
        }
        let Ok(bind) = cfg.bind.parse::<SocketAddr>() else {
            warn!(bind = %cfg.bind, "invalid api.bind; the API is off");
            return;
        };
        let token_path = token::path_for(&self.config_path);
        let token = match token::read(&token_path) {
            Some(t) => t,
            None => match token::rotate(&token_path) {
                Ok(t) => {
                    info!(path = %token_path.display(), "API token created");
                    t
                }
                Err(e) => {
                    error!(error = %e, "cannot create the API token; the API is off");
                    return;
                }
            },
        };
        let wanted = (bind, cfg.allow_lan, token.clone());
        if self.api.is_some() && self.api_settings.as_ref() == Some(&wanted) {
            return;
        }
        // Stop the old server first so its port is free.
        self.api = None;
        let handler: Handler = Arc::new(|call| post(move |app| app.on_api(call)));
        let settings = venecita_api::Settings {
            bind,
            allow_lan: cfg.allow_lan,
            token,
        };
        // The old listener closes asynchronously: retry briefly if the port is still busy.
        for attempt in 0..5 {
            match venecita_api::start(settings.clone(), handler.clone(), self.api_events.clone()) {
                Ok(server) => {
                    self.api = Some(server);
                    self.api_settings = Some(wanted);
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempt < 4 => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => {
                    error!(error = %e, %bind, "cannot start the local API");
                    return;
                }
            }
        }
    }

    /// A request from the local API.
    fn on_api(&mut self, call: Call) {
        use venecita_app::api;
        match call {
            Call::Query { query, reply } => {
                let engine = self.driver.engine();
                let value = match query {
                    Query::State => api::state(engine),
                    Query::Displays => {
                        let connected = self.driver.platform().displays().unwrap_or_default();
                        api::displays(engine, &connected)
                    }
                    Query::Windows => api::windows(engine),
                    Query::Profiles => api::profiles(engine),
                };
                let _ = reply.send(value);
            }
            Call::Action { action, reply } => {
                let result = api::check_action(self.driver.engine(), &action);
                if result.is_ok() {
                    info!(?action, "action from the API");
                    self.on_action(action);
                }
                let _ = reply.send(result);
            }
        }
    }

    /// Tells WebSocket clients about state changes (and profile changes in particular).
    fn publish_state(&mut self) {
        if self.api.is_none() {
            return;
        }
        let state = venecita_app::api::state(self.driver.engine());
        if self.last_state.as_ref() == Some(&state) {
            return;
        }
        let profile_changed = self.last_state.as_ref().is_some_and(|last| {
            last["profile"] != state["profile"] || last["layout"] != state["layout"]
        });
        if profile_changed {
            self.api_events.publish(serde_json::json!({
                "type": "profile",
                "profile": state["profile"],
                "layout": state["layout"],
            }));
        }
        self.api_events
            .publish(serde_json::json!({ "type": "state", "state": state }));
        self.last_state = Some(state);
    }

    /// Whether a reported move should make automatic arranging put windows back.
    fn wants_auto_arrange(
        &self,
        id: venecita_platform::WindowId,
        frame: venecita_platform::Rect,
    ) -> bool {
        let engine = self.driver.engine();
        self.flags.auto_arrange
            && !self.driver.status().paused
            && !engine.focus_mode()
            && self
                .last_auto_arrange
                .map_or(true, |t| t.elapsed() > AUTO_ARRANGE_COOLDOWN)
            && engine.is_user_move(id, frame)
    }

    /// The user stopped moving windows: put everything back in its zone.
    fn auto_arrange_now(&mut self) {
        if !self.flags.auto_arrange {
            return;
        }
        // Still dragging (the window just paused under the mouse): wait for the release.
        if os::mouse_button_down() {
            self.auto_arrange.trigger();
            return;
        }
        self.last_auto_arrange = Some(Instant::now());
        self.on_action(Action::RelayoutAll);
    }

    fn on_action(&mut self, action: Action) {
        let requests = self.driver.on_action(action);
        self.handle_ui(requests);
        self.refresh_menu();
        self.publish_state();
    }

    fn on_menu(&mut self, command: MenuCommand) {
        match command {
            MenuCommand::Profile(name) => self.on_action(Action::ActivateProfile { name }),
            MenuCommand::ToggleAutoProfile => {
                match menu::toggle_auto_profile(&self.driver.status()) {
                    Some(action) => self.on_action(action),
                    // Nothing to toggle; rebuild so the check mark the click flipped is restored.
                    None => self.refresh_menu(),
                }
            }
            MenuCommand::RelayoutAll => self.on_action(Action::RelayoutAll),
            MenuCommand::TogglePause => self.on_action(Action::TogglePause),
            MenuCommand::ToggleAutoArrange => {
                let enabled = !self.flags.auto_arrange;
                // Persist in the file; the watcher reloads it and applies the change.
                if let Err(e) = config_file::set_auto_arrange(&self.config_path, enabled) {
                    error!(error = %e, "cannot update auto_arrange in the config file");
                }
            }
            MenuCommand::ToggleLaunchAtLogin => {
                let enabled = !self.flags.launch_at_login;
                // Persist in the file; the watcher reloads it and applies the change.
                if let Err(e) = config_file::set_launch_at_login(&self.config_path, enabled) {
                    error!(error = %e, "cannot update launch_at_login in the config file");
                }
            }
            MenuCommand::OpenSettings => self.open_settings(),
            MenuCommand::OpenConfig => venecita_app::system::open_text_file(&self.config_path),
            MenuCommand::ReloadConfig => self.reload_config(),
            MenuCommand::ShowLogs => os::open_folder(&paths::log_dir()),
            MenuCommand::OpenPermissionSettings => os::open_permission_settings(),
            MenuCommand::Quit => {
                info!("quitting");
                os::quit();
            }
        }
    }

    /// Shows the settings window: brings it to the front if it is already open, otherwise
    /// starts it as a separate process (it runs its own event loop, see
    /// `venecita_app::settings::gui`). Saving there rewrites the config file, which the
    /// watcher reloads.
    fn open_settings(&mut self) {
        if let Some(child) = &mut self.settings_window {
            if matches!(child.try_wait(), Ok(None)) {
                os::bring_to_front(child.id());
                return;
            }
        }
        let spawned = std::env::current_exe()
            .and_then(|exe| std::process::Command::new(exe).arg(SETTINGS_FLAG).spawn());
        match spawned {
            Ok(child) => {
                info!(pid = child.id(), "settings window opened");
                self.settings_window = Some(child);
            }
            Err(e) => {
                error!(error = %e, "cannot open the settings window; opening the file instead");
                self.on_menu(MenuCommand::OpenConfig);
            }
        }
    }

    fn handle_ui(&mut self, requests: Vec<UiRequest>) {
        for request in requests {
            match request {
                UiRequest::OpenSettings => self.open_settings(),
                UiRequest::SaveRule(rule) => {
                    // The watcher reloads the file afterwards; the engine already applied it.
                    match config_file::save_pinned_rule(&self.config_path, &rule) {
                        Ok(()) => info!(app = %rule.app_id, profile = %rule.profile,
                            layout = %rule.layout, ?rule.placement, "pinned"),
                        Err(e) => {
                            error!(error = %e, "cannot save the pinned rule");
                            os::beep();
                        }
                    }
                }
                UiRequest::Notice(notice) => {
                    warn!(?notice, "action not possible");
                    os::beep();
                }
            }
        }
    }

    /// Re-reads the config file. Invalid files are reported and the previous config kept.
    fn reload_config(&mut self) {
        match config_file::read(&self.config_path) {
            Ok(config) => {
                self.flags.config_error = false;
                self.lang = Lang::resolve(config.general.language, self.os_language.as_deref());
                self.flags.launch_at_login = config.general.launch_at_login;
                self.flags.auto_arrange = config.general.auto_arrange;
                let requests = self.driver.reload_config(config);
                self.handle_ui(requests);
                self.apply_launch_at_login();
                self.register_hotkeys();
                self.apply_api();
                self.publish_state();
            }
            Err(e) => {
                error!("configuration not applied, keeping the previous one:\n{e}");
                self.flags.config_error = true;
            }
        }
        self.refresh_menu();
    }

    /// A global shortcut was pressed.
    fn on_hotkey(&mut self, id: u32) {
        if let Some(action) = self.hotkey_actions.get(&id).cloned() {
            self.on_action(action);
        }
    }

    /// (Re-)registers the `[hotkeys]` table with the OS. Shortcuts already taken by another
    /// app are reported in the menu and the log.
    fn register_hotkeys(&mut self) {
        let Some(manager) = &self.hotkey_manager else {
            return;
        };
        if let Err(e) = manager.unregister_all(&self.hotkeys_registered) {
            warn!(error = %e, "cannot unregister previous shortcuts");
        }
        self.hotkeys_registered.clear();
        self.flags.hotkey_conflicts.clear();

        let bindings = Bindings::from_config(self.driver.engine().config());
        for (native, text, _) in &bindings.entries {
            match manager.register(*native) {
                Ok(()) => self.hotkeys_registered.push(*native),
                Err(e) => {
                    warn!(hotkey = %text, error = %e, "shortcut not available");
                    self.flags.hotkey_conflicts.push(text.clone());
                }
            }
        }
        self.hotkey_actions = bindings.actions;
        info!(
            registered = self.hotkeys_registered.len(),
            "shortcuts registered"
        );
    }

    fn apply_launch_at_login(&self) {
        let enabled = self.flags.launch_at_login;
        if let Err(e) = self.driver.platform().set_launch_at_login(enabled) {
            warn!(error = %e, enabled, "cannot change launch at login");
        }
    }

    /// Watches the config directory (editors often replace the file instead of writing it).
    fn watch_config(&mut self) {
        let Some(dir) = self.config_path.parent().map(PathBuf::from) else {
            return;
        };
        let file = self.config_path.clone();
        let token_file = token::path_for(&self.config_path);
        let debouncer = notify_debouncer_mini::new_debouncer(
            Duration::from_millis(300),
            move |result: notify_debouncer_mini::DebounceEventResult| {
                if let Ok(events) = result {
                    if events.iter().any(|e| e.path == file) {
                        post(|app| app.reload_config());
                    }
                    // Rotated from the settings window or the CLI: restart with the new one.
                    if events.iter().any(|e| e.path == token_file) {
                        post(|app| app.apply_api());
                    }
                }
            },
        );
        match debouncer {
            Ok(mut d) => {
                if let Err(e) = d.watcher().watch(&dir, notify::RecursiveMode::NonRecursive) {
                    warn!(error = %e, "cannot watch the config directory");
                }
                self._watcher = Some(d);
            }
            Err(e) => warn!(error = %e, "cannot create the config watcher"),
        }
    }

    /// Rebuilds the tray menu from the current state.
    fn refresh_menu(&self) {
        let hotkeys = &self.driver.engine().config().hotkeys;
        let entries = menu::build(&self.driver.status(), &self.flags, self.lang, hotkeys);
        let native = Menu::new();
        for entry in &entries {
            if let Some(item) = native_item(entry) {
                if let Err(e) = native.append(item.as_ref()) {
                    warn!(error = %e, "cannot build menu item");
                }
            }
        }
        self.tray.set_menu(Some(Box::new(native)));
        let status = self.driver.status();
        let tooltip = match (&status.profile, &status.layout) {
            (Some(p), Some(l)) => format!("Venecita Dock — {p} / {l}"),
            _ => "Venecita Dock".to_string(),
        };
        let _ = self.tray.set_tooltip(Some(tooltip));
    }
}

/// The native menu item for an entry.
fn native_item(entry: &MenuEntry) -> Option<Box<dyn IsMenuItem>> {
    Some(match entry {
        MenuEntry::Label(text) => Box::new(MenuItem::new(text, false, None)),
        MenuEntry::Item {
            text,
            command,
            enabled,
            icon,
            shortcut,
        } => {
            let accelerator = shortcut.as_deref().and_then(accelerator);
            match icon {
                Some(kind) => Box::new(IconMenuItem::with_id(
                    command.id(),
                    text,
                    *enabled,
                    Some(menu_icon(*kind)),
                    accelerator,
                )),
                None => Box::new(MenuItem::with_id(command.id(), text, *enabled, accelerator)),
            }
        }
        MenuEntry::Check {
            text,
            command,
            checked,
            enabled,
            shortcut,
        } => Box::new(CheckMenuItem::with_id(
            command.id(),
            text,
            *enabled,
            *checked,
            shortcut.as_deref().and_then(accelerator),
        )),
        MenuEntry::Submenu {
            text,
            icon,
            entries,
        } => {
            let submenu = Submenu::new(text, true);
            if let Some(kind) = icon {
                submenu.set_icon(Some(menu_icon(*kind)));
            }
            for e in entries {
                if let Some(item) = native_item(e) {
                    let _ = submenu.append(item.as_ref());
                }
            }
            Box::new(submenu)
        }
        MenuEntry::Separator => Box::new(PredefinedMenuItem::separator()),
    })
}

/// A colored menu icon (drawn at 2× so it stays sharp on Retina / high-DPI screens).
fn menu_icon(kind: IconKind) -> tray_icon::menu::Icon {
    const SIZE: u32 = 36;
    tray_icon::menu::Icon::from_rgba(icon_rgba(kind, SIZE), SIZE, SIZE).expect("valid icon")
}

/// The menu's rendering of a shortcut such as `ctrl+alt+r` (shown greyed on the right).
fn accelerator(shortcut: &str) -> Option<Accelerator> {
    use tray_icon::menu::accelerator::{Code, Modifiers};
    let hotkey: Hotkey = shortcut.parse().ok()?;
    // The menu crate uses another version of `keyboard-types` than the hotkey crate: go
    // through the key's name (`KeyR`, `ArrowLeft`), which both spell the same.
    let key: Code = venecita_app::hotkeys::key_code(&hotkey.key)?
        .to_string()
        .parse()
        .ok()?;
    let m = hotkey.mods;
    let mut mods = Modifiers::empty();
    for (on, flag) in [
        (m.ctrl, Modifiers::CONTROL),
        (m.alt, Modifiers::ALT),
        (m.shift, Modifiers::SHIFT),
        (m.meta, Modifiers::SUPER),
    ] {
        if on {
            mods |= flag;
        }
    }
    Some(Accelerator::new(Some(mods), key))
}

/// Logs to the log directory (daily files, see [`paths::log_dir`]) and to stderr.
/// `RUST_LOG` overrides the level (default: info for Venecita crates, warn otherwise).
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "warn,venecita_app=info,venecita_platform=info,venecita_api=info,venecita_dock=info",
        )
    });
    let stderr = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    let dir = paths::log_dir();
    let (file_layer, guard) = match std::fs::create_dir_all(&dir) {
        Ok(()) => {
            let appender = tracing_appender::rolling::daily(&dir, "venecita-dock.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (
                Some(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(writer),
                ),
                Some(guard),
            )
        }
        Err(_) => (None, None),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file_layer)
        .init();
    guard
}

/// The tray icon: a monitor outline split like a layout (left half in two, right half
/// whole), drawn in code in `color` on a transparent background. On macOS it is a template
/// image the system tints for light and dark menu bars.
fn tray_image(color: [u8; 3]) -> Icon {
    const W: u32 = 36;
    const H: u32 = 36;
    let mut rgba = vec![0u8; (W * H * 4) as usize];
    let mut set = |x: u32, y: u32| {
        let i = ((y * W + x) * 4) as usize;
        rgba[i..i + 3].copy_from_slice(&color);
        rgba[i + 3] = 255;
    };
    // Outer frame: x 3..33, y 7..29, 3 px thick.
    let (x0, x1, y0, y1, t) = (3, 33, 7, 29, 3);
    for y in y0..y1 {
        for x in x0..x1 {
            let border = x < x0 + t || x >= x1 - t || y < y0 + t || y >= y1 - t;
            let vertical = (17..19).contains(&x);
            let horizontal = x < 18 && (17..19).contains(&y);
            if border || vertical || horizontal {
                set(x, y);
            }
        }
    }
    Icon::from_rgba(rgba, W, H).expect("valid icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_become_menu_accelerators() {
        use tray_icon::menu::accelerator::{Code, Modifiers};
        let a = accelerator("ctrl+alt+r").expect("valid");
        assert_eq!(
            a,
            Accelerator::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyR)
        );
        let a = accelerator("ctrl+alt+shift+left").expect("valid");
        assert_eq!(
            a,
            Accelerator::new(
                Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
                Code::ArrowLeft
            )
        );
        assert!(accelerator("ctrl+alt+bracketright").is_some());
        assert!(accelerator("not a shortcut").is_none());
    }
}
