//! The "Displays" tab: connected displays next to the definitions in the configuration,
//! "Assign as…" to create a definition from a connected display, and "Identify".

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;
use venecita_core::display::{resolve_displays, DisplayInfo, DisplayMatcher};
use venecita_core::grid::{GridSize, MAX_GRID_CELLS};

use super::theme::{self, Icons, Kind};
use crate::icons::IconKind;
use crate::intl::I18n;
use crate::settings::model::{matcher_for, suggest_display_id, SettingsModel};

/// How often the connected displays are re-read while the window is open.
const REFRESH: Duration = Duration::from_secs(2);
/// How long "Identify" shows the numbers.
const IDENTIFY_FOR: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(super) struct State {
    connected: Vec<DisplayInfo>,
    refreshed: Option<Instant>,
    error: Option<String>,
    /// Ids typed in the "Assign as…" fields, by display handle.
    assign_ids: HashMap<String, String>,
    identify_until: Option<Instant>,
}

impl State {
    /// The displays connected right now (refreshed every couple of seconds).
    pub(super) fn connected(&self) -> &[DisplayInfo] {
        &self.connected
    }

    pub(super) fn refresh_if_stale(&mut self) {
        if self.refreshed.is_some_and(|t| t.elapsed() < REFRESH) {
            return;
        }
        self.refreshed = Some(Instant::now());
        match crate::system::connected_displays() {
            Ok(list) => {
                self.connected = list;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    state: &mut State,
    icons: &mut Icons,
) {
    ui.horizontal(|ui| {
        theme::page_title(ui, i18n.get("displays-title"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let display_icon = icons.tile(ui.ctx(), IconKind::Display);
            if theme::button(ui, Kind::Primary, Some(&display_icon), i18n.get("identify"))
                .on_hover_text(i18n.get("identify-hint"))
                .clicked()
            {
                state.identify_until = Some(Instant::now() + IDENTIFY_FOR);
            }
        });
    });
    if let Some(e) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }

    let defs = model.config().displays.clone();
    let resolved = resolve_displays(&defs, &state.connected);
    let def_for = |handle: &str| {
        resolved
            .iter()
            .find(|r| r.handle == handle)
            .map(|r| r.def_id.clone())
    };
    let display_icon = icons.tile(ui.ctx(), IconKind::Display);

    // --- Connected ---
    theme::card(ui, Some(&i18n.get("connected-displays")), |ui| {
        ui.label(
            egui::RichText::new(i18n.get("connected-hint"))
                .small()
                .color(theme::subtle(ui)),
        );
        for (n, info) in state.connected.iter().enumerate() {
            theme::divider(ui);
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&display_icon).fit_to_exact_size(egui::Vec2::splat(32.0)));
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!("{}  {}", n + 1, info.name)).strong());
                    let kind = if info.builtin {
                        i18n.get("display-builtin")
                    } else {
                        i18n.get("display-external")
                    };
                    let (w, h) = info.pixel_size();
                    let matched = def_for(&info.handle)
                        .map(|id| format!("{}: {id}", i18n.get("displays-matched-as")))
                        .unwrap_or_else(|| i18n.get("display-unmatched"));
                    ui.label(
                        egui::RichText::new(format!("{kind} · {w} × {h} · {matched}"))
                            .small()
                            .color(theme::subtle(ui)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let id = state
                        .assign_ids
                        .entry(info.handle.clone())
                        .or_insert_with(|| suggest_display_id(info, &defs))
                        .clone();
                    if theme::button(ui, Kind::Secondary, None, i18n.get("assign-as")).clicked() {
                        match model.add_display(&id, &matcher_for(info)) {
                            Ok(_) => {
                                state.error = None;
                                state.assign_ids.remove(&info.handle);
                            }
                            Err(e) => state.error = Some(e),
                        }
                    }
                    if let Some(id) = state.assign_ids.get_mut(&info.handle) {
                        ui.add(egui::TextEdit::singleline(id).desired_width(120.0));
                    }
                });
            });
        }
    });

    // --- Defined ---
    let mut delete = None;
    theme::card(ui, Some(&i18n.get("defined-displays")), |ui| {
        for (i, def) in defs.iter().enumerate() {
            if i > 0 {
                theme::divider(ui);
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&def.id).strong().monospace());
                    ui.label(
                        egui::RichText::new(matcher_summary(&def.matcher))
                            .small()
                            .color(theme::subtle(ui)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::button(ui, Kind::Danger, None, i18n.get("delete")).clicked() {
                        delete = Some(i);
                    }
                    let mut custom = def.grid.is_some();
                    let mut grid = def.grid.unwrap_or(model.config().general.grid);
                    let mut resized = false;
                    if custom {
                        resized |= ui
                            .add(egui::DragValue::new(&mut grid.rows).range(1..=MAX_GRID_CELLS))
                            .changed();
                        ui.label("×");
                        resized |= ui
                            .add(egui::DragValue::new(&mut grid.cols).range(1..=MAX_GRID_CELLS))
                            .changed();
                    }
                    let toggled = theme::switch(ui, &mut custom);
                    ui.label(i18n.get("display-grid"));
                    if toggled || resized {
                        let grid = custom.then_some(GridSize {
                            cols: grid.cols,
                            rows: grid.rows,
                        });
                        state.error = model.set_display_grid(i, grid).err();
                    }
                });
            });
        }
    });
    if let Some(i) = delete {
        state.error = model.delete_display(i).err();
    }
}

/// `name ~ "DELL U2723QE", serial 1234` style description of a matcher.
fn matcher_summary(m: &DisplayMatcher) -> String {
    let mut parts = Vec::new();
    if let Some(b) = m.builtin {
        parts.push(if b {
            "built-in".into()
        } else {
            "external".into()
        });
    }
    if let Some(n) = &m.name {
        parts.push(format!("name ~ \"{n}\""));
    }
    for (label, value) in [
        ("vendor", m.vendor),
        ("model", m.model),
        ("serial", m.serial),
        ("width", m.width),
        ("height", m.height),
    ] {
        if let Some(v) = value {
            parts.push(format!("{label} {v}"));
        }
    }
    parts.join(", ")
}

/// While "Identify" is active, shows a big number in the middle of each connected display.
pub(super) fn show_identify(ctx: &egui::Context, state: &State) {
    let Some(until) = state.identify_until else {
        return;
    };
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return;
    }
    ctx.request_repaint_after(left);
    const SIZE: f32 = 220.0;
    for (n, info) in state.connected.iter().enumerate() {
        let x = (info.frame.x + info.frame.w / 2.0) as f32 - SIZE / 2.0;
        let y = (info.frame.y + info.frame.h / 2.0) as f32 - SIZE / 2.0;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(("identify", &info.handle)),
            egui::ViewportBuilder::default()
                .with_title("Venecita Dock")
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_mouse_passthrough(true)
                .with_taskbar(false)
                .with_position([x, y])
                .with_inner_size([SIZE, SIZE]),
            |ctx, _class| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none())
                    .show(ctx, |ui| {
                        let rect = ui.max_rect();
                        let painter = ui.painter();
                        painter.rect_filled(rect, 24.0, egui::Color32::from_black_alpha(190));
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            (n + 1).to_string(),
                            egui::FontId::proportional(140.0),
                            egui::Color32::WHITE,
                        );
                    });
            },
        );
    }
}
