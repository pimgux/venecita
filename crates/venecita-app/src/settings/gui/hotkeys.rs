//! The "Shortcuts" tab: every binding with its translated name and description, the keys
//! drawn as key caps, key capture and conflict warnings.

use eframe::egui;
use venecita_core::config::default_hotkeys;
use venecita_core::Hotkey;

use super::theme::{self, Kind};
use crate::intl::I18n;
use crate::settings::keys::{
    action_catalog, action_id, action_text, add_hotkey, capture, conflicts, param_of, AddError,
    HeldModifiers, Param,
};
use crate::settings::model::SettingsModel;

/// `State::capturing` value while the keys of the new shortcut are being pressed.
const NEW: &str = "\u{0}new";

#[derive(Default)]
pub(super) struct State {
    /// The binding (its current hotkey string) waiting for a key press, or [`NEW`].
    capturing: Option<String>,
    /// Fallback text field for shortcuts the OS or the running app intercepts.
    typed: String,
    error: Option<String>,
    /// The shortcut being created: which action of the catalog, its parameter, its keys.
    new_action: usize,
    new_dir: Option<venecita_core::Direction>,
    new_target: Option<venecita_core::action::DisplayTarget>,
    new_profile: String,
    new_key: Option<String>,
    new_typed: String,
    new_message: Option<(bool, String)>,
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
                    if old == NEW {
                        state.capturing = None;
                        state.new_key = Some(new);
                    } else {
                        rebind(model, state, i18n, &old, &new);
                    }
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

    new_shortcut(ui, model, i18n, state);

    if theme::button(ui, Kind::Secondary, None, i18n.get("reset-hotkeys")).clicked() {
        state.error = model.set_hotkeys(&default_hotkeys()).err();
        state.capturing = None;
    }
    if let Some(e) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
}

