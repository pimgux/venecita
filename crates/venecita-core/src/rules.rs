//! Application identity and matching.
//!
//! Rules and the ignore list refer to applications by strings. A string matches an
//! application if it is equal (ignoring case) to either:
//!
//! * the **app id**: the bundle identifier on macOS (`com.google.Chrome`), the executable
//!   file name on Windows (`chrome.exe`) or `WM_CLASS` on Linux (`google-chrome`), or
//! * the **app name**: the display/process name (`Google Chrome`).
//!
//! The `.exe` suffix is optional on both sides so `"chrome"` matches `chrome.exe`.
//! A rule may list several strings so the same config works on every OS.

use serde::{Deserialize, Serialize};

/// Identifies the application that owns a window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppIdentity {
    /// Bundle id (macOS), executable name (Windows) or WM_CLASS (Linux).
    pub id: String,
    /// Human readable application or process name.
    pub name: String,
}

impl AppIdentity {
    /// Convenience constructor.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

/// Lower-cases a string and removes a trailing `.exe`.
fn normalize(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    match lower.strip_suffix(".exe") {
        Some(stripped) => stripped.to_string(),
        None => lower,
    }
}

/// Returns `true` if `pattern` refers to `app` (see the module docs for the rules).
pub fn app_matches(pattern: &str, app: &AppIdentity) -> bool {
    let p = normalize(pattern);
    !p.is_empty() && (p == normalize(&app.id) || p == normalize(&app.name))
}

/// Returns `true` if any of `patterns` refers to `app`.
pub fn any_app_matches<S: AsRef<str>>(patterns: &[S], app: &AppIdentity) -> bool {
    patterns.iter().any(|p| app_matches(p.as_ref(), app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_bundle_id_or_name_ignoring_case() {
        let chrome = AppIdentity::new("com.google.Chrome", "Google Chrome");
        assert!(app_matches("com.google.chrome", &chrome));
        assert!(app_matches("Google Chrome", &chrome));
        assert!(!app_matches("Chrome", &chrome), "no substring matching");
        assert!(!app_matches("", &chrome));
    }

    #[test]
    fn exe_suffix_is_optional() {
        let chrome = AppIdentity::new("chrome.exe", "Google Chrome");
        assert!(app_matches("chrome", &chrome));
        assert!(app_matches("CHROME.EXE", &chrome));
        let code = AppIdentity::new("code", "Code");
        assert!(app_matches("Code.exe", &code));
    }

    #[test]
    fn any_of_several_patterns() {
        let firefox = AppIdentity::new("firefox", "Firefox");
        assert!(any_app_matches(
            &["org.mozilla.firefox", "firefox.exe"],
            &firefox
        ));
        assert!(!any_app_matches::<&str>(&[], &firefox));
    }
}
