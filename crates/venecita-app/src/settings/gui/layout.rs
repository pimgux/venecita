//! The "Layout" tab: each display of a layout drawn to scale with its grid. Drag across
//! cells to pick a zone, then choose the apps that go there. Click a rule to select it;
//! drag a new zone and apply to move or resize it.

use std::time::{Duration, Instant};

use eframe::egui;
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
use venecita_core::config::{Layout, Placement};
use venecita_core::display::resolve_displays;
use venecita_core::grid::{GridSize, Zone};

use super::displays;
use super::rule_color;
use crate::intl::I18n;
use crate::settings::model::{zone_from_drag, SettingsModel};

/// Aspect ratio used for a display that isn't connected right now.
const FALLBACK_SIZE: (f64, f64) = (1600.0, 1000.0);
/// Maximum height of a drawn display, in points.
const MAX_HEIGHT: f32 = 320.0;

#[derive(Default)]
pub(super) struct State {
    profile: usize,
    layout: usize,
    selection: Option<Placement>,
    /// Display id and cell where the current drag started.
    drag_from: Option<(String, (u8, u8))>,
    /// The rule being edited, if the selection came from one.
    editing_rule: Option<usize>,
    /// Comma-separated app ids for the selection.
    apps: String,
    running: Vec<(String, String)>,
    running_refreshed: Option<Instant>,
    error: Option<String>,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    state: &mut State,
    displays: &displays::State,
) {
    ui.heading(i18n.get("layout-editor-title"));
    let config = model.config().clone();
    if config.profiles.is_empty() {
        ui.label(egui::RichText::new(i18n.get("no-profiles")).weak());
        return;
    }

    // --- Profile / layout pickers ---
    let before = (state.profile, state.layout);
    state.profile = state.profile.min(config.profiles.len() - 1);
    ui.horizontal(|ui| {
        ui.label(i18n.get("select-profile"));
        egui::ComboBox::from_id_salt("profile")
            .selected_text(&config.profiles[state.profile].name)
            .show_ui(ui, |ui| {
                for (i, p) in config.profiles.iter().enumerate() {
                    ui.selectable_value(&mut state.profile, i, &p.name);
                }
            });
        let layouts = &config.profiles[state.profile].layouts;
        if layouts.is_empty() {
            return;
        }
        state.layout = state.layout.min(layouts.len() - 1);
        ui.label(i18n.get("select-layout"));
        egui::ComboBox::from_id_salt("layout")
            .selected_text(&layouts[state.layout].name)
            .show_ui(ui, |ui| {
                for (i, l) in layouts.iter().enumerate() {
                    ui.selectable_value(&mut state.layout, i, &l.name);
                }
            });
    });
    if (state.profile, state.layout) != before {
        clear_selection(state);
    }
    let Some(layout) = config.profiles[state.profile]
        .layouts
        .get(state.layout)
        .cloned()
    else {
        ui.label(egui::RichText::new(i18n.get("no-layouts")).weak());
        return;
    };
    if state.editing_rule.is_some_and(|i| i >= layout.rules.len()) {
        clear_selection(state);
    }

    // --- The displays, to scale ---
    let resolved = resolve_displays(&config.displays, displays.connected());
    let size_of = |id: &str| {
        resolved
            .iter()
            .find(|r| r.def_id == id)
            .and_then(|r| displays.connected().iter().find(|d| d.handle == r.handle))
            .map(|d| (d.frame.w, d.frame.h))
            .unwrap_or(FALLBACK_SIZE)
    };
    let sizes: Vec<(f64, f64)> = layout.displays.iter().map(|id| size_of(id)).collect();
    let spacing = 16.0;
    let total_w: f64 = sizes.iter().map(|s| s.0).sum();
    let max_h = sizes.iter().map(|s| s.1).fold(1.0, f64::max);
    let avail = ui.available_width() - spacing * layout.displays.len() as f32;
    let scale = (f64::from(avail) / total_w.max(1.0)).min(f64::from(MAX_HEIGHT) / max_h);

    ui.add_space(6.0);
    ui.label(egui::RichText::new(i18n.get("drag-hint")).weak());
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        for (id, (w, h)) in layout.displays.iter().zip(&sizes) {
            let grid = config.grid_for(id);
            ui.vertical(|ui| {
                ui.monospace(format!("{id}  ({}×{})", grid.cols, grid.rows));
                let size = egui::vec2((w * scale) as f32, (h * scale) as f32);
                display_canvas(
                    ui,
                    state,
                    &layout,
                    id,
                    grid,
                    size,
                    &i18n.get("dynamic-zone"),
                );
            });
        }
    });

    // --- The selection ---
    ui.add_space(10.0);
    let Some(selection) = state.selection.clone() else {
        ui.label(egui::RichText::new(i18n.get("no-selection")).weak());
        rules_list(ui, state, &layout);
        return;
    };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!(
            "{} · col {} · row {} · {}×{}",
            selection.display, selection.col, selection.row, selection.w, selection.h
        ));
        ui.label(i18n.get("zone-apps"));
        ui.add(
            egui::TextEdit::multiline(&mut state.apps)
                .hint_text(i18n.get("type-app"))
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );

        refresh_running(state);
        if !state.running.is_empty() {
            ui.label(egui::RichText::new(i18n.get("running-apps")).weak());
            ui.horizontal_wrapped(|ui| {
                for (bundle, name) in state.running.clone() {
                    let label = if name.is_empty() {
                        bundle.clone()
                    } else {
                        name
                    };
                    if ui.small_button(label).on_hover_text(&bundle).clicked() {
                        let mut apps = parse_apps(&state.apps);
                        if !apps.contains(&bundle) {
                            apps.push(bundle);
                            state.apps = apps.join(", ");
                        }
                    }
                }
            });
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (p, l) = (state.profile, state.layout);
            let apps = parse_apps(&state.apps);
            if ui.button(i18n.get("apply-rule")).clicked() {
                let result = match state.editing_rule {
                    Some(r) => model.set_rule(p, l, r, &apps, &selection),
                    None => model
                        .add_rule(p, l, &apps, &selection)
                        .map(|r| state.editing_rule = Some(r)),
                };
                state.error = result.err();
            }
            if let Some(r) = state.editing_rule {
                if ui.button(i18n.get("delete")).clicked() {
                    state.error = model.delete_rule(p, l, r).err();
                    clear_selection(state);
                }
                if ui.button(i18n.get("new-rule")).clicked() {
                    state.editing_rule = None;
                    state.apps.clear();
                }
            }
            if ui.button(i18n.get("mark-dynamic")).clicked() {
                state.error = model.set_dynamic_zone(p, l, Some(&selection)).err();
            }
            if layout.dynamic_zone.is_some() && ui.button(i18n.get("clear-dynamic")).clicked() {
                state.error = model.set_dynamic_zone(p, l, None).err();
            }
        });
        if let Some(e) = &state.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    });
    rules_list(ui, state, &layout);
}

