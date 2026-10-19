//! The "Layout" tab: each display of a layout drawn to scale with its grid.
//!
//! * Drag across empty cells to pick a zone, then choose the apps that go there.
//! * Drag a rule by its body to move it (also onto another display), or by an edge or a
//!   corner to resize it. The change is applied when the mouse is released.
//! * The dynamic zone can be resized by its edges; it is moved with "Mark as dynamic zone",
//!   because it often fills a whole display and its body must stay free for new zones.

use std::time::{Duration, Instant};

use eframe::egui;
use venecita_core::config::{Layout, Placement};
use venecita_core::display::resolve_displays;
use venecita_core::grid::{GridSize, Zone};

use super::displays;
use super::rule_color;
use crate::intl::I18n;
use crate::settings::model::{zone_from_drag, zone_moved, zone_resized, Edges, SettingsModel};

/// Aspect ratio used for a display that isn't connected right now.
const FALLBACK_SIZE: (f64, f64) = (1600.0, 1000.0);
/// Maximum height of a drawn display, in points.
const MAX_HEIGHT: f32 = 320.0;
/// How close to an edge, in points, the pointer must be to resize instead of move.
const EDGE_GRAB: f32 = 7.0;

/// Something on a display that can be dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Rule(usize),
    Dynamic,
}

/// A rule or the dynamic zone being moved or resized with the mouse.
#[derive(Debug, Clone)]
struct ItemDrag {
    target: Target,
    start: Placement,
    /// Cell of the zone the pointer held, relative to its top-left cell (moves).
    grab: (u8, u8),
    /// Edges held (resizes); none for a move.
    edges: Edges,
    /// Where the item would go if released now.
    preview: Placement,
}

/// Where a display was drawn, so a drag can continue onto another display.
#[derive(Debug, Clone)]
struct Canvas {
    id: String,
    rect: egui::Rect,
    grid: GridSize,
}

impl Canvas {
    fn cell_size(&self) -> egui::Vec2 {
        egui::vec2(
            self.rect.width() / f32::from(self.grid.cols.max(1)),
            self.rect.height() / f32::from(self.grid.rows.max(1)),
        )
    }

    /// The cell under `pos`, clamped to the grid.
    fn cell_at(&self, pos: egui::Pos2) -> (u8, u8) {
        let cell = self.cell_size();
        let col = ((pos.x - self.rect.min.x) / cell.x)
            .floor()
            .clamp(0.0, f32::from(self.grid.cols - 1));
        let row = ((pos.y - self.rect.min.y) / cell.y)
            .floor()
            .clamp(0.0, f32::from(self.grid.rows - 1));
        (col as u8, row as u8)
    }

    /// The on-screen rectangle of a zone.
    fn zone_rect(&self, z: Zone) -> egui::Rect {
        let cell = self.cell_size();
        egui::Rect::from_min_size(
            self.rect.min + egui::vec2(f32::from(z.col) * cell.x, f32::from(z.row) * cell.y),
            egui::vec2(f32::from(z.w) * cell.x, f32::from(z.h) * cell.y),
        )
        .shrink(1.5)
    }
}

/// The edges of `rect` near `pos` (empty if `pos` is inside, away from the edges).
fn edges_near(rect: egui::Rect, pos: egui::Pos2) -> Edges {
    // Small zones keep a grabbable body in the middle.
    let mx = EDGE_GRAB.min(rect.width() / 4.0);
    let my = EDGE_GRAB.min(rect.height() / 4.0);
    Edges {
        left: (pos.x - rect.min.x).abs() <= mx,
        right: (pos.x - rect.max.x).abs() <= mx,
        top: (pos.y - rect.min.y).abs() <= my,
        bottom: (pos.y - rect.max.y).abs() <= my,
    }
}

