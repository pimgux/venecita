//! Where Venecita Dock keeps its files.
//!
//! | OS | Config | Logs |
//! |---|---|---|
//! | macOS | `~/Library/Application Support/venecita-dock/` | `~/Library/Logs/venecita-dock/` |
//! | Windows | `%APPDATA%\venecita-dock\` | `%LOCALAPPDATA%\venecita-dock\logs\` |
//! | Linux | `$XDG_CONFIG_HOME/venecita-dock/` | `$XDG_STATE_HOME/venecita-dock/` |
//!
//! `VENECITA_CONFIG_DIR` overrides the config directory (handy for testing).

use std::path::PathBuf;

const APP_DIR: &str = "venecita-dock";

/// Directory holding `config.toml` and `token.secret`.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VENECITA_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

/// The configuration file.
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// Directory for log files.
pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            return home.join("Library/Logs").join(APP_DIR);
        }
    }
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
        .join("logs")
}
