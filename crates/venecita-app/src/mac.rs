//! The macOS side of the tray app (see `shell.rs`): the `NSApplication` event loop, the
//! main dispatch queue, and a few system services.

use std::path::Path;

use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBeep, NSEvent,
    NSRunningApplication,
};
use objc2_foundation::NSLocale;
use tracing::warn;
use venecita_platform::macos::MacPlatform;

/// The platform backend.
pub type Native = MacPlatform;

/// Proof that [`init`] ran on the main thread.
pub struct Ui {
    mtm: MainThreadMarker,
}

/// Prepares the process as a menu-bar app (no Dock icon, no main menu).
pub fn init() -> Ui {
    let mtm = MainThreadMarker::new().expect("Venecita Dock must start on the main thread");
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    Ui { mtm }
}

/// Creates the platform backend.
pub fn new_platform(ui: &Ui) -> Native {
    MacPlatform::new(ui.mtm)
}

/// Runs the event loop until [`quit`].
pub fn run_loop(ui: Ui) {
    NSApplication::sharedApplication(ui.mtm).run();
}

/// Schedules `f` on the main thread.
pub fn run_on_main(f: Box<dyn FnOnce() + Send>) {
    DispatchQueue::main().exec_async(f);
}

/// Ends the event loop.
pub fn quit() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).terminate(None);
    }
}

/// The system alert sound.
pub fn beep() {
    NSBeep();
}

/// The user's preferred language, e.g. `"es-AR"`.
pub fn os_language() -> Option<String> {
    NSLocale::preferredLanguages()
        .firstObject()
        .map(|s| s.to_string())
}

/// Opens a folder in Finder.
pub fn open_folder(path: &Path) {
    if let Err(e) = std::process::Command::new("open").arg(path).spawn() {
        warn!(error = %e, path = %path.display(), "cannot open folder");
    }
}

/// Opens the Accessibility pane of System Settings.
pub fn open_permission_settings() {
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
    if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
        warn!(error = %e, "cannot open System Settings");
    }
}

/// Brings another process's windows to the front.
pub fn bring_to_front(pid: u32) {
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }
}

/// Whether the primary mouse button is held (e.g. the user is still dragging a window).
pub fn mouse_button_down() -> bool {
    NSEvent::pressedMouseButtons() & 1 != 0
}

/// Black: the icon is a template image that macOS tints for the menu bar.
pub fn tray_icon_color() -> [u8; 3] {
    [0, 0, 0]
}