fn resize_cursor(edges: Edges) -> egui::CursorIcon {
    use egui::CursorIcon::*;
    match (edges.left || edges.right, edges.top || edges.bottom) {
        (true, false) => ResizeHorizontal,
        (false, true) => ResizeVertical,
        _ if (edges.left && edges.top) || (edges.right && edges.bottom) => ResizeNwSe,
        _ => ResizeNeSw,
    }
}

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
    /// A rule or the dynamic zone being dragged.
    item_drag: Option<ItemDrag>,
    /// Displays drawn in the previous frame (a drag can end on any of them).
    canvases: Vec<Canvas>,
    /// Displays drawn in this frame.
    canvases_next: Vec<Canvas>,
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
    state.canvases = std::mem::take(&mut state.canvases_next);
    let mut commit = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        for (id, (w, h)) in layout.displays.iter().zip(&sizes) {
            let grid = config.grid_for(id);
            ui.vertical(|ui| {
                ui.monospace(format!("{id}  ({}×{})", grid.cols, grid.rows));
                let size = egui::vec2((w * scale) as f32, (h * scale) as f32);
                let canvas = Canvas {
                    id: id.clone(),
                    // Replaced by the real rectangle once allocated.
                    rect: egui::Rect::NOTHING,
                    grid,
                };
                if let Some(c) =
                    display_canvas(ui, state, &layout, canvas, size, &i18n.get("dynamic-zone"))
                {
                    commit = Some(c);
                }
            });
        }
    });

    // A drag that ended: apply it to the configuration.
    if let Some((target, placement)) = commit {
        let (p, l) = (state.profile, state.layout);
        state.error = match target {
            Target::Rule(i) => {
                let apps = layout.rules[i].apps.clone();
                let result = model.set_rule(p, l, i, &apps, &placement);
                if result.is_ok() {
                    select_rule(state, i, placement, &apps);
                }
                result.err()
            }
            Target::Dynamic => {
                state.selection = Some(placement.clone());
                state.editing_rule = None;
                model.set_dynamic_zone(p, l, Some(&placement)).err()
            }
        };
    }

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
/// clicks and drags on it. Returns a move or resize to apply when a drag ends here.
fn display_canvas(
    ui: &mut egui::Ui,
    state: &mut State,
    layout: &Layout,
    canvas: Canvas,
    size: egui::Vec2,
    dynamic_label: &str,
) -> Option<(Target, Placement)> {
    let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
    let canvas = Canvas {
        rect: response.rect,
        ..canvas
    };
    let id = canvas.id.as_str();
    let grid = canvas.grid;
    let rect = canvas.rect;
    let cell = canvas.cell_size();

    // Where each item is drawn: the drag preview replaces the dragged item.
    let dragging = state.item_drag.as_ref().map(|d| d.target);
    let preview = state.item_drag.as_ref().map(|d| d.preview.clone());
    let placement_of = |target: Target, saved: &Placement| -> Placement {
        match (dragging, &preview) {
            (Some(t), Some(p)) if t == target => p.clone(),
            _ => saved.clone(),
        }
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

    let dynamic = layout
        .dynamic_zone
        .as_ref()
        .map(|d| placement_of(Target::Dynamic, d))
        .filter(|d| d.display == id);
    if let Some(dz) = &dynamic {
        // Diagonal hatching, so the dynamic zone reads differently from rules.
        let r = canvas.zone_rect(dz.zone());
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
        let width: f32 = if dragging == Some(Target::Dynamic) {
            2.5
        } else {
            1.5
        };
        painter.rect_stroke(r, 3.0, egui::Stroke::new(width, color));
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

    let rules: Vec<(usize, Placement)> = layout
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| (i, placement_of(Target::Rule(i), &r.zone)))
        .filter(|(_, z)| z.display == id)
        .collect();
    for (i, zone) in &rules {
        let r = canvas.zone_rect(zone.zone());
        let color = rule_color(*i);
        let dragged = dragging == Some(Target::Rule(*i));
        painter.rect_filled(
            r,
            3.0,
            color.gamma_multiply(if dragged { 0.8 } else { 0.55 }),
        );
        painter.rect_stroke(
            r,
            3.0,
            egui::Stroke::new(if dragged { 2.5_f32 } else { 1.0_f32 }, color),
        );
        let label = layout.rules[*i]
            .apps
            .first()
            .map(|a| short_app(a))
            .unwrap_or_default();
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }

    if state.item_drag.is_none() {
        if let Some(sel) = state.selection.as_ref().filter(|s| s.display == id) {
            painter.rect_stroke(
                canvas.zone_rect(sel.zone()),
                3.0,
                egui::Stroke::new(2.5_f32, visuals.selection.stroke.color),
            );
        }
    }

    // What is under the pointer: the top-most rule (body or edges), else the dynamic
    // zone's edges.
    let hit = |pos: egui::Pos2| -> Option<(Target, Placement, Edges)> {
        for (i, zone) in rules.iter().rev() {
            let r = canvas.zone_rect(zone.zone());
            if r.expand(EDGE_GRAB / 2.0).contains(pos) {
                return Some((Target::Rule(*i), zone.clone(), edges_near(r, pos)));
            }
        }
        let dz = dynamic.as_ref()?;
        let r = canvas.zone_rect(dz.zone());
        let edges = edges_near(r, pos);
        (r.expand(EDGE_GRAB / 2.0).contains(pos) && edges.any())
            .then(|| (Target::Dynamic, dz.clone(), edges))
    };

    // Cursor feedback.
    if state.item_drag.is_none() && response.hovered() {
        if let Some((_, _, edges)) = response.hover_pos().and_then(hit) {
            ui.ctx().set_cursor_icon(if edges.any() {
                resize_cursor(edges)
            } else {
                egui::CursorIcon::Grab
            });
        }
    }

    let mut commit = None;
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            match hit(pos) {
                Some((target, zone, edges)) => {
                    match target {
                        Target::Rule(i) => {
                            select_rule(state, i, zone.clone(), &layout.rules[i].apps);
                        }
                        Target::Dynamic => {
                            state.editing_rule = None;
                            state.apps.clear();
                            state.selection = Some(zone.clone());
                        }
                    }
                    let (col, row) = canvas.cell_at(pos);
                    let z = zone.zone();
                    state.item_drag = Some(ItemDrag {
                        target,
                        grab: (col.saturating_sub(z.col), row.saturating_sub(z.row)),
                        edges,
                        start: zone.clone(),
                        preview: zone,
                    });
                }
                None => state.drag_from = Some((id.to_string(), canvas.cell_at(pos))),
            }
        }
    }
    if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(drag) = state.item_drag.as_mut() {
                ui.ctx().set_cursor_icon(if drag.edges.any() {
                    resize_cursor(drag.edges)
                } else {
                    egui::CursorIcon::Grabbing
                });
                drag.preview = drag_preview(drag, pos, &canvas, &state.canvases);
            } else if let Some((from_id, from)) = &state.drag_from {
                if from_id == id {
                    state.selection = Some(Placement::new(
                        id,
                        zone_from_drag(*from, canvas.cell_at(pos)),
                    ));
                }
            }
        }
    }
    if response.drag_stopped() {
        state.drag_from = None;
        if let Some(drag) = state.item_drag.take() {
            if drag.preview != drag.start {
                commit = Some((drag.target, drag.preview));
            }
        }
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (col, row) = canvas.cell_at(pos);
            let inside = |z: Zone| {
                (z.col..z.col + z.w).contains(&col) && (z.row..z.row + z.h).contains(&row)
            };
            // The last rule drawn is the one on top.
            match rules.iter().rev().find(|(_, z)| inside(z.zone())) {
                Some((i, z)) => select_rule(state, *i, z.clone(), &layout.rules[*i].apps),
                None => {
                    state.editing_rule = None;
                    state.apps.clear();
                    state.selection = Some(Placement::new(id, Zone::new(col, row, 1, 1)));
                }
            }
        }
    }

    state.canvases_next.push(canvas);
    commit
}

