//! The "Shortcuts" tab: every binding with a key-capture field and conflict warnings.

use eframe::egui;
use venecita_core::config::default_hotkeys;
use venecita_core::Hotkey;

use crate::intl::I18n;
use crate::settings::keys::{action_label, capture, conflicts, HeldModifiers};
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
    ui.heading(i18n.get("hotkeys-title"));
    ui.label(egui::RichText::new(i18n.get("capture-hint")).weak());
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
    let mut rows: Vec<(String, String)> = hotkeys
        .iter()
        .map(|(k, spec)| (action_label(spec), k.clone()))
        .collect();
    rows.sort();

    let mut remove = None;
    egui::Grid::new("hotkeys")
        .num_columns(3)
        .striped(true)
        .spacing([24.0, 6.0])
        .show(ui, |ui| {
            ui.strong(i18n.get("col-action"));
            ui.strong(i18n.get("col-shortcut"));
            ui.label("");
            ui.end_row();
            for (label, key) in &rows {
                ui.label(label);
                ui.horizontal(|ui| {
                    if state.capturing.as_deref() == Some(key.as_str()) {
                        ui.add(egui::Button::new(i18n.get("press-keys")).selected(true));
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut state.typed)
                                .hint_text("ctrl+alt+k")
                                .desired_width(140.0),
                        );
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            match state.typed.parse::<Hotkey>() {
                                Ok(h) => rebind(model, state, i18n, key, &h.to_string()),
                                Err(e) => state.error = Some(e.to_string()),
                            }
                        }
                        if ui.button(i18n.get("cancel")).clicked() {
                            state.capturing = None;
                        }
                    } else {
                        let valid = key.parse::<Hotkey>().is_ok();
                        let mut text = egui::RichText::new(key).monospace();
                        if !valid || clashing.contains(key) {
                            text = text.color(ui.visuals().error_fg_color);
                        }
                        if ui.button(text).clicked() {
                            state.capturing = Some(key.clone());
                            state.typed.clear();
                            state.error = None;
                        }
                        if clashing.contains(key) {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                i18n.get("conflict-warning"),
                            );
                        }
                    }
                });
                if ui.small_button(i18n.get("remove")).clicked() {
                    remove = Some(key.clone());
                }
                ui.end_row();
            }
        });

    if let Some(key) = remove {
        let mut map = hotkeys.clone();
        map.remove(&key);
        state.error = model.set_hotkeys(&map).err();
    }

    ui.add_space(8.0);
    if ui.button(i18n.get("reset-hotkeys")).clicked() {
        state.error = model.set_hotkeys(&default_hotkeys()).err();
        state.capturing = None;
    }
    if let Some(e) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
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
