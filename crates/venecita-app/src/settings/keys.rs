//! Helpers for the shortcuts screen that don't need a GUI: turning a captured key press
//! into a hotkey string, spotting conflicts and naming actions.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use venecita_core::action::ActionSpec;
use venecita_core::Hotkey;

/// Modifier keys held during a key press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldModifiers {
    /// Control.
    pub ctrl: bool,
    /// Alt / Option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Cmd / Windows / Super.
    pub cmd: bool,
}

/// Turns a key press into a canonical hotkey string such as `ctrl+alt+left`.
///
/// `key_name` is the GUI toolkit's name for the key (egui's `Key::name()`: `"Left"`,
/// `"A"`, `"OpenBracket"`…). Returns `None` for keys a hotkey can't use, or presses that
/// wouldn't make a valid global shortcut (e.g. a letter without modifiers).
pub fn capture(mods: HeldModifiers, key_name: &str) -> Option<String> {
    let lower = key_name.to_lowercase();
    let key = match lower.as_str() {
        "equals" => "equal",
        "openbracket" => "bracketleft",
        "closebracket" => "bracketright",
        "backtick" => "backquote",
        other => other,
    };
    let mut text = String::new();
    for (on, name) in [
        (mods.ctrl, "ctrl"),
        (mods.alt, "alt"),
        (mods.shift, "shift"),
        (mods.cmd, "cmd"),
    ] {
        if on {
            text.push_str(name);
            text.push('+');
        }
    }
    text.push_str(key);
    text.parse::<Hotkey>().ok().map(|h| h.to_string())
}

/// The hotkey strings that collide with another entry: two strings that parse to the same
/// shortcut (`alt+x` and `option+x`). Unparseable strings are left to validation.
pub fn conflicts(hotkeys: &BTreeMap<String, ActionSpec>) -> BTreeSet<String> {
    let mut by_hotkey: HashMap<Hotkey, Vec<&String>> = HashMap::new();
    for key in hotkeys.keys() {
        if let Ok(h) = key.parse::<Hotkey>() {
            by_hotkey.entry(h).or_default().push(key);
        }
    }
    by_hotkey
        .into_values()
        .filter(|keys| keys.len() > 1)
        .flatten()
        .cloned()
        .collect()
}

/// A short readable name for an action: `snap_to_grid`, `move_zone (left)`,
/// `activate_profile (Casa)`.
pub fn action_label(spec: &ActionSpec) -> String {
    match toml::Value::try_from(spec) {
        Ok(toml::Value::String(name)) => name,
        Ok(toml::Value::Table(table)) => {
            let name = table
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let params: Vec<String> = table
                .iter()
                .filter(|(k, _)| k.as_str() != "action")
                .map(|(_, v)| match v {
                    toml::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect();
            if params.is_empty() {
                name
            } else {
                format!("{name} ({})", params.join(", "))
            }
        }
        _ => format!("{:?}", spec.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use venecita_core::config::default_hotkeys;

    const CTRL_ALT: HeldModifiers = HeldModifiers {
        ctrl: true,
        alt: true,
        shift: false,
        cmd: false,
    };

    #[test]
    fn captures_arrows_letters_and_symbols() {
        assert_eq!(capture(CTRL_ALT, "Left").as_deref(), Some("ctrl+alt+left"));
        assert_eq!(capture(CTRL_ALT, "K").as_deref(), Some("ctrl+alt+k"));
        assert_eq!(capture(CTRL_ALT, "7").as_deref(), Some("ctrl+alt+7"));
        assert_eq!(
            capture(CTRL_ALT, "CloseBracket").as_deref(),
            Some("ctrl+alt+bracketright")
        );
        assert_eq!(
            capture(CTRL_ALT, "Equals").as_deref(),
            Some("ctrl+alt+equal")
        );
        assert_eq!(
            capture(CTRL_ALT, "Backtick").as_deref(),
            Some("ctrl+alt+backquote")
        );
        let all = HeldModifiers {
            ctrl: true,
            alt: true,
            shift: true,
            cmd: true,
        };
        assert_eq!(
            capture(all, "Period").as_deref(),
            Some("ctrl+alt+shift+cmd+period")
        );
    }

    #[test]
    fn rejects_unusable_presses() {
        // No modifier: would steal normal typing.
        assert_eq!(capture(HeldModifiers::default(), "A"), None);
        // Function keys are allowed alone.
        assert_eq!(
            capture(HeldModifiers::default(), "F5").as_deref(),
            Some("f5")
        );
        // Keys Venecita can't register.
        assert_eq!(capture(CTRL_ALT, "Copy"), None);
        assert_eq!(capture(CTRL_ALT, "Questionmark"), None);
    }

    #[test]
    fn every_captured_default_round_trips() {
        // Each default shortcut, typed in canonical form, is recognised as itself.
        for key in default_hotkeys().keys() {
            let h: Hotkey = key.parse().unwrap();
            assert_eq!(&h.to_string(), key);
        }
    }

    #[test]
    fn finds_conflicting_spellings() {
        let mut map = default_hotkeys();
        assert!(conflicts(&map).is_empty());
        let spec = map["ctrl+alt+f"].clone();
        map.insert("option+ctrl+f".into(), spec);
        let found = conflicts(&map);
        assert_eq!(
            found,
            BTreeSet::from(["ctrl+alt+f".to_string(), "option+ctrl+f".to_string()])
        );
    }

    #[test]
    fn labels_actions() {
        let map = default_hotkeys();
        assert_eq!(action_label(&map["ctrl+alt+s"]), "snap_to_grid");
        assert_eq!(action_label(&map["ctrl+alt+left"]), "move_zone (left)");
        assert_eq!(action_label(&map["ctrl+alt+n"]), "move_to_display (next)");
    }
}