/// Where a dragged item would go with the pointer at `pos`. Moves can land on any display
/// drawn in the last frame (keeping the zone's proportion if its grid differs); resizes
/// stay on the item's own display.
fn drag_preview(drag: &ItemDrag, pos: egui::Pos2, own: &Canvas, all: &[Canvas]) -> Placement {
    let start = drag.start.zone();
    if drag.edges.any() {
        let zone = zone_resized(start, drag.edges, own.cell_at(pos));
        return Placement::new(&drag.start.display, zone);
    }
    let target = all.iter().find(|c| c.rect.contains(pos)).unwrap_or(own);
    let from_grid = all
        .iter()
        .find(|c| c.id == drag.start.display)
        .map_or(own.grid, |c| c.grid);
    let (zone, grab) = if target.id == drag.start.display {
        (start, drag.grab)
    } else {
        let z = start.rescaled(from_grid, target.grid);
        (z, (drag.grab.0.min(z.w - 1), drag.grab.1.min(z.h - 1)))
    };
    let moved = zone_moved(zone, grab, target.cell_at(pos), target.grid);
    Placement::new(&target.id, moved)
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
    state.item_drag = None;
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

/// The applications running now, as (app id, name), refreshed every few seconds.
fn refresh_running(state: &mut State) {
    if state
        .running_refreshed
        .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
    {
        return;
    }
    state.running_refreshed = Some(Instant::now());
    state.running = crate::system::running_apps();
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