/// Draws one display with its grid, rules, dynamic zone and selection, and handles the
/// clicks and drags on it.
fn display_canvas(
    ui: &mut egui::Ui,
    state: &mut State,
    layout: &Layout,
    id: &str,
    grid: GridSize,
    size: egui::Vec2,
    dynamic_label: &str,
) {
    let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
    let rect = response.rect;
    let cell = egui::vec2(
        rect.width() / f32::from(grid.cols.max(1)),
        rect.height() / f32::from(grid.rows.max(1)),
    );
    let zone_rect = |z: Zone| {
        egui::Rect::from_min_size(
            rect.min + egui::vec2(f32::from(z.col) * cell.x, f32::from(z.row) * cell.y),
            egui::vec2(f32::from(z.w) * cell.x, f32::from(z.h) * cell.y),
        )
        .shrink(1.5)
    };
    let cell_at = |pos: egui::Pos2| {
        let col = ((pos.x - rect.min.x) / cell.x)
            .floor()
            .clamp(0.0, f32::from(grid.cols - 1));
        let row = ((pos.y - rect.min.y) / cell.y)
            .floor()
            .clamp(0.0, f32::from(grid.rows - 1));
        (col as u8, row as u8)
    };

    let visuals = ui.visuals();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let line = egui::Stroke::new(1.0_f32, visuals.weak_text_color().gamma_multiply(0.35));
    for c in 1..grid.cols {
        let x = rect.min.x + f32::from(c) * cell.x;
        painter.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], line);
    }
    for r in 1..grid.rows {
        let y = rect.min.y + f32::from(r) * cell.y;
        painter.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], line);
    }
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, visuals.weak_text_color()),
    );

    if let Some(dz) = layout.dynamic_zone.as_ref().filter(|d| d.display == id) {
        // Diagonal hatching, so the dynamic zone reads differently from rules.
        let r = zone_rect(dz.zone());
        let color = visuals.text_color().gamma_multiply(0.6);
        let hatch = painter.with_clip_rect(r);
        let step = 10.0;
        let mut x = r.min.x - r.height();
        while x < r.max.x {
            hatch.line_segment(
                [egui::pos2(x, r.max.y), egui::pos2(x + r.height(), r.min.y)],
                egui::Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
            );
            x += step;
        }
        painter.rect_stroke(r, 3.0, egui::Stroke::new(1.5_f32, color));
        // A label on a solid background, readable over the hatching.
        let galley = painter.layout_no_wrap(
            dynamic_label.to_string(),
            egui::FontId::proportional(12.0),
            visuals.text_color(),
        );
        let label = egui::Align2::CENTER_CENTER.anchor_size(r.center(), galley.size());
        painter.rect_filled(label.expand(3.0), 3.0, visuals.extreme_bg_color);
        painter.galley(label.min, galley, visuals.text_color());
    }

    for (i, rule) in layout.rules.iter().enumerate() {
        if rule.zone.display != id {
            continue;
        }
        let r = zone_rect(rule.zone.zone());
        let color = rule_color(i);
        painter.rect_filled(r, 3.0, color.gamma_multiply(0.55));
        painter.rect_stroke(r, 3.0, egui::Stroke::new(1.0_f32, color));
        let label = rule.apps.first().map(|a| short_app(a)).unwrap_or_default();
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    if let Some(sel) = state.selection.as_ref().filter(|s| s.display == id) {
        painter.rect_stroke(
            zone_rect(sel.zone()),
            3.0,
            egui::Stroke::new(2.5_f32, visuals.selection.stroke.color),
        );
    }

    // Interaction
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            state.drag_from = Some((id.to_string(), cell_at(pos)));
        }
    }
    if response.dragged() {
        if let (Some((from_id, from)), Some(pos)) =
            (&state.drag_from, response.interact_pointer_pos())
        {
            if from_id == id {
                state.selection = Some(Placement::new(id, zone_from_drag(*from, cell_at(pos))));
            }
        }
    }
    if response.drag_stopped() {
        state.drag_from = None;
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (col, row) = cell_at(pos);
            let hit = |z: Zone| {
                (z.col..z.col + z.w).contains(&col) && (z.row..z.row + z.h).contains(&row)
            };
            // The last rule drawn is the one on top.
            let rule = layout
                .rules
                .iter()
                .enumerate()
                .rev()
                .find(|(_, r)| r.zone.display == id && hit(r.zone.zone()));
            match rule {
                Some((i, r)) => select_rule(state, i, r.zone.clone(), &r.apps),
                None => {
                    state.editing_rule = None;
                    state.apps.clear();
                    state.selection = Some(Placement::new(id, Zone::new(col, row, 1, 1)));
                }
            }
        }
    }
}

