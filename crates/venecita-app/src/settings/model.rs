//! The settings view-model: edit a [`Config`] without a GUI.
//!
//! [`SettingsModel`] keeps the configuration file as a [`toml_edit::DocumentMut`] — the one
//! source of truth — and derives a parsed, validated [`Config`] from it after every change.
//! Editing the document (instead of re-serialising a `Config`) means the user's comments and
//! formatting survive, and an untouched model saves back byte-for-byte identical.
//!
//! Every mutating method returns to a consistent state: the document is edited, then
//! [`SettingsModel::refresh`] re-parses it so [`SettingsModel::config`] and
//! [`SettingsModel::errors`] always describe the current document. The GUI reads those to
//! draw the UI and the validation messages, and calls [`SettingsModel::save`] to write.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, InlineTable, Item, Table};
use venecita_core::config::{Config, ConfigError, Language, Placement};
use venecita_core::display::{DisplayDef, DisplayInfo, DisplayMatcher};
use venecita_core::grid::{GridSize, Zone};

/// An editable configuration backed by its on-disk TOML document.
#[derive(Debug, Clone)]
pub struct SettingsModel {
    path: Option<PathBuf>,
    doc: DocumentMut,
    config: Config,
    errors: Vec<ConfigError>,
    /// Set when the current document does not even parse into a [`Config`] (should not
    /// happen through the typed setters, which only ever write well-typed values).
    parse_error: Option<String>,
    dirty: bool,
    /// The file uses Windows line endings. `toml_edit` writes `\n`, so they are restored on
    /// output to keep the file as the user (or their editor) wrote it.
    crlf: bool,
}

impl SettingsModel {
    /// Builds a model from the text of a configuration file. `path` is where
    /// [`SettingsModel::save`] writes; pass `None` for an in-memory model (tests).
    pub fn from_text(text: &str, path: Option<PathBuf>) -> Result<Self, String> {
        let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        let config = toml::from_str::<Config>(text).map_err(|e| e.to_string())?;
        let errors = config.validate();
        Ok(Self {
            path,
            doc,
            config,
            errors,
            parse_error: None,
            dirty: false,
            crlf: text.contains("\r\n"),
        })
    }

