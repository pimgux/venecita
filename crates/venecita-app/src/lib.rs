//! Venecita Dock application library.
//!
//! The binary (`main.rs`) is a thin shell: OS event loop, tray icon, config file watching.
//! Everything that can be tested without a GUI lives here.

pub mod config_file;
pub mod driver;
pub mod hotkeys;
pub mod i18n;
pub mod icons;
pub mod intl;
pub mod menu;
pub mod paths;
pub mod settings;
pub mod system;
pub mod token;
