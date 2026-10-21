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
    let exec = exec_value(&exe.to_string_lossy()).ok_or(PlatformError::Unsupported(
        "launch at login (the executable's path has a line break)",
    ))?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Venecita Dock\n\
         Comment=Remembers where every application belongs\nExec={exec}\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    std::fs::write(&path, entry).map_err(io_error("write the autostart entry"))
}

/// The `Exec` value that runs `path`, escaped as the Desktop Entry spec asks: quoted, with
/// `"`, `` ` ``, `$` and `\` escaped inside the quotes, `%` doubled, and then every `\`
/// doubled again as for any string value. `None` if the path has a line break.
fn exec_value(path: &str) -> Option<String> {
    if path.contains(['\n', '\r']) {
        return None;
    }
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    Some(quoted.replace('\\', "\\\\"))
}

#[cfg(test)]
mod tests {
    use super::exec_value;

    #[test]
    fn exec_paths_are_quoted_and_escaped() {
        assert_eq!(
            exec_value("/opt/venecita dock/venecita-dock").as_deref(),
            Some("\"/opt/venecita dock/venecita-dock\"")
        );
        assert_eq!(
            exec_value(r#"/home/a"b/$x/100%/c\d"#).as_deref(),
            Some(r#""/home/a\\"b/\\$x/100%%/c\\\\d""#)
        );
        assert_eq!(exec_value("/tmp/a\nb"), None);
    }
}
