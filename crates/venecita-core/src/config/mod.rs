//! User configuration (the `config.toml` file).
//!
//! The file is designed to be edited by hand as well as by the settings window, so every
//! section is optional and has sensible defaults: an empty file is a valid configuration.
//! See `docs/CONFIG.md` for the user-facing reference and `examples/config.toml` for a
//! complete example.
//!
//! Loading is a two step process: [`Config::from_toml_str`] parses the TOML (rejecting
//! unknown keys to catch typos) and then runs [`Config::validate`], which reports **every**
//! problem at once with a path such as `profiles[0].layouts[1].rules[2].zone`.

mod validate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::action::{Action, ActionSpec, Hotkey};
use crate::display::DisplayDef;
use crate::grid::{GridSize, Zone};

pub use validate::ConfigError;

/// The commented starter configuration written on first run.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default_config.toml");

/// Root of the configuration file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// General settings.
    #[serde(default)]
    pub general: General,
    /// Extra focus mode.
    #[serde(default)]
    pub focus: FocusSettings,
    /// Tab bar shown on zones holding several windows.
    #[serde(default)]
    pub tabs: TabSettings,
    /// Dimming the windows that don't have the focus.
    #[serde(default)]
    pub dim: DimSettings,
    /// Local API.
    #[serde(default)]
    pub api: ApiSettings,
    /// Known displays.
    #[serde(default)]
    pub displays: Vec<DisplayDef>,
    /// Profiles (one per desk setup).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Keyboard shortcuts: hotkey string → action. If the table is missing, the defaults
    /// from [`default_hotkeys`] are used; if present, it replaces them completely.
    #[serde(default = "default_hotkeys")]
    pub hotkeys: BTreeMap<String, ActionSpec>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            focus: FocusSettings::default(),
            tabs: TabSettings::default(),
            dim: DimSettings::default(),
            api: ApiSettings::default(),
            displays: Vec::new(),
            profiles: Vec::new(),
            hotkeys: default_hotkeys(),
        }
    }
}

/// Error returned by [`Config::from_toml_str`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    /// The file is not valid TOML or doesn't match the schema.
    #[error("config syntax error: {0}")]
    Parse(#[from] toml::de::Error),
    /// The file parsed but contains invalid values.
    #[error("invalid config:\n{}", .0.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n"))]
    Invalid(Vec<ConfigError>),
}

impl Config {
    /// Parses and validates a configuration file.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigLoadError> {
        let config: Config = toml::from_str(s)?;
        let errors = config.validate();
        if errors.is_empty() {
            Ok(config)
        } else {
            Err(ConfigLoadError::Invalid(errors))
        }
    }

    /// Serializes the configuration to TOML (without comments; the settings window uses
    /// `toml_edit` to preserve the user's formatting instead).
    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Checks every value and cross reference. Returns all problems found (empty = valid).
    pub fn validate(&self) -> Vec<ConfigError> {
        validate::validate(self)
    }

    /// The hotkeys parsed into [`Hotkey`]s. Entries that fail to parse are skipped, so call
    /// [`Config::validate`] first to report them.
    pub fn parsed_hotkeys(&self) -> Vec<(Hotkey, Action)> {
        self.hotkeys
            .iter()
            .filter_map(|(k, a)| k.parse().ok().map(|h| (h, a.0.clone())))
            .collect()
    }

    /// The grid used on a display: its own `grid` if set, otherwise `general.grid`.
    pub fn grid_for(&self, display_id: &str) -> GridSize {
        self.displays
            .iter()
            .find(|d| d.id == display_id)
            .and_then(|d| d.grid)
            .unwrap_or(self.general.grid)
    }

    /// Finds a profile by name (case-insensitive).
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// Language of the user interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Follow the operating system's language.
    #[default]
    Auto,
    /// English.
    En,
    /// Spanish.
    Es,
}

/// `[general]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// User interface language.
    pub language: Language,
    /// Grid used on every display.
    pub grid: GridSize,
    /// Space in points between windows and around the edges of the screen.
    pub gap: f64,
    /// Start Venecita Dock when the user logs in.
    pub launch_at_login: bool,
    /// Applications that are never moved (same matching rules as `rules.apps`).
    pub ignore: Vec<String>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            grid: GridSize::default(),
            gap: 8.0,
            launch_at_login: false,
            ignore: Vec::new(),
        }
    }
}