    /// Reads and parses the configuration file at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::from_text(&text, Some(path.to_path_buf()))
    }

    /// The parsed configuration as currently edited. Reflects every change made so far;
    /// if the document stopped parsing it is the last version that did (see
    /// [`SettingsModel::parse_error`]).
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The validation problems of the current configuration (empty means valid).
    pub fn errors(&self) -> &[ConfigError] {
        &self.errors
    }

    /// A document-level parse error, if the last edit produced TOML that no longer
    /// deserialises. Always `None` when only the typed setters are used.
    pub fn parse_error(&self) -> Option<&str> {
        self.parse_error.as_deref()
    }

    /// `true` if the configuration has no validation problems and still parses.
    pub fn is_valid(&self) -> bool {
        self.parse_error.is_none() && self.errors.is_empty()
    }

    /// `true` if anything has been edited since loading or the last save.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The configuration as it would be written to disk, comments and all.
    pub fn to_toml_string(&self) -> String {
        let text = self.doc.to_string();
        if self.crlf {
            text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            text
        }
    }

    /// Writes the configuration back to its file, preserving comments and formatting.
    /// Fails if the model has no path or the configuration is invalid.
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(err) = &self.parse_error {
            return Err(err.clone());
        }
        if !self.errors.is_empty() {
            return Err(format!(
                "configuration has {} problem(s); fix them before saving",
                self.errors.len()
            ));
        }
        let path = self
            .path
            .as_ref()
            .ok_or("this configuration has no file to save to")?;
        std::fs::write(path, self.to_toml_string()).map_err(|e| format!("{e}"))?;
        self.dirty = false;
        Ok(())
    }

    // --- General -----------------------------------------------------------------------

    /// Sets the interface language.
    pub fn set_language(&mut self, lang: Language) {
        self.set_general_scalar("language", language_str(lang));
    }

    /// Sets the global grid size.
    pub fn set_grid(&mut self, grid: GridSize) {
        self.set_general_scalar("grid", grid_value(grid));
    }

    /// Sets the gap between windows, in points.
    pub fn set_gap(&mut self, gap: f64) {
        self.set_general_scalar("gap", number_value(gap));
    }

    /// Enables or disables launch at login.
    pub fn set_launch_at_login(&mut self, enabled: bool) {
        self.set_general_scalar("launch_at_login", enabled);
    }

    /// Replaces the list of ignored applications.
    pub fn set_ignore(&mut self, apps: &[String]) {
        self.set_general_scalar("ignore", string_array(apps));
    }

    // --- Focus -------------------------------------------------------------------------

    /// Sets the fraction of the work area the focused window takes (0.3–1.0).
    pub fn set_focus_size(&mut self, size: f64) {
        self.set_section_scalar("focus", "size", number_value(size));
    }

    /// Sets the opacity of the dim layer behind the focused window (0.0–1.0).
    pub fn set_dim_opacity(&mut self, opacity: f64) {
        self.set_section_scalar("focus", "dim_opacity", number_value(opacity));
    }

    // --- Dimming -----------------------------------------------------------------------

    /// Enables or disables dimming every window except the focused one.
    pub fn set_dim_unfocused(&mut self, enabled: bool) {
        self.set_section_scalar("dim", "enabled", enabled);
    }

    /// Sets how dark the unfocused windows get (0.05–0.9).
    pub fn set_dim_unfocused_opacity(&mut self, opacity: f64) {
        self.set_section_scalar("dim", "opacity", number_value(opacity));
    }

    // --- API ---------------------------------------------------------------------------

    /// Enables or disables the local API server.
    pub fn set_api_enabled(&mut self, enabled: bool) {
        self.set_section_scalar("api", "enabled", enabled);
    }

    /// Sets the address the API listens on, e.g. `"127.0.0.1:47800"`.
    pub fn set_api_bind(&mut self, bind: &str) {
        self.set_section_scalar("api", "bind", bind);
    }

    /// Allows (or forbids) the API to listen on non-loopback addresses.
    pub fn set_api_allow_lan(&mut self, allow: bool) {
        self.set_section_scalar("api", "allow_lan", allow);
    }

    // --- Displays ----------------------------------------------------------------------

    /// Appends a display definition and returns its index. Fails if the id is empty or
    /// already used.
    pub fn add_display(&mut self, id: &str, matcher: &DisplayMatcher) -> Result<usize, String> {
        let id = id.trim();
        if id.is_empty() {
            return Err("the display needs an id".into());
        }
        if self.config.displays.iter().any(|d| d.id == id) {
            return Err(format!("a display with id `{id}` already exists"));
        }
        let mut table = Table::new();
        table["id"] = toml_edit::value(id);
        table["match"] = matcher_value(matcher);
        let displays = self.root_array("displays");
        displays.push(table);
        let index = displays.len() - 1;
        self.refresh();
        Ok(index)
    }

    /// Sets (or clears, with `None`) the grid override of a display.
    pub fn set_display_grid(&mut self, idx: usize, grid: Option<GridSize>) -> Result<(), String> {
        let display = self
            .root_array("displays")
            .get_mut(idx)
            .ok_or_else(|| format!("no display at index {idx}"))?;
        match grid {
            Some(g) => display["grid"] = grid_value(g),
            None => {
                display.remove("grid");
            }
        }
        self.refresh();
        Ok(())
    }

    /// Deletes a display definition. Layouts that still reference it become invalid and
    /// are reported by validation.
    pub fn delete_display(&mut self, idx: usize) -> Result<(), String> {
        let displays = self.root_array("displays");
        if idx >= displays.len() {
            return Err(format!("no display at index {idx}"));
        }
        displays.remove(idx);
        self.refresh();
        Ok(())
    }

    // --- Hotkeys -----------------------------------------------------------------------

    /// Replaces the whole `[hotkeys]` table. The GUI edits a working copy of
    /// [`Config::hotkeys`] and writes it back here; validation then reports any shortcut
    /// that fails to parse or any unknown action.
    pub fn set_hotkeys(
        &mut self,
        hotkeys: &std::collections::BTreeMap<String, venecita_core::action::ActionSpec>,
    ) -> Result<(), String> {
        #[derive(serde::Serialize)]
        struct Wrap<'a> {
            hotkeys: &'a std::collections::BTreeMap<String, venecita_core::action::ActionSpec>,
        }
        let text = toml::to_string(&Wrap { hotkeys }).map_err(|e| e.to_string())?;
        let parsed: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        self.doc["hotkeys"] = parsed["hotkeys"].clone();
        self.refresh();
        Ok(())
    }

    // --- Profiles ----------------------------------------------------------------------

    /// Appends a new, empty profile. Fails if the name is empty or already used.
    pub fn add_profile(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("the profile needs a name".into());
        }
        if self.profile_index(name).is_some() {
            return Err(format!("a profile named `{name}` already exists"));
        }
        let mut table = Table::new();
        table["name"] = toml_edit::value(name);
        self.profiles_array().push(table);
        self.refresh();
        Ok(())
    }

    /// Renames the profile at `idx`. Fails if the new name clashes with another profile.
    pub fn rename_profile(&mut self, idx: usize, new_name: &str) -> Result<(), String> {
        let new_name = new_name.trim().to_string();
        if new_name.is_empty() {
            return Err("the profile needs a name".into());
        }
        if self
            .profile_index(&new_name)
            .is_some_and(|existing| existing != idx)
        {
            return Err(format!("a profile named `{new_name}` already exists"));
        }
        self.profile_table_mut(idx)?["name"] = toml_edit::value(new_name);
        self.refresh();
        Ok(())
    }

    /// Copies the profile at `idx`, appending it with a fresh name.
    pub fn duplicate_profile(&mut self, idx: usize) -> Result<(), String> {
        let mut copy = self.profile_table_mut(idx)?.clone();
        let base = copy
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("profile")
            .to_string();
        let name = self.unique_profile_name(&base);
        copy["name"] = toml_edit::value(name);
        self.profiles_array().push(copy);
        self.refresh();
        Ok(())
    }

    /// Deletes the profile at `idx`.
    pub fn delete_profile(&mut self, idx: usize) -> Result<(), String> {
        let profiles = self.profiles_array();
        if idx >= profiles.len() {
            return Err(format!("no profile at index {idx}"));
        }
        profiles.remove(idx);
        self.refresh();
        Ok(())
    }

    // --- Layouts -----------------------------------------------------------------------

    /// Appends a layout to the profile at `profile_idx`.
    pub fn add_layout(
        &mut self,
        profile_idx: usize,
        name: &str,
        displays: &[String],
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("the layout needs a name".into());
        }
        let mut table = Table::new();
        table["name"] = toml_edit::value(name);
        table["displays"] = string_array(displays);
        self.layouts_array(profile_idx)?.push(table);
        self.refresh();
        Ok(())
    }

    /// Renames a layout.
    pub fn rename_layout(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        new_name: &str,
    ) -> Result<(), String> {
        let new_name = new_name.trim().to_string();
        if new_name.is_empty() {
            return Err("the layout needs a name".into());
        }
        self.layout_table_mut(profile_idx, layout_idx)?["name"] = toml_edit::value(new_name);
        self.refresh();
        Ok(())
    }

    /// Sets the display ids a layout requires.
    pub fn set_layout_displays(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        displays: &[String],
    ) -> Result<(), String> {
        self.layout_table_mut(profile_idx, layout_idx)?["displays"] = string_array(displays);
        self.refresh();
        Ok(())
    }

    /// Deletes a layout.
    pub fn delete_layout(&mut self, profile_idx: usize, layout_idx: usize) -> Result<(), String> {
        let layouts = self.layouts_array(profile_idx)?;
        if layout_idx >= layouts.len() {
            return Err(format!("no layout at index {layout_idx}"));
        }
        layouts.remove(layout_idx);
        self.refresh();
        Ok(())
    }

    /// Sets (or clears, with `None`) the dynamic zone of a layout.
    pub fn set_dynamic_zone(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        placement: Option<&Placement>,
    ) -> Result<(), String> {
        let layout = self.layout_table_mut(profile_idx, layout_idx)?;
        match placement {
            Some(p) => layout["dynamic_zone"] = placement_value(p),
            None => {
                layout.remove("dynamic_zone");
            }
        }
        self.refresh();
        Ok(())
    }

    // --- Rules -------------------------------------------------------------------------

    /// Adds a rule to a layout and returns its index.
    pub fn add_rule(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        apps: &[String],
        placement: &Placement,
    ) -> Result<usize, String> {
        if apps.iter().all(|a| a.trim().is_empty()) {
            return Err("a rule needs at least one application".into());
        }
        let rules = self.rules_array(profile_idx, layout_idx)?;
        let mut rule = Table::new();
        rule["apps"] = string_array(apps);
        rule["zone"] = placement_value(placement);
        rules.push(rule);
        let index = rules.len() - 1;
        self.refresh();
        Ok(index)
    }

    /// Replaces the apps and zone of an existing rule.
    pub fn set_rule(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        rule_idx: usize,
        apps: &[String],
        placement: &Placement,
    ) -> Result<(), String> {
        if apps.iter().all(|a| a.trim().is_empty()) {
            return Err("a rule needs at least one application".into());
        }
        let rules = self.rules_array(profile_idx, layout_idx)?;
        let rule = rules
            .get_mut(rule_idx)
            .ok_or_else(|| format!("no rule at index {rule_idx}"))?;
        rule["apps"] = string_array(apps);
        rule["zone"] = placement_value(placement);
        self.refresh();
        Ok(())
    }

    /// Deletes a rule.
    pub fn delete_rule(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
        rule_idx: usize,
    ) -> Result<(), String> {
        let rules = self.rules_array(profile_idx, layout_idx)?;
        if rule_idx >= rules.len() {
            return Err(format!("no rule at index {rule_idx}"));
        }
        rules.remove(rule_idx);
        self.refresh();
        Ok(())
    }

    // --- Internals ---------------------------------------------------------------------

    /// Re-parses the document into [`Self::config`] and refreshes the error list.
    fn refresh(&mut self) {
        self.dirty = true;
        let text = self.doc.to_string();
        match toml::from_str::<Config>(&text) {
            Ok(config) => {
                self.errors = config.validate();
                self.config = config;
                self.parse_error = None;
            }
            // Should be unreachable via the typed setters; keep the last good config.
            Err(e) => self.parse_error = Some(e.to_string()),
        }
    }

    fn set_general_scalar(&mut self, key: &str, value: impl Into<Item>) {
        self.set_section_scalar("general", key, value);
    }

    fn set_section_scalar(&mut self, section: &str, key: &str, value: impl Into<Item>) {
        let table = table_mut(&mut self.doc, section);
        let mut item: Item = value.into();
        // Keep the old value's surroundings: the alignment and the end-of-line comment
        // (`gap = 8   # points between windows…`) belong to the value's decor.
        if let (Some(old), Some(new)) =
            (table.get(key).and_then(Item::as_value), item.as_value_mut())
        {
            *new.decor_mut() = old.decor().clone();
        }
        table[key] = item;
        self.refresh();
    }

    /// A top-level array of tables (`[[profiles]]`, `[[displays]]`), created if missing.
    fn root_array(&mut self, name: &str) -> &mut toml_edit::ArrayOfTables {
        if !self.doc.contains_key(name) {
            self.doc[name] = Item::ArrayOfTables(Default::default());
        }
        self.doc[name]
            .as_array_of_tables_mut()
            .expect("serde accepted it, so it is an array of tables")
    }

    fn profiles_array(&mut self) -> &mut toml_edit::ArrayOfTables {
        self.root_array("profiles")
    }

    fn profile_table_mut(&mut self, idx: usize) -> Result<&mut Table, String> {
        self.profiles_array()
            .get_mut(idx)
            .ok_or_else(|| format!("no profile at index {idx}"))
    }

    fn layouts_array(
        &mut self,
        profile_idx: usize,
    ) -> Result<&mut toml_edit::ArrayOfTables, String> {
        let profile = self.profile_table_mut(profile_idx)?;
        if !profile.contains_key("layouts") {
            profile.insert("layouts", Item::ArrayOfTables(Default::default()));
        }
        profile["layouts"]
            .as_array_of_tables_mut()
            .ok_or_else(|| "`layouts` is not an array of tables".to_string())
    }

    fn layout_table_mut(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
    ) -> Result<&mut Table, String> {
        self.layouts_array(profile_idx)?
            .get_mut(layout_idx)
            .ok_or_else(|| format!("no layout at index {layout_idx}"))
    }

    fn rules_array(
        &mut self,
        profile_idx: usize,
        layout_idx: usize,
    ) -> Result<&mut toml_edit::ArrayOfTables, String> {
        let layout = self.layout_table_mut(profile_idx, layout_idx)?;
        if !layout.contains_key("rules") {
            layout.insert("rules", Item::ArrayOfTables(Default::default()));
        }
        layout["rules"]
            .as_array_of_tables_mut()
            .ok_or_else(|| "`rules` is not an array of tables".to_string())
    }

    fn profile_index(&self, name: &str) -> Option<usize> {
        self.config
            .profiles
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
    }

    fn unique_profile_name(&self, base: &str) -> String {
        let candidate = format!("{base} copy");
        if self.profile_index(&candidate).is_none() {
            return candidate;
        }
        (2..)
            .map(|n| format!("{base} copy {n}"))
            .find(|name| self.profile_index(name).is_none())
            .expect("an unused name exists")
    }
}

