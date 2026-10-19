//! The "General" tab: language, grid, gap, launch at login, ignored apps, focus mode and
//! the local API (including the access token).

use std::path::Path;

use eframe::egui;
use venecita_core::config::Language;
use venecita_core::grid::{GridSize, MAX_GRID_CELLS};

use crate::intl::I18n;
use crate::settings::model::SettingsModel;
use crate::token;

#[derive(Default)]
pub(super) struct State {
    new_ignore: String,
    api_bind: Option<String>,
    token: Option<String>,
    token_loaded: bool,
    token_visible: bool,
    token_message: Option<String>,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    state: &mut State,
    config_path: &Path,
) {
    ui.heading(i18n.get("general-title"));
    egui::Grid::new("general")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            // Language
            ui.label(i18n.get("general-language"));
            let mut language = model.config().general.language;
            let label = |l: Language| match l {
                Language::Auto => i18n.get("lang-auto"),
                Language::En => i18n.get("lang-en"),
                Language::Es => i18n.get("lang-es"),
            };
            egui::ComboBox::from_id_salt("language")
                .selected_text(label(language))
                .show_ui(ui, |ui| {
                    for l in [Language::Auto, Language::En, Language::Es] {
                        ui.selectable_value(&mut language, l, label(l));
                    }
                });
            if language != model.config().general.language {
                model.set_language(language);
            }
            ui.end_row();

            // Grid
            ui.label(i18n.get("general-grid"));
            let mut grid = model.config().general.grid;
            ui.horizontal(|ui| {
                ui.label(i18n.get("grid-cols"));
                ui.add(egui::DragValue::new(&mut grid.cols).range(1..=MAX_GRID_CELLS));
                ui.label(i18n.get("grid-rows"));
                ui.add(egui::DragValue::new(&mut grid.rows).range(1..=MAX_GRID_CELLS));
            });
            if grid != model.config().general.grid {
                model.set_grid(GridSize {
                    cols: grid.cols,
                    rows: grid.rows,
                });
            }
            ui.end_row();

            // Gap
            ui.label(i18n.get("general-gap"));
            let mut gap = model.config().general.gap;
            ui.add(
                egui::DragValue::new(&mut gap)
                    .range(0.0..=200.0)
                    .suffix(" pt"),
            );
            if gap != model.config().general.gap {
                model.set_gap(gap.round());
            }
            ui.end_row();

            // Launch at login
            ui.label("");
            let mut launch = model.config().general.launch_at_login;
            if ui
                .checkbox(&mut launch, i18n.get("general-launch-at-login"))
                .changed()
            {
                model.set_launch_at_login(launch);
            }
            ui.end_row();
        });

    // Ignored apps
    ui.add_space(12.0);
    ui.strong(i18n.get("general-ignore"));
    ui.label(egui::RichText::new(i18n.get("ignore-hint")).weak());
    let mut ignore = model.config().general.ignore.clone();
    let mut changed = false;
    let mut remove = None;
    for (i, app) in ignore.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.monospace(app);
            if ui.small_button(i18n.get("remove")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        ignore.remove(i);
        changed = true;
    }
    ui.horizontal(|ui| {
        let edit = ui
            .add(egui::TextEdit::singleline(&mut state.new_ignore).hint_text(i18n.get("type-app")));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let app = state.new_ignore.trim().to_string();
        if (ui.button(i18n.get("add")).clicked() || enter)
            && !app.is_empty()
            && !ignore.contains(&app)
        {
            ignore.push(app);
            state.new_ignore.clear();
            changed = true;
        }
    });
    if changed {
        model.set_ignore(&ignore);
    }

    // Focus mode
    ui.add_space(12.0);
    ui.strong(i18n.get("focus-title"));
    egui::Grid::new("focus")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(i18n.get("focus-size"));
            let mut size = model.config().focus.size;
            if ui
                .add(egui::Slider::new(&mut size, 0.3..=1.0).step_by(0.05))
                .changed()
            {
                model.set_focus_size(round2(size));
            }
            ui.end_row();
            ui.label(i18n.get("focus-dim"));
            let mut dim = model.config().focus.dim_opacity;
            if ui
                .add(egui::Slider::new(&mut dim, 0.0..=1.0).step_by(0.05))
                .changed()
            {
                model.set_dim_opacity(round2(dim));
            }
            ui.end_row();
        });

    // Dimming the unfocused windows
    ui.add_space(12.0);
    ui.strong(i18n.get("dim-title"));
    let dim = model.config().dim.clone();
    let mut enabled = dim.enabled;
    if ui.checkbox(&mut enabled, i18n.get("dim-enabled")).changed() {
        model.set_dim_unfocused(enabled);
    }
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label(i18n.get("dim-opacity"));
            let mut opacity = dim.opacity;
            if ui
                .add(egui::Slider::new(&mut opacity, 0.05..=0.9).step_by(0.05))
                .changed()
            {
                model.set_dim_unfocused_opacity(round2(opacity));
            }
        });
    });

    // API
    ui.add_space(12.0);
    ui.strong(i18n.get("api-title"));
    let api = model.config().api.clone();
    let mut enabled = api.enabled;
    if ui.checkbox(&mut enabled, i18n.get("api-enabled")).changed() {
        model.set_api_enabled(enabled);
    }
    ui.horizontal(|ui| {
        ui.label(i18n.get("api-bind"));
        let bind = state.api_bind.get_or_insert_with(|| api.bind.clone());
        if ui.text_edit_singleline(bind).changed() {
            model.set_api_bind(bind.trim());
        }
    });
    let mut lan = api.allow_lan;
    if ui.checkbox(&mut lan, i18n.get("api-allow-lan")).changed() {
        model.set_api_allow_lan(lan);
    }
    if lan {
        ui.colored_label(ui.visuals().warn_fg_color, i18n.get("api-lan-warning"));
    }

    // Token
    let token_path = token::path_for(config_path);
    if !state.token_loaded {
        state.token = token::read(&token_path);
        state.token_loaded = true;
    }
    ui.horizontal(|ui| {
        ui.label(i18n.get("api-token"));
        match &state.token {
            Some(t) if state.token_visible => {
                ui.monospace(t);
            }
            Some(_) => {
                ui.monospace("****************");
            }
            None => {
                ui.label(egui::RichText::new(i18n.get("token-missing")).weak());
            }
        }
        if let Some(t) = &state.token {
            let toggle = if state.token_visible {
                "token-hide"
            } else {
                "token-show"
            };
            if ui.button(i18n.get(toggle)).clicked() {
                state.token_visible = !state.token_visible;
            }
            if ui.button(i18n.get("token-copy")).clicked() {
                ui.output_mut(|o| o.copied_text = t.clone());
            }
        }
        if ui.button(i18n.get("token-rotate")).clicked() {
            match token::rotate(&token_path) {
                Ok(t) => {
                    state.token = Some(t);
                    state.token_message = Some(i18n.get("token-rotated"));
                }
                Err(e) => state.token_message = Some(e.to_string()),
            }
        }
    });
    if let Some(message) = &state.token_message {
        ui.label(egui::RichText::new(message).weak());
    }
}

/// Sliders produce values like 0.6500000000000001; keep the file tidy.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
