//! Venecita Dock: the menu-bar application.
//!
//! This file only wires things together; the logic lives in the library
//! (`venecita_app::driver`, `menu`, `config_file`…) where it is unit tested.

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
fn main() {
    if std::env::args().any(|a| a == mac::SETTINGS_FLAG) {
        mac::run_settings();
    } else {
        mac::run();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Venecita Dock currently runs on macOS only (Windows: phase 9, Linux: phase 10).");
    std::process::exit(1);
}
