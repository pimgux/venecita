//! The "Shortcuts" tab: every binding with its translated name and description, the keys
//! drawn as key caps, key capture and conflict warnings.

use eframe::egui;
use venecita_core::config::default_hotkeys;
use venecita_core::Hotkey;

use super::theme::{self, Kind};
use crate::intl::I18n;
use crate::settings::keys::{action_id, action_text, capture, conflicts, HeldModifiers};
use crate::settings::model::SettingsModel;

#[derive(Default)]
pub(super) struct State {
    /// The binding (its current hotkey string) waiting for a key press.
    capturing: Option<String>,
    /// Fallback text field for shortcuts the OS or the running app intercepts.
    typed: String,
    error: Option<String>,
}

pub(super) fn show(ui: &mut egui::Ui, model: &mut SettingsModel, i18n: &I18n, state: &mut State) {
    theme::page_title(ui, i18n.get("hotkeys-title"));
    ui.label(egui::RichText::new(i18n.get("capture-hint")).color(theme::subtle(ui)));
    ui.add_space(6.0);

    // A key press while capturing (unless the fallback text field has the focus).
    if let Some(old) = state.capturing.clone() {
        if !ui.ctx().wants_keyboard_input() {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                if key == egui::Key::Escape && !m.any() {
                    state.capturing = None;
                } else if let Some(new) = capture(
                    HeldModifiers {
                        ctrl: m.ctrl,
                        alt: m.alt,
                        shift: m.shift,
                        cmd: m.mac_cmd,
                    },
                    key.name(),
                ) {
                    rebind(model, state, i18n, &old, &new);
                }
            }
        }
    }

    let hotkeys = model.config().hotkeys.clone();
    let clashing = conflicts(&hotkeys);
    // Grouped by action, then by name ("Move left", "Move right"…).
    let mut rows: Vec<(String, String, String, String)> = hotkeys
        .iter()
        .map(|(key, spec)| {
            let (name, desc) = action_text(i18n, &spec.0);
            (action_id(&spec.0), name, desc, key.clone())
        })
        .collect();
    rows.sort();

    let mut remove = None;
    theme::card(ui, None, |ui| {
        for (i, (_, name, desc, key)) in rows.iter().enumerate() {
            if i > 0 {
                theme::divider(ui);
            }
            theme::row(ui, name, Some(desc), |ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("×").strong()).frame(false))
                    .on_hover_text(i18n.get("remove"))
                    .clicked()
                {
                    remove = Some(key.clone());
                }
                if state.capturing.as_deref() == Some(key.as_str()) {
                    if theme::button(ui, Kind::Secondary, None, i18n.get("cancel")).clicked() {
                        state.capturing = None;
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut state.typed)
                            .hint_text("ctrl+alt+k")
                            .desired_width(130.0),
                    );
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        match state.typed.parse::<Hotkey>() {
                            Ok(h) => rebind(model, state, i18n, key, &h.to_string()),
                            Err(e) => state.error = Some(e.to_string()),
                        }
                    }
                    theme::button(ui, Kind::Primary, None, i18n.get("press-keys"));
                } else {
                    let bad = key.parse::<Hotkey>().is_err() || clashing.contains(key);
                    if keycaps(ui, key, bad)
                        .on_hover_text(i18n.get("edit"))
                        .clicked()
                    {
                        state.capturing = Some(key.clone());
                        state.typed.clear();
                        state.error = None;
                    }
                    if clashing.contains(key) {
                        ui.colored_label(ui.visuals().error_fg_color, i18n.get("conflict-warning"));
                    }
                }
            });
        }
    });

    if let Some(key) = remove {
        let mut map = hotkeys.clone();
        map.remove(&key);
        state.error = model.set_hotkeys(&map).err();
    }

    if theme::button(ui, Kind::Secondary, None, i18n.get("reset-hotkeys")).clicked() {
        state.error = model.set_hotkeys(&default_hotkeys()).err();
        state.capturing = None;
    }
    if let Some(e) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
}

/// A shortcut drawn as key caps (`Ctrl` `Alt` `Left`). Clickable to change it.
fn keycaps(ui: &mut egui::Ui, key: &str, error: bool) -> egui::Response {
    let caps: Vec<String> = key.split('+').map(cap_label).collect();
    let fill = ui.visuals().widgets.inactive.weak_bg_fill;
    let text = if error {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    let inner = egui::Frame::none()
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for cap in &caps {
                    egui::Frame::none()
                        .fill(fill)
                        .rounding(egui::Rounding::same(5.0))
                        .inner_margin(egui::Margin::symmetric(7.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(cap).monospace().color(text));
                        });
                }
            });
        })
        .response;
    ui.interact(inner.rect, inner.id.with("caps"), egui::Sense::click())
}

/// How one part of a shortcut is written on its key cap.
fn cap_label(part: &str) -> String {
    let cmd = if cfg!(windows) { "Win" } else { "Cmd" };
    match part {
        "ctrl" => "Ctrl".into(),
        "alt" => if cfg!(target_os = "macos") {
            "Option"
        } else {
            "Alt"
        }
        .into(),
        "shift" => "Shift".into(),
        "cmd" => cmd.into(),
        "bracketleft" => "[".into(),
        "bracketright" => "]".into(),
        "period" => ".".into(),
        "comma" => ",".into(),
        "minus" => "-".into(),
        "equal" => "=".into(),
        "slash" => "/".into(),
        "backslash" => "\\".into(),
        "semicolon" => ";".into(),
        "quote" => "'".into(),
        "backquote" => "`".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        }
    }
}

/// Moves the binding at `old` to `new`, refusing to silently replace another binding.
fn rebind(model: &mut SettingsModel, state: &mut State, i18n: &I18n, old: &str, new: &str) {
    state.capturing = None;
    if old == new {
        return;
    }
    let mut map = model.config().hotkeys.clone();
    let taken = map
        .keys()
        .any(|k| k != old && k.parse::<Hotkey>().ok() == new.parse::<Hotkey>().ok());
    if taken {
        state.error = Some(format!("{new}: {}", i18n.get("conflict-warning")));
        return;
    }
    if let Some(spec) = map.remove(old) {
        map.insert(new.to_string(), spec);
        state.error = model.set_hotkeys(&map).err();
    }
}
