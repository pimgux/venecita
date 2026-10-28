//! The "Updates" card of the General page: this version, the latest published one and a
//! button to update. The work (asking GitHub, downloading, verifying) runs on a thread
//! of its own (`crate::update::net`); this file only shows its state.
//!
//! - Windows: downloads the setup .exe, verifies its SHA-256 and runs it with `/SILENT`;
//!   the installer closes the tray app, updates it and starts it again, and this window
//!   closes itself.
//! - macOS: downloads the DMG, verifies it and opens it; the user drags the app.

use std::sync::{Arc, Mutex, MutexGuard};

use eframe::egui;
use fluent::FluentArgs;

use super::theme::{self, Kind};
use crate::intl::I18n;
use crate::update::{net, select_package, Os, Package, Release, UpdateError, Version};

/// What the card shows; shared with the worker threads.
#[derive(Default)]
pub(super) struct State {
    shared: Arc<Mutex<Shared>>,
    started: bool,
}

#[derive(Default)]
struct Shared {
    check: Check,
    install: Install,
}

#[derive(Default)]
enum Check {
    #[default]
    NotYet,
    Checking,
    Done(Result<Release, UpdateError>),
}

#[derive(Default)]
enum Install {
    #[default]
    Idle,
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    Failed(UpdateError),
    /// macOS: the DMG is open in Finder.
    Opened,
    /// Windows: the installer is running; this window should close.
    Launched,
}

impl Install {
    fn busy(&self) -> bool {
        matches!(self, Install::Downloading { .. } | Install::Launched)
    }
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl State {
    /// Checks for updates the first time it is called (once per opening of the window).
    pub(super) fn check_once(&mut self, ctx: &egui::Context) {
        if !self.started {
            self.started = true;
            self.check(ctx);
        }
    }

    fn check(&self, ctx: &egui::Context) {
        {
            let mut shared = lock(&self.shared);
            if matches!(shared.check, Check::Checking) || shared.install.busy() {
                return;
            }
            shared.check = Check::Checking;
            shared.install = Install::Idle;
        }
        let shared = Arc::clone(&self.shared);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let result = net::fetch_latest();
                lock(&shared).check = Check::Done(result);
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            lock(&self.shared).check = Check::Done(Err(UpdateError::Network(e.to_string())));
        }
    }

