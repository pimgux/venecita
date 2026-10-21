//! The settings window (egui), run as its own process: `venecita-dock --settings`.
//!
//! The menu-bar app owns the `NSApplication` event loop, and eframe wants to own one too,
//! so instead of embedding egui the tray spawns this window as a separate process. It edits
//! `config.toml` through [`SettingsModel`] and saves it; the running app's file watcher
//! reloads the change. Connected displays are queried directly from the platform layer.

mod displays;
mod general;
mod hotkeys;
mod layout;
mod profiles;
mod theme;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use fluent::FluentArgs;

use super::model::SettingsModel;
use crate::i18n::Lang;
use crate::icons::IconKind;
use crate::intl::I18n;

/// Opens the settings window and blocks until it is closed.
pub fn run(config_path: PathBuf, os_language: Option<String>) -> eframe::Result<()> {
    let model = SettingsModel::load(&config_path);
    let language = model
        .as_ref()
        .map(|m| m.config().general.language)
        .unwrap_or_default();
    let i18n = I18n::new(Lang::resolve(language, os_language.as_deref()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(i18n.get("window-title"))
            .with_inner_size([1060.0, 740.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "venecita-dock-settings",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(SettingsApp {
                model,
                config_path,
                i18n,
                os_language,
                tab: Tab::Layout,
                layout: Default::default(),
                displays: Default::default(),
                profiles: Default::default(),
                hotkeys: Default::default(),
                general: Default::default(),
                icons: Default::default(),
                toast: None,
                confirm_close: false,
                allow_close: false,
            }))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Layout,
    Displays,
    Profiles,
    Hotkeys,
    General,
}

const TABS: [(Tab, &str, IconKind); 5] = [
    (Tab::Layout, "tab-layout", IconKind::Arrange),
    (Tab::Displays, "tab-displays", IconKind::Display),
    (Tab::Profiles, "tab-profiles", IconKind::Profile),
    (Tab::Hotkeys, "tab-hotkeys", IconKind::Keyboard),
    (Tab::General, "tab-general", IconKind::General),
];

struct SettingsApp {
    /// `Err` when the file could not be read or is not valid TOML.
    model: Result<SettingsModel, String>,
    config_path: PathBuf,
    i18n: I18n,
    os_language: Option<String>,
    tab: Tab,
    layout: layout::State,
    displays: displays::State,
    profiles: profiles::State,
    hotkeys: hotkeys::State,
    general: general::State,
    icons: theme::Icons,
    /// A short message shown in the status bar.
    toast: Option<(String, Instant)>,
    /// The user tried to close the window with unsaved changes.
    confirm_close: bool,
    /// The user chose to close anyway.
    allow_close: bool,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let model = match &mut self.model {
            Ok(model) => model,
            Err(error) => {
                show_load_error(ctx, &self.i18n, &self.config_path, error);
                return;
            }
        };
        let language_before = model.config().general.language;

        // Connected displays are needed by both the layout editor and the displays tab.
        self.displays.refresh_if_stale();
        displays::show_identify(ctx, &self.displays);

        // Sidebar navigation, like System Settings.
        let sidebar = egui::Frame::none()
            .fill(theme::sidebar_fill(&ctx.style().visuals))
            .inner_margin(egui::Margin::symmetric(10.0, 14.0));
        egui::SidePanel::left("nav")
            .exact_width(210.0)
            .resizable(false)
            .frame(sidebar)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Venecita Dock").size(17.0).strong());
                });
                ui.add_space(14.0);
                for (tab, key, icon) in TABS {
                    let texture = self.icons.tile(ui.ctx(), icon);
                    if nav_item(ui, &texture, &self.i18n.get(key), self.tab == tab).clicked() {
                        self.tab = tab;
                    }
                }
            });

        let page = egui::Frame::none()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(egui::Margin::symmetric(24.0, 10.0));
        egui::TopBottomPanel::bottom("status")
            .frame(page)
            .show(ctx, |ui| {
                status_bar(ui, model, &self.i18n, &mut self.toast);
            });

        egui::CentralPanel::default()
            .frame(page.inner_margin(egui::Margin {
                left: 28.0,
                right: 28.0,
                top: 18.0,
                bottom: 6.0,
            }))
            .show(ctx, |ui| {
                // The layout editor sizes itself to the window (no page scrolling).
                if self.tab == Tab::Layout {
                    layout::show(
                        ui,
                        model,
                        &self.i18n,
                        &mut self.layout,
                        &self.displays,
                        &mut self.icons,
                    );
                    return;
                }
                // The other pages scroll vertically when they are taller than the window.
                // Never horizontally: that would give them unlimited width.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let icons = &mut self.icons;
                        match self.tab {
                            Tab::Layout => {}
                            Tab::Displays => {
                                displays::show(ui, model, &self.i18n, &mut self.displays, icons)
                            }
                            Tab::Profiles => {
                                profiles::show(ui, model, &self.i18n, &mut self.profiles, icons)
                            }
                            Tab::Hotkeys => hotkeys::show(ui, model, &self.i18n, &mut self.hotkeys),
                            Tab::General => general::show(
                                ui,
                                model,
                                &self.i18n,
                                &mut self.general,
                                &self.config_path,
                                icons,
                            ),
                        }
                    });
            });

        // Switching the language takes effect immediately.
        let language = model.config().general.language;
        if language != language_before {
            self.i18n = I18n::new(Lang::resolve(language, self.os_language.as_deref()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.i18n.get("window-title")));
        }

        // Closing with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && model.is_dirty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.confirm_close {
            confirm_close_dialog(ctx, self);
        }

        if let Some((_, since)) = &self.toast {
            let left = Duration::from_secs(3).saturating_sub(since.elapsed());
            if left.is_zero() {
                self.toast = None;
            } else {
                ctx.request_repaint_after(left);
            }
        }
    }
}