/// The "New shortcut" card: pick an action (and its parameter), press the keys, add.
fn new_shortcut(ui: &mut egui::Ui, model: &mut SettingsModel, i18n: &I18n, state: &mut State) {
    use venecita_core::action::DisplayTarget;
    use venecita_core::{Action, Direction};

    let catalog = action_catalog();
    state.new_action = state.new_action.min(catalog.len() - 1);
    let profiles: Vec<String> = model
        .config()
        .profiles
        .iter()
        .map(|p| p.name.clone())
        .collect();

    theme::card(ui, Some(&i18n.get("new-shortcut")), |ui| {
        // The action.
        theme::row(ui, &i18n.get("col-action"), None, |ui| {
            let selected = action_text(i18n, &catalog[state.new_action]).0;
            egui::ComboBox::from_id_salt("new-action")
                .width(260.0)
                .selected_text(generic_name(i18n, &catalog[state.new_action], &selected))
                .show_ui(ui, |ui| {
                    for (i, action) in catalog.iter().enumerate() {
                        let name = action_text(i18n, action).0;
                        ui.selectable_value(
                            &mut state.new_action,
                            i,
                            generic_name(i18n, action, &name),
                        );
                    }
                });
        });

        // Its parameter, if it takes one.
        let template = catalog[state.new_action].clone();
        let action = match param_of(&template) {
            Param::None => Some(template),
            Param::Direction => {
                let dir = *state.new_dir.get_or_insert(Direction::Left);
                theme::divider(ui);
                theme::row(ui, &i18n.get("new-shortcut-direction"), None, |ui| {
                    let label = |d: Direction| i18n.get(&format!("dir-{}", dir_id(d)));
                    egui::ComboBox::from_id_salt("new-dir")
                        .selected_text(label(dir))
                        .show_ui(ui, |ui| {
                            for d in [
                                Direction::Left,
                                Direction::Right,
                                Direction::Up,
                                Direction::Down,
                            ] {
                                ui.selectable_value(&mut state.new_dir, Some(d), label(d));
                            }
                        });
                });
                let dir = state.new_dir.unwrap_or(Direction::Left);
                Some(match template {
                    Action::Grow { .. } => Action::Grow { dir },
                    Action::Shrink { .. } => Action::Shrink { dir },
                    _ => Action::MoveZone { dir },
                })
            }
            Param::Display => {
                let target = *state.new_target.get_or_insert(DisplayTarget::Next);
                theme::divider(ui);
                theme::row(ui, &i18n.get("new-shortcut-display"), None, |ui| {
                    let label = |t: DisplayTarget| match t {
                        DisplayTarget::Next => i18n.get("target-next"),
                        DisplayTarget::Prev => i18n.get("target-prev"),
                    };
                    egui::ComboBox::from_id_salt("new-target")
                        .selected_text(label(target))
                        .show_ui(ui, |ui| {
                            for t in [DisplayTarget::Next, DisplayTarget::Prev] {
                                ui.selectable_value(&mut state.new_target, Some(t), label(t));
                            }
                        });
                });
                Some(Action::MoveToDisplay {
                    target: state.new_target.unwrap_or(DisplayTarget::Next),
                })
            }
            Param::Profile => {
                theme::divider(ui);
                if !profiles.contains(&state.new_profile) {
                    state.new_profile = profiles.first().cloned().unwrap_or_default();
                }
                theme::row(ui, &i18n.get("select-profile"), None, |ui| {
                    egui::ComboBox::from_id_salt("new-profile")
                        .selected_text(&state.new_profile)
                        .show_ui(ui, |ui| {
                            for p in &profiles {
                                ui.selectable_value(&mut state.new_profile, p.clone(), p);
                            }
                        });
                });
                (!state.new_profile.is_empty()).then(|| Action::ActivateProfile {
                    name: state.new_profile.clone(),
                })
            }
        };

        // The keys.
        theme::divider(ui);
        theme::row(ui, &i18n.get("col-shortcut"), None, |ui| {
            if state.capturing.as_deref() == Some(NEW) {
                if theme::button(ui, Kind::Secondary, None, i18n.get("cancel")).clicked() {
                    state.capturing = None;
                }
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut state.new_typed)
                        .hint_text("ctrl+alt+k")
                        .desired_width(130.0),
                );
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    state.new_key = Some(state.new_typed.trim().to_string());
                    state.capturing = None;
                }
                theme::button(ui, Kind::Primary, None, i18n.get("press-keys"));
            } else {
                let text = state
                    .new_key
                    .clone()
                    .unwrap_or_else(|| i18n.get("new-shortcut-choose-keys"));
                let clicked = match &state.new_key {
                    Some(key) => keycaps(ui, key, false).clicked(),
                    None => theme::button(ui, Kind::Secondary, None, text).clicked(),
                };
                if clicked {
                    state.capturing = Some(NEW.to_string());
                    state.new_typed.clear();
                    state.new_message = None;
                }
            }
        });

        ui.add_space(4.0);
        let ready = action.is_some() && state.new_key.is_some();
        let add = ui
            .add_enabled_ui(ready, |ui| {
                theme::button(ui, Kind::Primary, None, i18n.get("add"))
            })
            .inner;
        if add.clicked() {
            if let (Some(action), Some(key)) = (action, state.new_key.clone()) {
                let mut map = model.config().hotkeys.clone();
                state.new_message = Some(match add_hotkey(&mut map, &key, action) {
                    Ok(_) => match model.set_hotkeys(&map) {
                        Ok(()) => {
                            state.new_key = None;
                            (true, i18n.get("new-shortcut-added"))
                        }
                        Err(e) => (false, e),
                    },
                    Err(AddError::Taken) => {
                        (false, format!("{key}: {}", i18n.get("conflict-warning")))
                    }
                    Err(AddError::Invalid) => (
                        false,
                        format!("{key}: {}", i18n.get("new-shortcut-invalid")),
                    ),
                });
            }
        }
        if let Some((ok, message)) = &state.new_message {
            let color = if *ok {
                egui::Color32::from_rgb(52, 199, 89)
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, message);
        }
    });
}

/// The name of an action for the picker, without its default parameter ("Move", not
/// "Move left"): the parameter is chosen in the next row.
fn generic_name(i18n: &I18n, action: &venecita_core::Action, full: &str) -> String {
    match param_of(action) {
        Param::None => full.to_string(),
        _ => i18n.get(&format!("pick-{}", action_id(action))),
    }
}

fn dir_id(dir: venecita_core::Direction) -> &'static str {
    use venecita_core::Direction::*;
    match dir {
        Left => "left",
        Right => "right",
        Up => "up",
        Down => "down",
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