    fn install(&self, ctx: &egui::Context, package: Package) {
        lock(&self.shared).install = Install::Downloading {
            done: 0,
            total: package.file.size,
        };
        let shared = Arc::clone(&self.shared);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("update-download".into())
            .spawn(move || {
                let mut last_shown = 0u64;
                let mut progress = |done: u64, total: Option<u64>| {
                    // Repaint every 256 KB, not for every chunk.
                    if done == 0 || done - last_shown >= 256 * 1024 || Some(done) == total {
                        last_shown = done;
                        lock(&shared).install = Install::Downloading { done, total };
                        ctx.request_repaint();
                    }
                };
                let result = net::download(&package, &mut progress).and_then(|path| {
                    net::install(&path)?;
                    Ok(())
                });
                lock(&shared).install = match result {
                    Ok(()) if cfg!(windows) => Install::Launched,
                    Ok(()) => Install::Opened,
                    Err(e) => {
                        tracing::warn!(error = %e, "update failed");
                        Install::Failed(e)
                    }
                };
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            lock(&self.shared).install = Install::Failed(UpdateError::File(e.to_string()));
        }
    }

    /// A newer version was found (for a mark in the sidebar).
    pub(super) fn update_available(&self) -> bool {
        matches!(&lock(&self.shared).check, Check::Done(Ok(r)) if r.is_newer_than(Version::current()))
    }

    /// The Windows installer is running: the settings window should close now.
    pub(super) fn should_close(&self) -> bool {
        matches!(lock(&self.shared).install, Install::Launched)
    }
}

/// The card. `unsaved` disables updating, which closes the window on Windows.
pub(super) fn show(ui: &mut egui::Ui, i18n: &I18n, state: &mut State, unsaved: bool) {
    // Copy what's needed so the lock isn't held while drawing (or while a click starts a
    // thread that takes it).
    let (checking, latest, check_error, install) = {
        let shared = lock(&state.shared);
        let (checking, latest, error) = match &shared.check {
            Check::NotYet | Check::Checking => (true, None, None),
            Check::Done(Ok(release)) => (false, Some(release.clone()), None),
            Check::Done(Err(e)) => (false, None, Some(e.clone())),
        };
        let install = match &shared.install {
            Install::Idle => Install::Idle,
            Install::Downloading { done, total } => Install::Downloading {
                done: *done,
                total: *total,
            },
            Install::Failed(e) => Install::Failed(e.clone()),
            Install::Opened => Install::Opened,
            Install::Launched => Install::Launched,
        };
        (checking, latest, error, install)
    };
    let current = Version::current();
    let newer = latest.as_ref().filter(|r| r.is_newer_than(current));
    let package = newer.and_then(|r| select_package(r, Os::current()));

    theme::card(ui, Some(&i18n.get("update-title")), |ui| {
        theme::row(ui, &i18n.get("update-current"), None, |ui| {
            ui.monospace(current.to_string());
        });
        theme::divider(ui);
        theme::row(ui, &i18n.get("update-latest"), None, |ui| {
            if checking {
                ui.spinner();
            } else if let Some(release) = &latest {
                ui.monospace(release.version.to_string());
            } else {
                ui.label(egui::RichText::new("—").color(theme::subtle(ui)));
            }
        });
        theme::divider(ui);

        // A sentence about where things stand, and the actions.
        let status = if checking {
            i18n.get("update-checking")
        } else if let Some(e) = &check_error {
            format!(
                "{} {}",
                i18n.get("update-check-failed"),
                error_text(i18n, e)
            )
        } else if let Some(release) = newer {
            let mut args = FluentArgs::new();
            args.set("version", release.version.to_string());
            i18n.get_args("update-available", &args)
        } else {
            i18n.get("update-up-to-date")
        };
        theme::row(ui, &status, None, |ui| {
            let busy = install.busy();
            if let Some(release) = newer {
                match &package {
                    Some(package) => {
                        let mut args = FluentArgs::new();
                        args.set("version", release.version.to_string());
                        let enabled = !busy && !unsaved;
                        let mut response = ui
                            .add_enabled_ui(enabled, |ui| {
                                theme::button(
                                    ui,
                                    Kind::Primary,
                                    None,
                                    i18n.get_args("update-install", &args),
                                )
                            })
                            .inner;
                        if unsaved {
                            response =
                                response.on_disabled_hover_text(i18n.get("update-save-first"));
                        }
                        if response.clicked() {
                            tracing::info!(tag = %release.tag, "update requested");
                            state.install(ui.ctx(), package.clone());
                        }
                        if theme::button(ui, Kind::Secondary, None, i18n.get("update-notes"))
                            .clicked()
                        {
                            net::open_page(release.safe_page_url());
                        }
                    }
                    None => {
                        if theme::button(ui, Kind::Primary, None, i18n.get("update-open-page"))
                            .clicked()
                        {
                            net::open_page(release.safe_page_url());
                        }
                    }
                }
            }
            let check = ui
                .add_enabled_ui(!checking && !busy, |ui| {
                    theme::button(ui, Kind::Secondary, None, i18n.get("update-check"))
                })
                .inner;
            if check.clicked() {
                state.check(ui.ctx());
            }
        });

        // What the update itself is doing.
        let note = |ui: &mut egui::Ui, text: String| {
            ui.add(
                egui::Label::new(egui::RichText::new(text).small().color(theme::subtle(ui))).wrap(),
            );
        };
        if newer.is_some() && package.is_none() && Os::current() != Os::Other {
            note(ui, i18n.get("update-no-package"));
        }
        if newer.is_some() && package.is_some() && unsaved && !install.busy() {
            note(ui, i18n.get("update-save-first"));
        }
        match &install {
            Install::Idle => {}
            Install::Downloading { done, total } => {
                let mut args = FluentArgs::new();
                args.set("done", megabytes(*done));
                let (fraction, text) = match total {
                    Some(total) if *total > 0 => {
                        args.set("total", megabytes(*total));
                        (
                            Some(*done as f32 / *total as f32),
                            i18n.get_args("update-downloading", &args),
                        )
                    }
                    _ => (None, i18n.get_args("update-downloading-unknown", &args)),
                };
                let bar = match fraction {
                    Some(f) => egui::ProgressBar::new(f.clamp(0.0, 1.0)),
                    None => egui::ProgressBar::new(0.0).animate(true),
                };
                ui.add(bar.text(text));
            }
            Install::Failed(e) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(error_text(i18n, e)).color(ui.visuals().error_fg_color),
                    )
                    .wrap(),
                );
            }
            Install::Opened => note(ui, i18n.get("update-dmg-opened")),
            Install::Launched => note(ui, i18n.get("update-launched")),
        }
    });
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// An error in plain words.
fn error_text(i18n: &I18n, error: &UpdateError) -> String {
    let mut args = FluentArgs::new();
    let key = match error {
        UpdateError::Network(detail) => {
            args.set("detail", detail.clone());
            "update-error-network"
        }
        UpdateError::RateLimited => "update-error-rate-limited",
        UpdateError::Http(code) => {
            args.set("code", code.to_string());
            "update-error-http"
        }
        UpdateError::BadResponse => "update-error-response",
        UpdateError::UrlNotAllowed(_) => "update-error-url",
        UpdateError::HashMismatch => "update-error-hash",
        UpdateError::TooLarge => "update-error-too-large",
        UpdateError::File(detail) => {
            args.set("detail", detail.clone());
            "update-error-file"
        }
        UpdateError::Launch(detail) => {
            args.set("detail", detail.clone());
            "update-error-launch"
        }
    };
    i18n.get_args(key, &args)
}
