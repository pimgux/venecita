//! Actions and keyboard shortcuts.
//!
//! [`Action`] is the single list of things Venecita Dock can do. Hotkeys, the tray menu,
//! the local API and the CLI all produce `Action`s, so every feature is implemented once.
//!
//! In JSON (API) an action looks like `{"action": "move_zone", "dir": "left"}`.
//! In the TOML `[hotkeys]` table the short form `"focus_mode_toggle"` is also accepted for
//! actions without parameters (see [`ActionSpec`]).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geometry::Direction;
use crate::window::WindowId;

/// Which display to send a window to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    /// The next display, ordered left to right (wraps around).
    Next,
    /// The previous display, ordered left to right (wraps around).
    Prev,
}

/// Everything that can be triggered by a hotkey, the tray menu, the API or the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Move the focused window one cell in a direction.
    MoveZone {
        /// Direction to move in.
        dir: Direction,
    },
    /// Push the focused window's edge on the given side out by one cell.
    Grow {
        /// Side whose edge moves.
        dir: Direction,
    },
    /// Pull the focused window's edge on the given side in by one cell.
    Shrink {
        /// Side whose edge moves.
        dir: Direction,
    },
    /// Send the focused window to another display, keeping its zone.
    MoveToDisplay {
        /// Next or previous display.
        target: DisplayTarget,
    },
    /// Snap the focused window to the nearest zone (without remembering it).
    SnapToGrid,
    /// Snap the focused window and remember that zone for its application.
    PinHere,
    /// Show the next window of the focused window's stack.
    StackNext,
    /// Show the previous window of the focused window's stack.
    StackPrev,
    /// Show the next window of the dynamic zone.
    DynamicNext,
    /// Show the previous window of the dynamic zone.
    DynamicPrev,
    /// Focus a specific window (e.g. a click on a tab), activating it in its stack.
    FocusWindow {
        /// The window.
        id: WindowId,
    },
    /// Enter or leave extra focus mode.
    FocusModeToggle,
    /// Put every window back in its configured place.
    RelayoutAll,
    /// Switch to a profile by name (overrides automatic detection).
    ActivateProfile {
        /// Profile name as written in the config.
        name: String,
    },
    /// Cycle to the next profile that fits the connected displays.
    NextProfile,
    /// Return to automatic profile detection.
    AutoProfile,
    /// Pause or resume automatic placement.
    TogglePause,
    /// Open the settings window.
    OpenSettings,
}

/// An [`Action`] as written in the `[hotkeys]` table: either a plain string for actions
/// without parameters (`"pin_here"`) or a table (`{ action = "move_zone", dir = "left" }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec(pub Action);

impl<'de> Deserialize<'de> for ActionSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = toml::Value::deserialize(deserializer)?;
        let table = match value {
            toml::Value::String(name) => {
                let mut t = toml::Table::new();
                t.insert("action".into(), toml::Value::String(name));
                toml::Value::Table(t)
            }
            other @ toml::Value::Table(_) => other,
            other => {
                return Err(D::Error::custom(format!(
                    "expected an action name or table, found {}",
                    other.type_str()
                )))
            }
        };
        table.try_into().map(ActionSpec).map_err(D::Error::custom)
    }
}

impl Serialize for ActionSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let value = toml::Value::try_from(&self.0).map_err(S::Error::custom)?;
        // Use the short string form when the action has no parameters.
        if let toml::Value::Table(t) = &value {
            if t.len() == 1 {
                if let Some(name) = t.get("action") {
                    return name.serialize(serializer);
                }
            }
        }
        value.serialize(serializer)
    }
}

/// Error returned when a hotkey string can't be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid hotkey `{input}`: {reason}")]
pub struct HotkeyError {
    /// The text that was parsed.
    pub input: String,
    /// Why it was rejected.
    pub reason: String,
}

/// Modifier keys of a [`Hotkey`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers {
    /// Control.
    pub ctrl: bool,
    /// Alt / Option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Command (macOS), Windows key, Super (Linux).
    pub meta: bool,
}

impl Modifiers {
    fn any(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.meta
    }
}

/// A global keyboard shortcut such as `ctrl+alt+left`.
///
/// Parsing is case-insensitive and accepts common aliases (`option` = `alt`,
/// `cmd`/`super`/`win` = `meta`, `return` = `enter`, `]` = `bracketright`…). Two hotkeys
/// are equal if they have the same modifiers and key, regardless of how they were written.
/// [`Hotkey`]'s `Display` produces the canonical form, e.g. `ctrl+alt+shift+cmd+k`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hotkey {
    /// Modifier keys.
    pub mods: Modifiers,
    /// Canonical key name (see [`KEY_NAMES`]).
    pub key: String,
}

/// Canonical names of the keys a hotkey can use (besides `a`–`z`, `0`–`9` and `f1`–`f24`).
pub const KEY_NAMES: &[&str] = &[
    "left",
    "right",
    "up",
    "down",
    "enter",
    "tab",
    "space",
    "escape",
    "backspace",
    "delete",
    "home",
    "end",
    "pageup",
    "pagedown",
    "minus",
    "equal",
    "comma",
    "period",
    "slash",
    "backslash",
    "semicolon",
    "quote",
    "backquote",
    "bracketleft",
    "bracketright",
];

