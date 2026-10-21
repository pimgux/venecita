//! Launch at login: an XDG autostart entry, `~/.config/autostart/venecita-dock.desktop`.

use crate::{PlatformError, Result};

const FILE: &str = "venecita-dock.desktop";

fn io_error(operation: &'static str) -> impl Fn(std::io::Error) -> PlatformError {
    move |e| PlatformError::Os {
        operation,
        code: i64::from(e.raw_os_error().unwrap_or(0)),
    }
}

/// Writes or removes the autostart entry for the current executable.
pub fn set(enabled: bool) -> Result<()> {
    let dir = dirs::config_dir()
        .ok_or(PlatformError::Unsupported(
            "launch at login (no config directory)",
        ))?
        .join("autostart");
    let path = dir.join(FILE);
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(io_error("remove the autostart entry")(e))
            }
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(io_error("find the executable"))?;
    std::fs::create_dir_all(&dir).map_err(io_error("create the autostart folder"))?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Venecita Dock\n\
         Comment=Remembers where every application belongs\nExec=\"{}\"\n\
         X-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(&path, entry).map_err(io_error("write the autostart entry"))
}