/// Builds a [`Zone`] from the two corner cells of a drag (inclusive), in any order.
///
/// `(from_col, from_row)` and `(to_col, to_row)` are the cells the pointer started and
/// ended on; the result is the smallest zone covering both.
pub fn zone_from_drag(from: (u8, u8), to: (u8, u8)) -> Zone {
    let col = from.0.min(to.0);
    let row = from.1.min(to.1);
    let w = from.0.abs_diff(to.0) + 1;
    let h = from.1.abs_diff(to.1) + 1;
    Zone::new(col, row, w, h)
}

/// How to recognise a connected display in the configuration: the built-in panel by
/// `builtin = true`, an external one by its name (plus its serial when the monitor reports
/// a real one, which tells identical models apart).
pub fn matcher_for(info: &DisplayInfo) -> DisplayMatcher {
    if info.builtin {
        DisplayMatcher {
            builtin: Some(true),
            ..Default::default()
        }
    } else {
        DisplayMatcher {
            name: Some(info.name.clone()),
            serial: info.serial.filter(|&s| s != 0),
            ..Default::default()
        }
    }
}

/// A short, unused display id for a connected display: `laptop` for the built-in panel,
/// otherwise a slug of its name (`"LG HDR 4K"` → `lg_hdr_4k`), with `_2`, `_3`… appended
/// if needed.
pub fn suggest_display_id(info: &DisplayInfo, existing: &[DisplayDef]) -> String {
    let base = if info.builtin {
        "laptop".to_string()
    } else {
        let slug: String = info
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let slug = slug
            .split('_')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        if slug.is_empty() {
            "display".to_string()
        } else {
            slug
        }
    };
    let taken = |id: &str| existing.iter().any(|d| d.id == id);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|id| !taken(id))
        .expect("an unused id exists")
}

