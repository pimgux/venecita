//! The "Displays" tab: connected displays next to the definitions in the configuration,
//! "Assign as…" to create a definition from a connected display, and "Identify".

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;
use venecita_core::display::{resolve_displays, DisplayInfo, DisplayMatcher};
use venecita_core::grid::{GridSize, MAX_GRID_CELLS};

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

pub(super) fn show(ui: &mut egui::Ui, model: &mut SettingsModel, i18n: &I18n, state: &mut State) {
    ui.heading(i18n.get("displays-title"));
    ui.horizontal(|ui| {
        if ui.button(i18n.get("identify")).clicked() {
            state.identify_until = Some(Instant::now() + IDENTIFY_FOR);
        }
        ui.label(egui::RichText::new(i18n.get("identify-hint")).weak());
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

    // --- Connected ---
    ui.add_space(8.0);
    ui.strong(i18n.get("connected-displays"));
    egui::Grid::new("connected")
        .num_columns(5)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("#");
            ui.label(i18n.get("display-name"));
            ui.label(i18n.get("display-resolution"));
            ui.label(i18n.get("displays-matched-as"));
            ui.label("");
            ui.end_row();
            for (n, info) in state.connected.iter().enumerate() {
                ui.label((n + 1).to_string());
                let kind = if info.builtin {
                    i18n.get("display-builtin")
                } else {
                    i18n.get("display-external")
                };
                ui.label(format!("{}  ({kind})", info.name));
                let (w, h) = info.pixel_size();
                ui.label(format!("{w} × {h}"));
                match def_for(&info.handle) {
                    Some(id) => {
                        ui.monospace(id);
                    }
                    None => {
                        ui.label(egui::RichText::new(i18n.get("display-unmatched")).weak());
                    }
                }
                ui.horizontal(|ui| {
                    let id = state
                        .assign_ids
                        .entry(info.handle.clone())
                        .or_insert_with(|| suggest_display_id(info, &defs));
                    ui.add(egui::TextEdit::singleline(id).desired_width(110.0));
                    if ui.button(i18n.get("assign-as")).clicked() {
                        match model.add_display(id, &matcher_for(info)) {
                            Ok(_) => {
                                state.error = None;
                                state.assign_ids.remove(&info.handle);
                            }
                            Err(e) => state.error = Some(e),
                        }
                    }
                });
                ui.end_row();
            }
        });

    // --- Defined ---
    ui.add_space(12.0);
    ui.strong(i18n.get("defined-displays"));
    let mut delete = None;
    egui::Grid::new("defined")
        .num_columns(4)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for (i, def) in defs.iter().enumerate() {
                ui.monospace(&def.id);
                ui.label(matcher_summary(&def.matcher));
                ui.horizontal(|ui| {
                    let mut custom = def.grid.is_some();
                    let mut grid = def.grid.unwrap_or(model.config().general.grid);
                    let toggled = ui.checkbox(&mut custom, i18n.get("display-grid")).changed();
                    let mut resized = false;
                    if custom {
                        resized |= ui
                            .add(egui::DragValue::new(&mut grid.cols).range(1..=MAX_GRID_CELLS))
                            .changed();
                        ui.label("×");
                        resized |= ui
                            .add(egui::DragValue::new(&mut grid.rows).range(1..=MAX_GRID_CELLS))
                            .changed();
                    }
                    if toggled || resized {
                        let grid = custom.then_some(GridSize {
                            cols: grid.cols,
                            rows: grid.rows,
                        });
                        state.error = model.set_display_grid(i, grid).err();
                    }
                });
                if ui.button(i18n.get("delete")).clicked() {
                    delete = Some(i);
                }
                ui.end_row();
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
