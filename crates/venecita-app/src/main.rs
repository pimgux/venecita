//! Venecita Dock: the tray application.
//!
//! This file only wires things together; the logic lives in the library
//! (`venecita_app::driver`, `menu`, `config_file`…) where it is unit tested.

// A tray app on Windows: no console window (debug builds keep it for the logs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
#[path = "mac.rs"]
mod os;
#[cfg(windows)]
#[path = "win.rs"]
mod os;
#[cfg(any(target_os = "macos", windows))]
mod shell;

#[cfg(any(target_os = "macos", windows))]
fn main() {
    if std::env::args().any(|a| a == shell::SETTINGS_FLAG) {
        shell::run_settings();
    } else {
        shell::run();
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {
    eprintln!("Venecita Dock runs on macOS and Windows for now (Linux: phase 10).");
    std::process::exit(1);
}