fn matcher_value(m: &DisplayMatcher) -> Item {
    let mut t = InlineTable::new();
    if let Some(b) = m.builtin {
        t.insert("builtin", b.into());
    }
    if let Some(n) = &m.name {
        t.insert("name", n.as_str().into());
    }
    for (key, value) in [
        ("vendor", m.vendor),
        ("model", m.model),
        ("serial", m.serial),
        ("width", m.width),
        ("height", m.height),
    ] {
        if let Some(v) = value {
            t.insert(key, i64::from(v).into());
        }
    }
    toml_edit::value(t)
}

fn table_mut<'a>(doc: &'a mut DocumentMut, name: &str) -> &'a mut Table {
    if !doc.contains_table(name) {
        doc[name] = toml_edit::table();
    }
    doc[name].as_table_mut().expect("just ensured a table")
}

fn language_str(lang: Language) -> &'static str {
    match lang {
        Language::Auto => "auto",
        Language::En => "en",
        Language::Es => "es",
    }
}

fn number_value(v: f64) -> Item {
    // Write whole numbers without a trailing `.0`, matching the hand-written style
    // (`gap = 8`) while keeping fractions exact (`dim_opacity = 0.55`).
    if v.fract() == 0.0 && v.abs() < i64::MAX as f64 {
        toml_edit::value(v as i64)
    } else {
        toml_edit::value(v)
    }
}

fn grid_value(grid: GridSize) -> Item {
    let mut t = InlineTable::new();
    t.insert("cols", i64::from(grid.cols).into());
    t.insert("rows", i64::from(grid.rows).into());
    toml_edit::value(t)
}

fn placement_value(p: &Placement) -> Item {
    let mut t = InlineTable::new();
    t.insert("display", p.display.as_str().into());
    for (k, v) in [("col", p.col), ("row", p.row), ("w", p.w), ("h", p.h)] {
        t.insert(k, i64::from(v).into());
    }
    toml_edit::value(t)
}

fn string_array(items: &[String]) -> Item {
    let mut array = toml_edit::Array::new();
    for item in items {
        array.push(item.as_str());
    }
    toml_edit::value(array)
}

#[cfg(test)]
mod tests;
