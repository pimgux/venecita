//! Launch at login through a per-user LaunchAgent.
//!
//! `SMAppService` (the modern API) only works for signed `.app` bundles, which arrive with
//! packaging in phase 10. A LaunchAgent plist in `~/Library/LaunchAgents` works for any
//! executable, including development builds, and is easy to inspect or remove by hand.

use std::path::{Path, PathBuf};

use crate::{PlatformError, Result};

/// Label of the LaunchAgent (also its file name).
pub const AGENT_LABEL: &str = "io.github.venecita-dock";

/// `~/Library/LaunchAgents/io.github.venecita-dock.plist`.
pub fn agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        Path::new(&home)
            .join("Library/LaunchAgents")
            .join(format!("{AGENT_LABEL}.plist"))
    })
}

/// Escapes text for inclusion in an XML plist.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The plist that starts `executable` at login.
pub fn agent_plist(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(&executable.to_string_lossy())
    )
}

fn io_error(operation: &'static str, e: std::io::Error) -> PlatformError {
    PlatformError::Os {
        operation,
        code: i64::from(e.raw_os_error().unwrap_or(-1)),
    }
}

/// Installs or removes the LaunchAgent for the running executable.
pub fn set_enabled(enabled: bool) -> Result<()> {
    let path = agent_path().ok_or(PlatformError::Unsupported("HOME is not set"))?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| io_error("current_exe", e))?;
        let plist = agent_plist(&exe);
        // Don't rewrite an identical file (avoids touching it on every start).
        if std::fs::read_to_string(&path).ok().as_deref() == Some(plist.as_str()) {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io_error("create LaunchAgents", e))?;
        }
        std::fs::write(&path, plist).map_err(|e| io_error("write LaunchAgent", e))
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error("remove LaunchAgent", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_contains_escaped_path() {
        let plist = agent_plist(Path::new("/Apps/A & B/venecita-dock"));
        assert!(plist.contains("<string>/Apps/A &amp; B/venecita-dock</string>"));
        assert!(plist.contains(AGENT_LABEL));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }
}
