//! The "Profiles" tab: create, rename, duplicate and delete profiles and their layouts.

use eframe::egui;

use super::theme::{self, Icons, Kind};
use crate::icons::IconKind;
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

pub(super) fn show(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    state: &mut State,
    icons: &mut Icons,
) {
    theme::page_title(ui, i18n.get("profiles-title"));
    let count = model.config().profiles.len();
    if state.selected >= count {
        state.selected = count.saturating_sub(1);
    }
    let profile_icon = icons.tile(ui.ctx(), IconKind::Profile);
    let arrange_icon = icons.tile(ui.ctx(), IconKind::Arrange);

    // Side by side when there is room, one above the other in a narrow window.
    let narrow = ui.available_width() < 640.0;
    let direction = if narrow {
        egui::Layout::top_down(egui::Align::Min)
    } else {
        egui::Layout::left_to_right(egui::Align::Min)
    };
    let list_width = if narrow { ui.available_width() } else { 260.0 };
    ui.with_layout(direction, |ui| {
        // --- Left: the profiles ---------------------------------------------------------
        ui.vertical(|ui| {
            ui.set_width(list_width);
            theme::card(ui, None, |ui| {
                if count == 0 {
                    ui.label(egui::RichText::new(i18n.get("no-profiles")).color(theme::subtle(ui)));
                }
                for (i, profile) in model.config().profiles.iter().enumerate() {
                    let selected = state.selected == i;
                    let button = egui::Button::image_and_text(
                        egui::Image::new(&profile_icon).fit_to_exact_size(egui::Vec2::splat(20.0)),
                        &profile.name,
                    )
                    .selected(selected)
                    .frame(selected)
                    .min_size(egui::vec2(ui.available_width(), 30.0))
                    .rounding(theme::RADIUS);
                    if ui.add(button).clicked() {
                        state.selected = i;
                        state.profile_name = None;
                        state.layout_names.clear();
                    }
                }
                theme::divider(ui);
                ui.add(
                    egui::TextEdit::singleline(&mut state.new_profile)
                        .hint_text(i18n.get("profile-name"))
                        .desired_width(f32::INFINITY),
                );
                if theme::button(ui, Kind::Primary, None, i18n.get("new-profile")).clicked() {
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
        });

        // --- Right: the selected profile ------------------------------------------------
        ui.vertical(|ui| {
            let Some(profile) = model.config().profiles.get(state.selected).cloned() else {
                return;
            };
            let idx = state.selected;

            theme::card(ui, Some(&i18n.get("profile-actions")), |ui| {
                ui.horizontal(|ui| {
                    let name = match &mut state.profile_name {
                        Some((i, name)) if *i == idx => name,
                        slot => &mut slot.insert((idx, profile.name.clone())).1,
                    };
                    ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                    if theme::button(ui, Kind::Secondary, None, i18n.get("rename")).clicked() {
                        let name = name.clone();
                        state.error = model.rename_profile(idx, &name).err();
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if theme::button(ui, Kind::Secondary, None, i18n.get("duplicate")).clicked() {
                        match model.duplicate_profile(idx) {
                            Ok(()) => state.selected = model.config().profiles.len() - 1,
                            Err(e) => state.error = Some(e),
                        }
                        state.profile_name = None;
                    }
                    if theme::button(ui, Kind::Danger, None, i18n.get("delete")).clicked() {
                        state.error = model.delete_profile(idx).err();
                        state.profile_name = None;
                        state.layout_names.clear();
                    }
                });
            });

            let display_ids: Vec<String> = model
                .config()
                .displays
                .iter()
                .map(|d| d.id.clone())
                .collect();
            state.layout_names.resize(profile.layouts.len(), None);
            let mut delete = None;
            for (li, layout) in profile.layouts.iter().enumerate() {
                let caption = if li == 0 {
                    Some(i18n.get("layouts"))
                } else {
                    None
                };
                theme::card(ui, caption.as_deref(), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(&arrange_icon)
                                .fit_to_exact_size(egui::Vec2::splat(20.0)),
                        );
                        let name =
                            state.layout_names[li].get_or_insert_with(|| layout.name.clone());
                        ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                        if theme::button(ui, Kind::Secondary, None, i18n.get("rename")).clicked() {
                            let name = name.clone();
                            state.error = model.rename_layout(idx, li, &name).err();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::button(ui, Kind::Danger, None, i18n.get("delete")).clicked() {
                                delete = Some(li);
                            }
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(i18n.get("layout-displays"))
                                .color(theme::subtle(ui)),
                        );
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
                    ui.label(
                        egui::RichText::new(i18n.get_args("rule-count", &args))
                            .small()
                            .color(theme::subtle(ui)),
                    );
                });
            }
            if let Some(li) = delete {
                state.error = model.delete_layout(idx, li).err();
                state.layout_names.clear();
            }

            theme::card(ui, None, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_layout)
                            .hint_text(i18n.get("layout-name"))
                            .desired_width(220.0),
                    );
                    if theme::button(ui, Kind::Primary, None, i18n.get("new-layout")).clicked() {
                        // A new layout uses every defined display; untick the ones it doesn't need.
                        match model.add_layout(idx, &state.new_layout, &display_ids) {
                            Ok(()) => state.new_layout.clear(),
                            Err(e) => state.error = Some(e),
                        }
                    }
                });
            });

            if let Some(e) = &state.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        });
    });
}
