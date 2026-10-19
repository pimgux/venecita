//! Translating Venecita's [`Hotkey`]s into OS-level global shortcuts.
//!
//! The config parser (`venecita_core::action`) owns the hotkey *syntax*; this module maps the
//! canonical key names onto the `global-hotkey` crate, which registers them with macOS,
//! Windows or X11.

use std::collections::HashMap;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use venecita_core::{Action, Config, Hotkey};

/// Maps a canonical key name (see `venecita_core::action::KEY_NAMES`) to a key code.
pub fn key_code(key: &str) -> Option<Code> {
    use Code::*;
    const LETTERS: [Code; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [Code; 10] = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    ];
    const FKEYS: [Code; 24] = [
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
        F21, F22, F23, F24,
    ];
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            return Some(LETTERS[(c as u8 - b'a') as usize]);
        }
        if c.is_ascii_digit() {
            return Some(DIGITS[(c as u8 - b'0') as usize]);
        }
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<usize>().ok()) {
        return FKEYS.get(n.checked_sub(1)?).copied();
    }
    Some(match key {
        "left" => ArrowLeft,
        "right" => ArrowRight,
        "up" => ArrowUp,
        "down" => ArrowDown,
        "enter" => Enter,
        "tab" => Tab,
        "space" => Space,
        "escape" => Escape,
        "backspace" => Backspace,
        "delete" => Delete,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        "minus" => Minus,
        "equal" => Equal,
        "comma" => Comma,
        "period" => Period,
        "slash" => Slash,
        "backslash" => Backslash,
        "semicolon" => Semicolon,
        "quote" => Quote,
        "backquote" => Backquote,
        "bracketleft" => BracketLeft,
        "bracketright" => BracketRight,
        _ => return None,
    })
}

/// Converts a parsed hotkey to an OS hotkey. `cmd` maps to Command / Windows / Super.
pub fn to_native(hotkey: &Hotkey) -> Option<HotKey> {
    let m = hotkey.mods;
    let mut mods = Modifiers::empty();
    for (on, flag) in [
        (m.ctrl, Modifiers::CONTROL),
        (m.alt, Modifiers::ALT),
        (m.shift, Modifiers::SHIFT),
        (m.meta, Modifiers::META),
    ] {
        if on {
            mods |= flag;
        }
    }
    let code = key_code(&hotkey.key)?;
    Some(HotKey::new((!mods.is_empty()).then_some(mods), code))
}

/// The hotkeys of a configuration, ready to register.
#[derive(Debug, Default)]
pub struct Bindings {
    /// Native hotkey, its canonical text and the action it triggers.
    pub entries: Vec<(HotKey, String, Action)>,
    /// Native hotkey id → action (what the event handler looks up).
    pub actions: HashMap<u32, Action>,
}

impl Bindings {
    /// Builds the bindings from the `[hotkeys]` table (already validated).
    pub fn from_config(config: &Config) -> Self {
        let mut b = Bindings::default();
        for (hotkey, action) in config.parsed_hotkeys() {
            if let Some(native) = to_native(&hotkey) {
                b.actions.insert(native.id(), action.clone());
                b.entries.push((native, hotkey.to_string(), action));
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use venecita_core::action::KEY_NAMES;

    #[test]
    fn every_supported_key_has_a_code() {
        let mut keys: Vec<String> = KEY_NAMES.iter().map(|s| s.to_string()).collect();
        keys.extend(('a'..='z').map(String::from));
        keys.extend(('0'..='9').map(String::from));
        keys.extend((1..=24).map(|n| format!("f{n}")));
        for key in keys {
            assert!(key_code(&key).is_some(), "no key code for `{key}`");
        }
        assert_eq!(key_code("f25"), None);
        assert_eq!(key_code("f0"), None);
        assert_eq!(key_code("nope"), None);
    }

    #[test]
    fn modifiers_are_mapped() {
        let h: Hotkey = "ctrl+alt+cmd+shift+left".parse().unwrap();
        let native = to_native(&h).unwrap();
        assert_eq!(
            native.mods,
            Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT
        );
        assert_eq!(native.key, Code::ArrowLeft);
    }

    #[test]
    fn default_config_bindings_are_unique() {
        let config = Config::default();
        let b = Bindings::from_config(&config);
        assert_eq!(b.entries.len(), config.hotkeys.len());
        assert_eq!(
            b.actions.len(),
            b.entries.len(),
            "native ids must not collide"
        );
    }
}