/// Maps aliases to canonical key names.
fn canonical_key(key: &str) -> Option<String> {
    let alias = match key {
        "return" => "enter",
        "esc" => "escape",
        "del" => "delete",
        "pgup" => "pageup",
        "pgdn" => "pagedown",
        "-" => "minus",
        "=" => "equal",
        "," => "comma",
        "." => "period",
        "/" => "slash",
        "\\" => "backslash",
        ";" => "semicolon",
        "'" => "quote",
        "`" => "backquote",
        "[" => "bracketleft",
        "]" => "bracketright",
        other => other,
    };
    let is_alnum = alias.len() == 1 && alias.chars().all(|c| c.is_ascii_alphanumeric());
    let is_fkey = alias
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    (is_alnum || is_fkey || KEY_NAMES.contains(&alias)).then(|| alias.to_string())
}

impl FromStr for Hotkey {
    type Err = HotkeyError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let err = |reason: &str| HotkeyError {
            input: input.to_string(),
            reason: reason.to_string(),
        };
        let lower = input.trim().to_lowercase();
        if lower.is_empty() {
            return Err(err("empty"));
        }
        let parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let (key_part, mod_parts) = parts.split_last().ok_or_else(|| err("empty"))?;

        let mut mods = Modifiers::default();
        for m in mod_parts {
            let flag = match *m {
                "ctrl" | "control" => &mut mods.ctrl,
                "alt" | "option" | "opt" => &mut mods.alt,
                "shift" => &mut mods.shift,
                "cmd" | "command" | "super" | "win" | "meta" => &mut mods.meta,
                "" => return Err(err("empty part (use `equal` for the + key)")),
                other => return Err(err(&format!("unknown modifier `{other}`"))),
            };
            if *flag {
                return Err(err(&format!("modifier `{m}` repeated")));
            }
            *flag = true;
        }

        let key =
            canonical_key(key_part).ok_or_else(|| err(&format!("unknown key `{key_part}`")))?;
        // A global shortcut without modifiers would steal normal typing. Function keys
        // are the only exception.
        let is_function_key =
            key.len() > 1 && key.starts_with('f') && key[1..].chars().all(|c| c.is_ascii_digit());
        if !mods.any() && !is_function_key {
            return Err(err("needs at least one modifier (ctrl, alt, shift, cmd)"));
        }
        Ok(Hotkey { mods, key })
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = &self.mods;
        for (on, name) in [
            (m.ctrl, "ctrl"),
            (m.alt, "alt"),
            (m.shift, "shift"),
            (m.meta, "cmd"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_json_shape() {
        let a = Action::MoveZone {
            dir: Direction::Left,
        };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"action":"move_zone","dir":"left"}"#
        );
        let parsed: Action = serde_json::from_str(r#"{"action":"focus_mode_toggle"}"#).unwrap();
        assert_eq!(parsed, Action::FocusModeToggle);
        let unknown = serde_json::from_str::<Action>(r#"{"action":"explode"}"#);
        assert!(unknown.is_err());
    }

    #[derive(Deserialize, Serialize)]
    struct Holder {
        a: ActionSpec,
    }

    #[test]
    fn action_spec_short_and_table_forms() {
        let h: Holder = toml::from_str(r#"a = "pin_here""#).unwrap();
        assert_eq!(h.a.0, Action::PinHere);
        let h: Holder = toml::from_str(r#"a = { action = "grow", dir = "up" }"#).unwrap();
        assert_eq!(h.a.0, Action::Grow { dir: Direction::Up });
        // A parameterised action can't use the short form.
        assert!(toml::from_str::<Holder>(r#"a = "move_zone""#).is_err());
        assert!(toml::from_str::<Holder>("a = 3").is_err());
    }

    #[test]
    fn action_spec_serializes_short_form_when_possible() {
        let h = Holder {
            a: ActionSpec(Action::RelayoutAll),
        };
        assert_eq!(toml::to_string(&h).unwrap().trim(), r#"a = "relayout_all""#);
        let h = Holder {
            a: ActionSpec(Action::ActivateProfile {
                name: "Casa".into(),
            }),
        };
        let back: Holder = toml::from_str(&toml::to_string(&h).unwrap()).unwrap();
        assert_eq!(back.a, h.a);
    }

    #[test]
    fn hotkey_canonical_form() {
        let a: Hotkey = "Alt+Ctrl+F".parse().unwrap();
        let b: Hotkey = "control+option+f".parse().unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), "ctrl+alt+f");
        let c: Hotkey = "cmd+shift+ctrl+alt+]".parse().unwrap();
        assert_eq!(c.to_string(), "ctrl+alt+shift+cmd+bracketright");
        let d: Hotkey = "super+return".parse().unwrap();
        assert_eq!(d.to_string(), "cmd+enter");
    }

    #[test]
    fn hotkey_function_key_without_modifier_is_allowed() {
        assert_eq!("F13".parse::<Hotkey>().unwrap().to_string(), "f13");
    }

    #[test]
    fn hotkey_errors() {
        for bad in [
            "",
            "ctrl+",
            "ctrl+alt",
            "hyper+a",
            "ctrl+ctrl+a",
            "a",
            "left",
            "f",
            "f25",
            "ctrl+foo",
        ] {
            assert!(bad.parse::<Hotkey>().is_err(), "`{bad}` should be rejected");
        }
    }
}