/// One entry of the sidebar: icon and label, highlighted when selected.
fn nav_item(
    ui: &mut egui::Ui,
    icon: &egui::TextureHandle,
    label: &str,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(ui.available_width(), 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.visuals();
    let fill = if selected {
        Some(egui::Color32::from_rgb(10, 132, 255))
    } else if response.hovered() {
        Some(visuals.widgets.hovered.weak_bg_fill)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, theme::RADIUS - 1.0, fill);
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 20.0, rect.center().y),
        egui::vec2(20.0, 20.0),
    );
    egui::Image::new(icon).paint_at(ui, icon_rect);
    let color = if selected {
        egui::Color32::WHITE
    } else {
        visuals.text_color()
    };
    ui.painter().text(
        egui::pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        color,
    );
    ui.add_space(2.0);
    response
}

/// Validation problems, the unsaved-changes marker and the Save button.
fn status_bar(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    toast: &mut Option<(String, Instant)>,
) {
    let problems = model.errors().len() + usize::from(model.parse_error().is_some());
    if problems > 0 {
        let mut args = FluentArgs::new();
        args.set("count", problems);
        egui::CollapsingHeader::new(
            egui::RichText::new(i18n.get_args("problems", &args))
                .color(ui.visuals().error_fg_color),
        )
        .id_salt("problems")
        .show(ui, |ui| {
            if let Some(e) = model.parse_error() {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            for e in model.errors() {
                ui.colored_label(ui.visuals().error_fg_color, e.to_string());
            }
        });
    }
    ui.horizontal(|ui| {
        if let Some((message, _)) = toast {
            ui.label(message.as_str());
        } else if model.is_dirty() {
            ui.label(egui::RichText::new(i18n.get("unsaved-changes")).color(theme::subtle(ui)));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let enabled = model.is_dirty() && model.is_valid();
            let save = ui
                .add_enabled_ui(enabled, |ui| {
                    theme::button(ui, theme::Kind::Primary, None, i18n.get("save"))
                })
                .inner;
            let shortcut = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
            if save.clicked() || (shortcut && enabled) {
                *toast = Some((save_message(model, i18n), Instant::now()));
            }
        });
    });
}

fn save_message(model: &mut SettingsModel, i18n: &I18n) -> String {
    match model.save() {
        Ok(()) => i18n.get("saved"),
        Err(error) => {
            let mut args = FluentArgs::new();
            args.set("error", error);
            i18n.get_args("save-failed", &args)
        }
    }
}

fn confirm_close_dialog(ctx: &egui::Context, app: &mut SettingsApp) {
    let Ok(model) = &mut app.model else { return };
    egui::Window::new(app.i18n.get("unsaved-changes"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(model.is_valid(), egui::Button::new(app.i18n.get("save")))
                    .clicked()
                {
                    let message = save_message(model, &app.i18n);
                    if model.is_dirty() {
                        app.toast = Some((message, Instant::now()));
                    } else {
                        app.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    app.confirm_close = false;
                }
                if ui.button(app.i18n.get("discard")).clicked() {
                    app.allow_close = true;
                    app.confirm_close = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(app.i18n.get("cancel")).clicked() {
                    app.confirm_close = false;
                }
            });
        });
}

fn show_load_error(ctx: &egui::Context, i18n: &I18n, path: &std::path::Path, error: &str) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(i18n.get("load-failed"));
        ui.label(path.display().to_string());
        ui.add_space(8.0);
        ui.colored_label(ui.visuals().error_fg_color, error);
        ui.add_space(8.0);
        if ui.button(i18n.get("open-file")).clicked() {
            crate::system::open_text_file(path);
        }
    });
}

/// A colour per rule index, distinguishable in light and dark mode.
fn rule_color(index: usize) -> egui::Color32 {
    const PALETTE: [egui::Color32; 8] = [
        egui::Color32::from_rgb(0x4e, 0x79, 0xa7),
        egui::Color32::from_rgb(0xf2, 0x8e, 0x2b),
        egui::Color32::from_rgb(0x59, 0xa1, 0x4f),
        egui::Color32::from_rgb(0xb0, 0x7a, 0xa1),
        egui::Color32::from_rgb(0xed, 0xc9, 0x48),
        egui::Color32::from_rgb(0x76, 0xb7, 0xb2),
        egui::Color32::from_rgb(0xe1, 0x57, 0x59),
        egui::Color32::from_rgb(0x9c, 0x75, 0x5f),
    ];
    PALETTE[index % PALETTE.len()]
}
