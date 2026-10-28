//! The "General" tab: language, grid and gap, behavior (launch at login, automatic
//! arranging), ignored apps, focus mode, dimming, the local API and the files, after the
//! version and updates card (`updates`).

use std::path::Path;

use eframe::egui;
use venecita_core::config::Language;
use venecita_core::grid::{GridSize, MAX_GRID_CELLS};

use super::theme::{self, Icons, Kind};
use super::updates;
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
    update: &mut updates::State,
    config_path: &Path,
    icons: &mut Icons,
) {
    theme::page_title(ui, i18n.get("general-title"));

    // --- Version and updates -------------------------------------------------------------
    updates::show(ui, i18n, update, model.is_dirty());

    // --- Language and grid ---------------------------------------------------------------
    theme::card(ui, None, |ui| {
        theme::row(ui, &i18n.get("general-language"), None, |ui| {
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
        });
        theme::divider(ui);
        theme::row(
            ui,
            &i18n.get("general-grid"),
            Some(&i18n.get("general-grid-desc")),
            |ui| {
                let mut grid = model.config().general.grid;
                ui.add(egui::DragValue::new(&mut grid.rows).range(1..=MAX_GRID_CELLS));
                ui.label(i18n.get("grid-rows"));
                ui.add_space(8.0);
                ui.add(egui::DragValue::new(&mut grid.cols).range(1..=MAX_GRID_CELLS));
                ui.label(i18n.get("grid-cols"));
                if grid != model.config().general.grid {
                    model.set_grid(GridSize {
                        cols: grid.cols,
                        rows: grid.rows,
                    });
                }
            },
        );
        theme::divider(ui);
        theme::row(ui, &i18n.get("general-gap"), None, |ui| {
            let mut gap = model.config().general.gap;
            if ui
                .add(
                    egui::Slider::new(&mut gap, 0.0..=40.0)
                        .suffix(" pt")
                        .step_by(1.0),
                )
                .changed()
            {
                model.set_gap(gap.round());
            }
        });
    });

    // --- Behavior ------------------------------------------------------------------------
    theme::card(ui, Some(&i18n.get("general-behavior")), |ui| {
        theme::row(
            ui,
            &i18n.get("general-auto-arrange"),
            Some(&i18n.get("general-auto-arrange-desc")),
            |ui| {
                let mut on = model.config().general.auto_arrange;
                if theme::switch(ui, &mut on) {
                    model.set_auto_arrange(on);
                }
            },
        );
        theme::divider(ui);
        theme::row(ui, &i18n.get("general-launch-at-login"), None, |ui| {
            let mut on = model.config().general.launch_at_login;
            if theme::switch(ui, &mut on) {
                model.set_launch_at_login(on);
            }
        });
    });

    // --- Ignored apps --------------------------------------------------------------------
    theme::card(ui, Some(&i18n.get("general-ignore")), |ui| {
        ui.label(egui::RichText::new(i18n.get("ignore-hint")).color(theme::subtle(ui)));
        let mut ignore = model.config().general.ignore.clone();
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            let mut remove = None;
            for (i, app) in ignore.iter().enumerate() {
                if theme::app_chip(ui, icons, app, app, true).1 {
                    remove = Some(i);
                }
            }
            if let Some(i) = remove {
                ignore.remove(i);
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut state.new_ignore)
                    .hint_text(i18n.get("type-app"))
                    .desired_width(260.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let app = state.new_ignore.trim().to_string();
            if (theme::button(ui, Kind::Secondary, None, i18n.get("add")).clicked() || enter)
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
    });

    // --- Focus mode and dimming ----------------------------------------------------------
    theme::card(ui, Some(&i18n.get("focus-title")), |ui| {
        theme::row(ui, &i18n.get("focus-size"), None, |ui| {
            let mut size = model.config().focus.size;
            if ui
                .add(egui::Slider::new(&mut size, 0.3..=1.0).step_by(0.05))
                .changed()
            {
                model.set_focus_size(round2(size));
            }
        });
        theme::divider(ui);
        theme::row(ui, &i18n.get("focus-dim"), None, |ui| {
            let mut dim = model.config().focus.dim_opacity;
            if ui
                .add(egui::Slider::new(&mut dim, 0.0..=1.0).step_by(0.05))
                .changed()
            {
                model.set_dim_opacity(round2(dim));
            }
        });
    });
    theme::card(ui, Some(&i18n.get("dim-title")), |ui| {
        let dim = model.config().dim.clone();
        theme::row(ui, &i18n.get("dim-enabled"), None, |ui| {
            let mut on = dim.enabled;
            if theme::switch(ui, &mut on) {
                model.set_dim_unfocused(on);
            }
        });
        theme::divider(ui);
        theme::row(ui, &i18n.get("dim-opacity"), None, |ui| {
            ui.add_enabled_ui(dim.enabled, |ui| {
                let mut opacity = dim.opacity;
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.05..=0.9).step_by(0.05))
                    .changed()
                {
                    model.set_dim_unfocused_opacity(round2(opacity));
                }
            });
        });
    });

    // --- Local API -----------------------------------------------------------------------
    theme::card(ui, Some(&i18n.get("api-title")), |ui| {
        let api = model.config().api.clone();
        theme::row(ui, &i18n.get("api-enabled"), None, |ui| {
            let mut on = api.enabled;
            if theme::switch(ui, &mut on) {
                model.set_api_enabled(on);
            }
        });
        theme::divider(ui);
        theme::row(ui, &i18n.get("api-bind"), None, |ui| {
            let bind = state.api_bind.get_or_insert_with(|| api.bind.clone());
            if ui
                .add(egui::TextEdit::singleline(bind).desired_width(180.0))
                .changed()
            {
                model.set_api_bind(bind.trim());
            }
        });
        theme::divider(ui);
        theme::row(ui, &i18n.get("api-allow-lan"), None, |ui| {
            let mut on = api.allow_lan;
            if theme::switch(ui, &mut on) {
                model.set_api_allow_lan(on);
            }
        });
        if api.allow_lan {
            ui.colored_label(ui.visuals().warn_fg_color, i18n.get("api-lan-warning"));
        }
        theme::divider(ui);

        let token_path = token::path_for(config_path);
        if !state.token_loaded {
            state.token = token::read(&token_path);
            state.token_loaded = true;
        }
        theme::row(ui, &i18n.get("api-token"), None, |ui| {
            if theme::button(ui, Kind::Secondary, None, i18n.get("token-rotate")).clicked() {
                match token::rotate(&token_path) {
                    Ok(t) => {
                        state.token = Some(t);
                        state.token_message = Some(i18n.get("token-rotated"));
                    }
                    Err(e) => state.token_message = Some(e.to_string()),
                }
            }
            if let Some(t) = state.token.clone() {
                if theme::button(ui, Kind::Secondary, None, i18n.get("token-copy")).clicked() {
                    ui.output_mut(|o| o.copied_text = t.clone());
                }
                let toggle = if state.token_visible {
                    "token-hide"
                } else {
                    "token-show"
                };
                if theme::button(ui, Kind::Secondary, None, i18n.get(toggle)).clicked() {
                    state.token_visible = !state.token_visible;
                }
                let shown = if state.token_visible {
                    t
                } else {
                    "••••••••••••••••".to_string()
                };
                ui.monospace(shown);
            } else {
                ui.label(egui::RichText::new(i18n.get("token-missing")).color(theme::subtle(ui)));
            }
        });
        if let Some(message) = &state.token_message {
            ui.label(
                egui::RichText::new(message)
                    .small()
                    .color(theme::subtle(ui)),
            );
        }
    });

    // --- Files ---------------------------------------------------------------------------
    theme::card(ui, Some(&i18n.get("files-title")), |ui| {
        ui.horizontal(|ui| {
            if theme::button(ui, Kind::Secondary, None, i18n.get("open-config")).clicked() {
                crate::system::open_text_file(config_path);
            }
            if theme::button(ui, Kind::Secondary, None, i18n.get("open-logs")).clicked() {
                crate::system::open_folder(&crate::paths::log_dir());
            }
        });
        ui.label(
            egui::RichText::new(config_path.display().to_string())
                .small()
                .color(theme::subtle(ui)),
        );
    });
}

/// Sliders produce values like 0.6500000000000001; keep the file tidy.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
