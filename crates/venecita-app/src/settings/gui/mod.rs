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

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use fluent::FluentArgs;

use super::model::SettingsModel;
use crate::i18n::Lang;
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
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "venecita-dock-settings",
        options,
        Box::new(move |_cc| {
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

const TABS: [(Tab, &str); 5] = [
    (Tab::Layout, "tab-layout"),
    (Tab::Displays, "tab-displays"),
    (Tab::Profiles, "tab-profiles"),
    (Tab::Hotkeys, "tab-hotkeys"),
    (Tab::General, "tab-general"),
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

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (tab, key) in TABS {
                    ui.selectable_value(&mut self.tab, tab, self.i18n.get(key));
                }
            });
            ui.add_space(2.0);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            status_bar(ui, model, &self.i18n, &mut self.toast);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.tab {
                    Tab::Layout => {
                        layout::show(ui, model, &self.i18n, &mut self.layout, &self.displays)
                    }
                    Tab::Displays => displays::show(ui, model, &self.i18n, &mut self.displays),
                    Tab::Profiles => profiles::show(ui, model, &self.i18n, &mut self.profiles),
                    Tab::Hotkeys => hotkeys::show(ui, model, &self.i18n, &mut self.hotkeys),
                    Tab::General => {
                        general::show(ui, model, &self.i18n, &mut self.general, &self.config_path)
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

/// Validation problems, the unsaved-changes marker and the Save button.
fn status_bar(
    ui: &mut egui::Ui,
    model: &mut SettingsModel,
    i18n: &I18n,
    toast: &mut Option<(String, Instant)>,
) {
    ui.add_space(4.0);
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
        let save = ui.add_enabled(
            model.is_dirty() && model.is_valid(),
            egui::Button::new(i18n.get("save")),
        );
        if save.clicked()
            || (ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S))
                && model.is_dirty())
        {
            *toast = Some((save_message(model, i18n), Instant::now()));
        }
        if model.is_dirty() {
            ui.label(egui::RichText::new(i18n.get("unsaved-changes")).weak());
        }
        if let Some((message, _)) = toast {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(message.as_str());
            });
        }
    });
    ui.add_space(2.0);
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
