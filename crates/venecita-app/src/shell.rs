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
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tracing::{error, info, warn};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use venecita_app::config_file;
use venecita_app::driver::{Driver, UiRequest};
use venecita_app::hotkeys::Bindings;
use venecita_app::i18n::Lang;
use venecita_app::menu::{self, AppFlags, MenuCommand, MenuEntry};
use venecita_app::paths;
use venecita_core::Action;
use venecita_platform::{PermissionStatus, Platform, PlatformEvent};

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
}

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
        let affects_menu = matches!(
            event,
            PlatformEvent::DisplaysChanged | PlatformEvent::SystemWoke
        );
        let requests = self.driver.on_event(event);
        self.handle_ui(requests);
        if affects_menu {
            self.refresh_menu();
        }
    }

    fn on_action(&mut self, action: Action) {
        let requests = self.driver.on_action(action);
        self.handle_ui(requests);
        self.refresh_menu();
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
                let requests = self.driver.reload_config(config);
                self.handle_ui(requests);
                self.apply_launch_at_login();
                self.register_hotkeys();
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
        let debouncer = notify_debouncer_mini::new_debouncer(
            Duration::from_millis(300),
            move |result: notify_debouncer_mini::DebounceEventResult| {
                if let Ok(events) = result {
                    if events.iter().any(|e| e.path == file) {
                        post(|app| app.reload_config());
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
        let entries = menu::build(&self.driver.status(), &self.flags, self.lang);
        let native = Menu::new();
        for entry in entries {
            let result = match entry {
                MenuEntry::Label(text) => native.append(&MenuItem::new(text, false, None)),
                MenuEntry::Item {
                    text,
                    command,
                    enabled,
                } => native.append(&MenuItem::with_id(command.id(), text, enabled, None)),
                MenuEntry::Check {
                    text,
                    command,
                    checked,
                    enabled,
                } => native.append(&CheckMenuItem::with_id(
                    command.id(),
                    text,
                    enabled,
                    checked,
                    None,
                )),
                MenuEntry::Separator => native.append(&PredefinedMenuItem::separator()),
            };
            if let Err(e) = result {
                warn!(error = %e, "cannot build menu item");
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

/// Logs to the log directory (daily files, see [`paths::log_dir`]) and to stderr.
/// `RUST_LOG` overrides the level (default: info for Venecita crates, warn otherwise).
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,venecita_app=info,venecita_platform=info,venecita_dock=info")
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
