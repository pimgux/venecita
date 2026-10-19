//! Operating system services used by the tray app and the settings window: opening a
//! text file, the connected displays and the applications that have windows open.

use std::path::Path;

#[cfg(any(target_os = "macos", windows))]
use venecita_core::display::DisplayInfo;

/// Opens a text file in the user's text editor.
pub fn open_text_file(path: &Path) {
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open")
        .arg("-t")
        .arg(path)
        .spawn();
    #[cfg(windows)]
    let spawned = std::process::Command::new("notepad.exe").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let spawned = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = spawned {
        tracing::warn!(error = %e, path = %path.display(), "cannot open the file");
    }
}

/// The displays connected right now. On macOS this must run on the main thread.
#[cfg(target_os = "macos")]
pub fn connected_displays() -> Result<Vec<DisplayInfo>, String> {
    let mtm = objc2::MainThreadMarker::new().ok_or("not on the main thread")?;
    venecita_platform::macos::displays::list(mtm).map_err(|e| e.to_string())
}

/// The displays connected right now.
#[cfg(windows)]
pub fn connected_displays() -> Result<Vec<DisplayInfo>, String> {
    use venecita_platform::Platform;
    venecita_platform::windows::WinPlatform::new()
        .displays()
        .map_err(|e| e.to_string())
}

/// Regular (Dock) applications running now, as (bundle id, name), sorted by name.
#[cfg(target_os = "macos")]
pub fn running_apps() -> Vec<(String, String)> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
    let mut apps: Vec<(String, String)> = NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| {
            let bundle = a.bundleIdentifier()?.to_string();
            let name = a.localizedName().map(|n| n.to_string()).unwrap_or_default();
            Some((bundle, name))
        })
        .collect();
    sort_apps(&mut apps);
    apps
}

/// Applications with a window open now, as (exe name, product name), sorted by name.
#[cfg(windows)]
pub fn running_apps() -> Vec<(String, String)> {
    use venecita_platform::Platform;
    let mut apps: Vec<(String, String)> = venecita_platform::windows::WinPlatform::new()
        .windows()
        .unwrap_or_default()
        .into_iter()
        .map(|w| (w.app.id, w.app.name))
        .collect();
    sort_apps(&mut apps);
    apps
}

#[cfg(any(target_os = "macos", windows))]
fn sort_apps(apps: &mut Vec<(String, String)>) {
    apps.sort_by_key(|(id, name)| (name.to_lowercase(), id.to_lowercase()));
    apps.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
}
