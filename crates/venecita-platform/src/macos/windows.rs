//! Running applications and their windows.

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

use super::ax::{attr, role, Element};
use crate::{AppIdentity, Pid, WindowId, WindowInfo, WindowKind};

/// Packs a process id and a CoreGraphics window number into a [`WindowId`].
///
/// Window numbers are unique system-wide, but keeping the pid lets us find the AX
/// element again without any lookup table: we ask that app for its windows and pick
/// the one with the matching number.
pub fn window_id(pid: Pid, number: u32) -> WindowId {
    WindowId(((pid as u32 as u64) << 32) | u64::from(number))
}

/// Inverse of [`window_id`].
pub fn split_window_id(id: WindowId) -> (Pid, u32) {
    ((id.0 >> 32) as u32 as Pid, id.0 as u32)
}

/// Identity of a running application: bundle id (falling back to the executable name)
/// and localized name.
pub fn identity(app: &NSRunningApplication) -> AppIdentity {
    let exe = app
        .executableURL()
        .and_then(|u| u.lastPathComponent())
        .map(|s| s.to_string());
    let id = app
        .bundleIdentifier()
        .map(|s| s.to_string())
        .or_else(|| exe.clone())
        .unwrap_or_default();
    let name = app
        .localizedName()
        .map(|s| s.to_string())
        .or(exe)
        .unwrap_or_else(|| id.clone());
    AppIdentity { id, name }
}

/// Returns `true` for apps that show up in the Dock (and so have normal windows).
pub fn is_regular(app: &NSRunningApplication) -> bool {
    !app.isTerminated() && app.activationPolicy() == NSApplicationActivationPolicy::Regular
}

/// All running regular applications.
pub fn regular_apps() -> Vec<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|a| is_regular(a))
        .collect()
}

/// Looks up a running application by pid.
pub fn app_by_pid(pid: Pid) -> Option<Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
}

/// Classifies a window element, or returns `None` if it isn't a window at all.
pub fn classify(el: &Element) -> Option<WindowKind> {
    if el.string(attr::ROLE).as_deref() != Some(role::WINDOW) {
        return None;
    }
    if el.bool(attr::MINIMIZED) == Some(true) {
        return Some(WindowKind::Minimized);
    }
    if el.bool(attr::FULLSCREEN) == Some(true) {
        return Some(WindowKind::Fullscreen);
    }
    Some(match el.string(attr::SUBROLE).as_deref() {
        Some(role::STANDARD_WINDOW) => WindowKind::Normal,
        Some(role::DIALOG) | Some(role::SYSTEM_DIALOG) => WindowKind::Dialog,
        _ => WindowKind::Floating,
    })
}

/// Builds a [`WindowInfo`] for a window element, or `None` if it isn't a usable window.
pub fn window_info(pid: Pid, app: &AppIdentity, el: &Element) -> Option<WindowInfo> {
    let number = el.window_number()?;
    let kind = classify(el)?;
    let frame = el.frame()?;
    Some(WindowInfo {
        id: window_id(pid, number),
        pid,
        app: app.clone(),
        title: el.string(attr::TITLE).unwrap_or_default(),
        frame,
        kind,
    })
}

/// All windows of one application.
pub fn app_windows(pid: Pid, app: &AppIdentity) -> Vec<WindowInfo> {
    Element::application(pid)
        .elements(attr::WINDOWS)
        .iter()
        .filter_map(|el| window_info(pid, app, el))
        .collect()
}

/// Finds the AX element of a window.
pub fn find_window(id: WindowId) -> Option<(Element, Element)> {
    let (pid, number) = split_window_id(id);
    let app = Element::application(pid);
    let win = app
        .elements(attr::WINDOWS)
        .into_iter()
        .find(|w| w.window_number() == Some(number))?;
    Some((app, win))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_id_round_trip() {
        for (pid, n) in [(1, 1), (99999, 123456), (i32::MAX, u32::MAX)] {
            assert_eq!(split_window_id(window_id(pid, n)), (pid, n));
        }
    }
}
