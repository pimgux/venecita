//! Helpers for the shortcuts screen that don't need a GUI: turning a captured key press
//! into a hotkey string, spotting conflicts and naming actions.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fluent::FluentArgs;
use venecita_core::action::ActionSpec;
use venecita_core::{Action, Hotkey};

use crate::intl::I18n;

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

/// The name and a one-line description of an action, in the interface language:
/// ("Move left", "Moves the focused window one cell left, keeping its size.").
pub fn action_text(i18n: &I18n, action: &Action) -> (String, String) {
    let id = action_id(action);
    let mut args = FluentArgs::new();
    let word = |prefix: &str, value: String| i18n.get(&format!("{prefix}-{value}"));
    match action {
        Action::MoveZone { dir } => {
            args.set("dir", word("dir", snake(dir)));
        }
        Action::Grow { dir } | Action::Shrink { dir } => {
            args.set("edge", word("edge", snake(dir)));
        }
        Action::MoveToDisplay { target } => {
            args.set("target", word("target", snake(target)));
        }
        Action::ActivateProfile { name } => {
            args.set("name", name.clone());
        }
        _ => {}
    }
    (
        i18n.get_args(&format!("action-{id}"), &args),
        i18n.get_args(&format!("action-{id}-desc"), &args),
    )
}

/// The parameter an action takes, for building a new shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    /// No parameter.
    None,
    /// A direction (`move_zone`, `grow`, `shrink`).
    Direction,
    /// Next or previous display (`move_to_display`).
    Display,
    /// A profile name (`activate_profile`).
    Profile,
}

/// Every action a shortcut can run (all but `focus_window`, which needs a window), with a
/// default parameter, in the order the "New shortcut" picker shows them.
pub fn action_catalog() -> Vec<Action> {
    use venecita_core::action::DisplayTarget;
    use venecita_core::Direction;
    vec![
        Action::MoveZone {
            dir: Direction::Left,
        },
        Action::Grow {
            dir: Direction::Right,
        },
        Action::Shrink {
            dir: Direction::Left,
        },
        Action::MoveToDisplay {
            target: DisplayTarget::Next,
        },
        Action::SnapToGrid,
        Action::PinHere,
        Action::StackNext,
        Action::StackPrev,
        Action::DynamicNext,
        Action::DynamicPrev,
        Action::FocusModeToggle,
        Action::RelayoutAll,
        Action::ActivateProfile {
            name: String::new(),
        },
        Action::NextProfile,
        Action::AutoProfile,
        Action::TogglePause,
        Action::OpenSettings,
    ]
}

/// What parameter `action` takes.
pub fn param_of(action: &Action) -> Param {
    match action {
        Action::MoveZone { .. } | Action::Grow { .. } | Action::Shrink { .. } => Param::Direction,
        Action::MoveToDisplay { .. } => Param::Display,
        Action::ActivateProfile { .. } => Param::Profile,
        _ => Param::None,
    }
}

/// Adds a shortcut, refusing keys that don't parse or are already used (also when written
/// differently, like `option+ctrl+f` for `ctrl+alt+f`). Returns the canonical key.
pub fn add_hotkey(
    hotkeys: &mut BTreeMap<String, ActionSpec>,
    key: &str,
    action: Action,
) -> Result<String, AddError> {
    let hotkey: Hotkey = key.parse().map_err(|_| AddError::Invalid)?;
    if hotkeys
        .keys()
        .any(|k| k.parse::<Hotkey>().ok().as_ref() == Some(&hotkey))
    {
        return Err(AddError::Taken);
    }
    let key = hotkey.to_string();
    hotkeys.insert(key.clone(), ActionSpec(action));
    Ok(key)
}

/// Why a shortcut could not be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddError {
    /// Not a valid shortcut.
    Invalid,
    /// Another action already uses it.
    Taken,
}

/// The config name of an action (`move_zone`, `pin_here`…).
pub fn action_id(action: &Action) -> String {
    match toml::Value::try_from(action) {
        Ok(toml::Value::Table(t)) => t
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

/// The config spelling of a parameter value (`left`, `next`).
fn snake<T: serde::Serialize>(value: &T) -> String {
    match toml::Value::try_from(value) {
        Ok(toml::Value::String(s)) => s,
        _ => String::new(),
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
    fn every_action_has_a_name_and_description_in_both_languages() {
        use crate::i18n::Lang;
        let mut actions: Vec<Action> = default_hotkeys().into_values().map(|s| s.0).collect();
        actions.extend([
            Action::ActivateProfile {
                name: "Casa".into(),
            },
            Action::AutoProfile,
            Action::TogglePause,
            Action::OpenSettings,
            Action::FocusWindow {
                id: venecita_core::window::WindowId(1),
            },
        ]);
        for lang in [Lang::En, Lang::Es] {
            let i18n = I18n::new(lang);
            for action in &actions {
                let (name, desc) = action_text(&i18n, action);
                for text in [&name, &desc] {
                    assert!(
                        !text.starts_with("action-") && !text.contains('{') && !text.is_empty(),
                        "{lang:?} {action:?}: {text:?}"
                    );
                }
            }
        }
        let es = I18n::new(Lang::Es);
        let left = Action::MoveZone {
            dir: venecita_core::Direction::Left,
        };
        assert_eq!(action_text(&es, &left).0, "Mover a la izquierda");
        let grow = Action::Grow {
            dir: venecita_core::Direction::Up,
        };
        assert_eq!(action_text(&es, &grow).0, "Agrandar (borde superior)");
        let en = I18n::new(Lang::En);
        let profile = Action::ActivateProfile {
            name: "Casa".into(),
        };
        assert_eq!(action_text(&en, &profile).0, "Profile “Casa”");
    }

    #[test]
    fn the_catalog_covers_every_action_but_focus_window() {
        let catalog = action_catalog();
        let ids: BTreeSet<String> = catalog.iter().map(action_id).collect();
        assert_eq!(ids.len(), catalog.len(), "no duplicates");
        for spec in default_hotkeys().values() {
            assert!(ids.contains(&action_id(&spec.0)), "{:?} is missing", spec.0);
        }
        assert!(!ids.contains("focus_window"));
        assert_eq!(param_of(&catalog[0]), Param::Direction);
        assert_eq!(param_of(&Action::RelayoutAll), Param::None);
    }

    #[test]
    fn adding_hotkeys_refuses_duplicates_and_bad_keys() {
        let mut map = default_hotkeys();
        // Already used, written another way.
        assert_eq!(
            add_hotkey(&mut map, "option+ctrl+f", Action::RelayoutAll),
            Err(AddError::Taken)
        );
        assert_eq!(
            add_hotkey(&mut map, "ctrl+banana", Action::RelayoutAll),
            Err(AddError::Invalid)
        );
        let key = add_hotkey(&mut map, "shift+alt+ctrl+k", Action::PinHere).unwrap();
        assert_eq!(key, "ctrl+alt+shift+k", "stored in canonical form");
        assert_eq!(map[&key].0, Action::PinHere);
    }

    #[test]
    fn labels_actions() {
        let map = default_hotkeys();
        assert_eq!(action_label(&map["ctrl+alt+s"]), "snap_to_grid");
        assert_eq!(action_label(&map["ctrl+alt+left"]), "move_zone (left)");
        assert_eq!(action_label(&map["ctrl+alt+n"]), "move_to_display (next)");
    }
}
