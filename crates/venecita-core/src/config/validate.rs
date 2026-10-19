//! Semantic validation of a parsed [`Config`].
//!
//! Parsing (serde) already guarantees types and rejects unknown keys. This module checks
//! ranges and cross references and collects *all* problems so the user can fix them in one
//! go. Each error carries a path pointing at the offending value.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::SocketAddr;

use super::{Config, Placement};
use crate::action::{Action, Hotkey};
use crate::grid::{GridSize, MAX_GRID_CELLS};

/// A single validation problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// Where the problem is, e.g. `profiles[0].layouts[1].rules[2].zone`.
    pub path: String,
    /// What is wrong, in plain English.
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ConfigError {}

/// Accumulates errors while walking the config.
#[derive(Default)]
struct Errors(Vec<ConfigError>);

impl Errors {
    fn add(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.0.push(ConfigError {
            path: path.into(),
            message: message.into(),
        });
    }

    fn check_range(&mut self, path: &str, value: f64, min: f64, max: f64) {
        if !(min..=max).contains(&value) {
            self.add(
                path,
                format!("must be between {min} and {max}, got {value}"),
            );
        }
    }
}

pub(super) fn validate(config: &Config) -> Vec<ConfigError> {
    let mut e = Errors::default();
    let grid = config.general.grid;

    // --- general / focus / tabs -------------------------------------------------------
    if !grid.is_valid() {
        e.add(
            "general.grid",
            format!(
                "cols and rows must be between 1 and {MAX_GRID_CELLS}, got {}x{}",
                grid.cols, grid.rows
            ),
        );
    }
    e.check_range("general.gap", config.general.gap, 0.0, 200.0);
    e.check_range("focus.size", config.focus.size, 0.3, 1.0);
    e.check_range("focus.dim_opacity", config.focus.dim_opacity, 0.0, 1.0);
    e.check_range("tabs.height", config.tabs.height, 16.0, 64.0);
    e.check_range("dim.opacity", config.dim.opacity, 0.05, 0.9);
    for (i, app) in config.general.ignore.iter().enumerate() {
        if app.trim().is_empty() {
            e.add(format!("general.ignore[{i}]"), "must not be empty");
        }
    }

    // --- api ----------------------------------------------------------------------------
    match config.api.bind.parse::<SocketAddr>() {
        Err(_) => e.add(
            "api.bind",
            format!(
                "`{}` is not a valid address like 127.0.0.1:47800",
                config.api.bind
            ),
        ),
        Ok(addr) if !addr.ip().is_loopback() && !config.api.allow_lan => e.add(
            "api.bind",
            "listening on a non-loopback address requires `allow_lan = true`",
        ),
        Ok(_) => {}
    }

    // --- displays -----------------------------------------------------------------------
    let mut display_ids = HashSet::new();
    for (i, d) in config.displays.iter().enumerate() {
        let path = format!("displays[{i}]");
        if d.id.trim().is_empty() {
            e.add(format!("{path}.id"), "must not be empty");
        } else if !display_ids.insert(d.id.as_str()) {
            e.add(
                format!("{path}.id"),
                format!("duplicate display id `{}`", d.id),
            );
        }
        if let Some(g) = d.grid {
            if !g.is_valid() {
                e.add(
                    format!("{path}.grid"),
                    format!(
                        "cols and rows must be between 1 and {MAX_GRID_CELLS}, got {}x{}",
                        g.cols, g.rows
                    ),
                );
            }
        }
        if d.matcher.is_empty() {
            e.add(
                format!("{path}.match"),
                "needs at least one criterion (builtin, name, vendor, model, serial, width, height)",
            );
        }
    }

    // --- profiles -----------------------------------------------------------------------
    let mut profile_names = HashSet::new();
    for (pi, profile) in config.profiles.iter().enumerate() {
        let ppath = format!("profiles[{pi}]");
        if profile.name.trim().is_empty() {
            e.add(format!("{ppath}.name"), "must not be empty");
        } else if !profile_names.insert(profile.name.to_lowercase()) {
            e.add(
                format!("{ppath}.name"),
                format!("duplicate profile name `{}`", profile.name),
            );
        }
        if profile.layouts.is_empty() {
            e.add(
                format!("{ppath}.layouts"),
                "a profile needs at least one layout",
            );
        }

        for (li, layout) in profile.layouts.iter().enumerate() {
            let lpath = format!("{ppath}.layouts[{li}]");
            if layout.displays.is_empty() {
                e.add(
                    format!("{lpath}.displays"),
                    "a layout needs at least one display",
                );
            }
            let mut seen = HashSet::new();
            for (di, id) in layout.displays.iter().enumerate() {
                if !display_ids.contains(id.as_str()) {
                    e.add(
                        format!("{lpath}.displays[{di}]"),
                        format!("unknown display `{id}` (define it in [[displays]])"),
                    );
                } else if !seen.insert(id.as_str()) {
                    e.add(
                        format!("{lpath}.displays[{di}]"),
                        format!("display `{id}` listed twice"),
                    );
                }
            }
            let layout_displays: HashSet<&str> =
                layout.displays.iter().map(String::as_str).collect();

            if let Some(dz) = &layout.dynamic_zone {
                check_placement(
                    &mut e,
                    &format!("{lpath}.dynamic_zone"),
                    dz,
                    &layout_displays,
                    config.grid_for(&dz.display),
                );
            }
            for (ri, rule) in layout.rules.iter().enumerate() {
                let rpath = format!("{lpath}.rules[{ri}]");
                if rule.apps.iter().all(|a| a.trim().is_empty()) {
                    e.add(format!("{rpath}.apps"), "list at least one application");
                }
                check_placement(
                    &mut e,
                    &format!("{rpath}.zone"),
                    &rule.zone,
                    &layout_displays,
                    config.grid_for(&rule.zone.display),
                );
            }
        }
    }

    // --- hotkeys ------------------------------------------------------------------------
    let mut canonical: HashMap<Hotkey, &str> = HashMap::new();
    for (key, spec) in &config.hotkeys {
        let path = format!("hotkeys.\"{key}\"");
        match key.parse::<Hotkey>() {
            Err(err) => e.add(&path, err.reason),
            Ok(hk) => {
                if let Some(previous) = canonical.insert(hk.clone(), key) {
                    e.add(
                        &path,
                        format!("same shortcut as `{previous}` (both are `{hk}`)"),
                    );
                }
            }
        }
        if let Action::ActivateProfile { name } = &spec.0 {
            if config.profile(name).is_none() {
                e.add(&path, format!("unknown profile `{name}`"));
            }
        }
    }

    e.0
}

fn check_placement(
    e: &mut Errors,
    path: &str,
    placement: &Placement,
    layout_displays: &HashSet<&str>,
    grid: GridSize,
) {
    if !layout_displays.contains(placement.display.as_str()) {
        e.add(
            format!("{path}.display"),
            format!("display `{}` is not part of this layout", placement.display),
        );
    }
    let z = placement.zone();
    if z.w == 0 || z.h == 0 {
        e.add(path, "w and h must be at least 1");
    } else if !z.fits(grid) {
        e.add(
            path,
            format!(
                "zone (col {}, row {}, w {}, h {}) does not fit the {}x{} grid of display `{}`",
                z.col, z.row, z.w, z.h, grid.cols, grid.rows, placement.display
            ),
        );
    }
}
