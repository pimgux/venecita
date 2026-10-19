//! The "Profiles" tab: create, rename, duplicate and delete profiles and their layouts.

use eframe::egui;

use crate::intl::I18n;
use crate::settings::model::SettingsModel;

#[derive(Default)]
pub(super) struct State {
    selected: usize,
    new_profile: String,
    /// Name being typed for the selected profile.
    profile_name: Option<(usize, String)>,
    new_layout: String,
    /// Names being typed for layouts, by index.
    layout_names: Vec<Option<String>>,
    error: Option<String>,
}

pub(super) fn show(ui: &mut egui::Ui, model: &mut SettingsModel, i18n: &I18n, state: &mut State) {
    ui.heading(i18n.get("profiles-title"));
    let count = model.config().profiles.len();
    if state.selected >= count {
        state.selected = count.saturating_sub(1);
    }

    ui.columns(2, |cols| {
        // --- Left: profile list ---
        let ui = &mut cols[0];
        if count == 0 {
            ui.label(egui::RichText::new(i18n.get("no-profiles")).weak());
        }
        for (i, profile) in model.config().profiles.iter().enumerate() {
            if ui
                .selectable_label(state.selected == i, &profile.name)
                .clicked()
            {
                state.selected = i;
                state.profile_name = None;
                state.layout_names.clear();
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.new_profile)
                    .hint_text(i18n.get("profile-name"))
                    .desired_width(160.0),
            );
            if ui.button(i18n.get("new-profile")).clicked() {
                match model.add_profile(&state.new_profile) {
                    Ok(()) => {
                        state.selected = model.config().profiles.len() - 1;
                        state.new_profile.clear();
                        state.error = None;
                    }
                    Err(e) => state.error = Some(e),
                }
            }
        });

        // --- Right: the selected profile ---
        let ui = &mut cols[1];
        let Some(profile) = model.config().profiles.get(state.selected).cloned() else {
            return;
        };
        let idx = state.selected;

        ui.horizontal(|ui| {
            let name = match &mut state.profile_name {
                Some((i, name)) if *i == idx => name,
                slot => &mut slot.insert((idx, profile.name.clone())).1,
            };
            ui.add(egui::TextEdit::singleline(name).desired_width(160.0));
            if ui.button(i18n.get("rename")).clicked() {
                let name = name.clone();
                state.error = model.rename_profile(idx, &name).err();
            }
            if ui.button(i18n.get("duplicate")).clicked() {
                match model.duplicate_profile(idx) {
                    Ok(()) => state.selected = model.config().profiles.len() - 1,
                    Err(e) => state.error = Some(e),
                }
                state.profile_name = None;
            }
            if ui.button(i18n.get("delete")).clicked() {
                state.error = model.delete_profile(idx).err();
                state.profile_name = None;
                state.layout_names.clear();
            }
        });

        ui.add_space(8.0);
        ui.strong(i18n.get("layouts"));
        let display_ids: Vec<String> = model
            .config()
            .displays
            .iter()
            .map(|d| d.id.clone())
            .collect();
        state.layout_names.resize(profile.layouts.len(), None);
        let mut delete = None;
        for (li, layout) in profile.layouts.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name = state.layout_names[li].get_or_insert_with(|| layout.name.clone());
                    ui.add(egui::TextEdit::singleline(name).desired_width(160.0));
                    if ui.button(i18n.get("rename")).clicked() {
                        let name = name.clone();
                        state.error = model.rename_layout(idx, li, &name).err();
                    }
                    if ui.button(i18n.get("delete")).clicked() {
                        delete = Some(li);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(i18n.get("layout-displays"));
                    for id in &display_ids {
                        let mut on = layout.displays.contains(id);
                        if ui.checkbox(&mut on, id).changed() {
                            // Keep the configuration order of displays.
                            let displays: Vec<String> = display_ids
                                .iter()
                                .filter(|d| {
                                    if *d == id {
                                        on
                                    } else {
                                        layout.displays.contains(d)
                                    }
                                })
                                .cloned()
                                .collect();
                            state.error = model.set_layout_displays(idx, li, &displays).err();
                        }
                    }
                });
                let mut args = fluent::FluentArgs::new();
                args.set("count", layout.rules.len());
                ui.label(egui::RichText::new(i18n.get_args("rule-count", &args)).weak());
            });
        }
        if let Some(li) = delete {
            state.error = model.delete_layout(idx, li).err();
            state.layout_names.clear();
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.new_layout)
                    .hint_text(i18n.get("layout-name"))
                    .desired_width(160.0),
            );
            if ui.button(i18n.get("new-layout")).clicked() {
                // A new layout uses every defined display; untick the ones it doesn't need.
                match model.add_layout(idx, &state.new_layout, &display_ids) {
                    Ok(()) => state.new_layout.clear(),
                    Err(e) => state.error = Some(e),
                }
            }
        });

        if let Some(e) = &state.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    });
}