/// The rules of the layout as a clickable list (also handy when rules overlap).
fn rules_list(ui: &mut egui::Ui, state: &mut State, layout: &Layout) {
    ui.add_space(8.0);
    for (i, rule) in layout.rules.iter().enumerate() {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 2.0, rule_color(i));
            let z = &rule.zone;
            let text = format!(
                "{}  ->  {} · {},{} · {}×{}",
                rule.apps.join(", "),
                z.display,
                z.col,
                z.row,
                z.w,
                z.h
            );
            if ui
                .selectable_label(state.editing_rule == Some(i), text)
                .clicked()
            {
                select_rule(state, i, rule.zone.clone(), &rule.apps);
            }
        });
    }
}

fn select_rule(state: &mut State, index: usize, zone: Placement, apps: &[String]) {
    state.editing_rule = Some(index);
    state.selection = Some(zone);
    state.apps = apps.join(", ");
    state.error = None;
}

fn clear_selection(state: &mut State) {
    state.selection = None;
    state.editing_rule = None;
    state.drag_from = None;
    state.apps.clear();
    state.error = None;
}

/// `com.microsoft.VSCode` → `VSCode`; `firefox.exe` → `firefox`.
fn short_app(app: &str) -> String {
    let app = app.strip_suffix(".exe").unwrap_or(app);
    app.rsplit('.').next().unwrap_or(app).to_string()
}

fn parse_apps(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Regular (Dock) applications running now, as (bundle id, name), refreshed every few
/// seconds.
fn refresh_running(state: &mut State) {
    if state
        .running_refreshed
        .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
    {
        return;
    }
    state.running_refreshed = Some(Instant::now());
    let workspace = NSWorkspace::sharedWorkspace();
    let mut apps: Vec<(String, String)> = workspace
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| {
            let bundle = a.bundleIdentifier()?.to_string();
            let name = a.localizedName().map(|n| n.to_string()).unwrap_or_default();
            Some((bundle, name))
        })
        .collect();
    apps.sort_by_key(|(_, name)| name.to_lowercase());
    apps.dedup();
    state.running = apps;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names() {
        assert_eq!(short_app("com.microsoft.VSCode"), "VSCode");
        assert_eq!(short_app("firefox.exe"), "firefox");
        assert_eq!(short_app("code"), "code");
    }

    #[test]
    fn apps_split_on_commas_and_lines() {
        assert_eq!(
            parse_apps(" a, b\n c ,, "),
            vec!["a".to_string(), "b".into(), "c".into()]
        );
    }
}