/// `[focus]` section: extra focus mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FocusSettings {
    /// Fraction of the work area the focused window takes (0.3–1.0).
    pub size: f64,
    /// Opacity of the dark layer over the other windows (0.0–1.0).
    pub dim_opacity: f64,
}

impl Default for FocusSettings {
    fn default() -> Self {
        Self {
            size: 0.8,
            dim_opacity: 0.55,
        }
    }
}

/// `[tabs]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TabSettings {
    /// Height of the tab bar in points (16–64).
    pub height: f64,
}

impl Default for TabSettings {
    fn default() -> Self {
        Self { height: 24.0 }
    }
}

/// `[dim]` section: darken every window except the focused one, so it stands out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DimSettings {
    /// Whether the other windows are dimmed.
    pub enabled: bool,
    /// Opacity of the dark layer over the other windows (0.05–0.9).
    pub opacity: f64,
}

impl Default for DimSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            opacity: 0.3,
        }
    }
}

/// Default address of the local API.
pub const DEFAULT_API_BIND: &str = "127.0.0.1:47800";

/// `[api]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiSettings {
    /// Whether the API server runs.
    pub enabled: bool,
    /// Address and port to listen on.
    pub bind: String,
    /// Must be `true` to listen on anything other than a loopback address.
    pub allow_lan: bool,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: DEFAULT_API_BIND.into(),
            allow_lan: false,
        }
    }
}

/// A desk setup (e.g. "Home", "Work").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Unique name.
    pub name: String,
    /// Layout variants, e.g. "Laptop open" and "Laptop closed".
    #[serde(default)]
    pub layouts: Vec<Layout>,
}

/// Where each application goes for a particular set of connected displays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Name shown in the menu.
    pub name: String,
    /// Display ids this layout needs. The layout is used when all of them are connected
    /// (the one needing the most displays wins).
    pub displays: Vec<String>,
    /// Zone for windows without a rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_zone: Option<Placement>,
    /// Application placement rules.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A zone on a specific display.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Display id (from `[[displays]]`).
    pub display: String,
    /// First column (0-based).
    pub col: u8,
    /// First row (0-based).
    pub row: u8,
    /// Width in cells.
    pub w: u8,
    /// Height in cells.
    pub h: u8,
}

impl Placement {
    /// Builds a placement from a display id and a zone.
    pub fn new(display: impl Into<String>, zone: Zone) -> Self {
        Self {
            display: display.into(),
            col: zone.col,
            row: zone.row,
            w: zone.w,
            h: zone.h,
        }
    }

    /// The grid zone of this placement.
    pub fn zone(&self) -> Zone {
        Zone::new(self.col, self.row, self.w, self.h)
    }
}

/// Sends the windows of some applications to a zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// App ids or names (see [`crate::rules`]). Several entries make a rule portable
    /// across operating systems.
    pub apps: Vec<String>,
    /// Where those windows go.
    pub zone: Placement,
}

/// The hotkeys used when the config has no `[hotkeys]` table.
pub fn default_hotkeys() -> BTreeMap<String, ActionSpec> {
    use crate::action::DisplayTarget::{Next, Prev};
    use crate::geometry::Direction::{Down, Left, Right, Up};
    let mut map = BTreeMap::new();
    let mut add = |k: &str, a: Action| {
        map.insert(k.to_string(), ActionSpec(a));
    };
    for (key, dir) in [("left", Left), ("right", Right), ("up", Up), ("down", Down)] {
        add(&format!("ctrl+alt+{key}"), Action::MoveZone { dir });
        add(&format!("ctrl+alt+shift+{key}"), Action::Grow { dir });
        add(&format!("ctrl+alt+cmd+{key}"), Action::Shrink { dir });
    }
    add("ctrl+alt+n", Action::MoveToDisplay { target: Next });
    add("ctrl+alt+b", Action::MoveToDisplay { target: Prev });
    add("ctrl+alt+s", Action::SnapToGrid);
    add("ctrl+alt+p", Action::PinHere);
    add("ctrl+alt+r", Action::RelayoutAll);
    add("ctrl+alt+f", Action::FocusModeToggle);
    add("ctrl+alt+bracketright", Action::StackNext);
    add("ctrl+alt+bracketleft", Action::StackPrev);
    add("ctrl+alt+period", Action::DynamicNext);
    add("ctrl+alt+comma", Action::DynamicPrev);
    add("ctrl+alt+0", Action::NextProfile);
    map
}

#[cfg(test)]
mod tests;
