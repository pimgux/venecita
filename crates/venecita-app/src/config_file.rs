//! Loading, creating and editing the configuration file.

use std::path::Path;

use venecita_core::config::{Config, ConfigLoadError, Placement, DEFAULT_CONFIG_TOML};
use venecita_core::engine::PinnedRule;

/// Result of reading the configuration file.
#[derive(Debug)]
pub struct Loaded {
    /// The configuration to use (the defaults if the file is invalid).
    pub config: Config,
    /// Why the file couldn't be used, if it couldn't.
    pub error: Option<String>,
    /// `true` if the file didn't exist and the starter file was written.
    pub created: bool,
}

/// Reads the configuration, writing the commented starter file first if there is none.
/// Never fails: problems are reported in [`Loaded::error`] and the defaults are used.
pub fn load_or_create(path: &Path) -> Loaded {
    let mut created = false;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(path, DEFAULT_CONFIG_TOML) {
            Ok(()) => created = true,
            Err(e) => {
                return Loaded {
                    config: Config::default(),
                    error: Some(format!("cannot create {}: {e}", path.display())),
                    created,
                }
            }
        }
    }
    match read(path) {
        Ok(config) => Loaded {
            config,
            error: None,
            created,
        },
        Err(error) => Loaded {
            config: Config::default(),
            error: Some(error),
            created,
        },
    }
}

/// Reads and validates the configuration file.
pub fn read(path: &Path) -> Result<Config, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Config::from_toml_str(&text).map_err(|e: ConfigLoadError| e.to_string())
}

/// Sets `general.launch_at_login` in the file, keeping the user's comments and layout.
pub fn set_launch_at_login(path: &Path, enabled: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    if !doc.contains_table("general") {
        doc["general"] = toml_edit::table();
    }
    doc["general"]["launch_at_login"] = toml_edit::value(enabled);
    std::fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

/// The inline table `{ display = "lg43", col = 0, row = 0, w = 4, h = 4 }`.
fn placement_value(p: &Placement) -> toml_edit::Item {
    let mut t = toml_edit::InlineTable::new();
    t.insert("display", p.display.as_str().into());
    for (k, v) in [("col", p.col), ("row", p.row), ("w", p.w), ("h", p.h)] {
        t.insert(k, i64::from(v).into());
    }
    toml_edit::value(t)
}

/// Writes a rule created or changed by "pin here" into the file, keeping the user's
/// comments and formatting. The file watcher then reloads it.
pub fn save_pinned_rule(path: &Path, pin: &PinnedRule) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;

    let profile = doc
        .get_mut("profiles")
        .and_then(|p| p.as_array_of_tables_mut())
        .and_then(|a| {
            a.iter_mut().find(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(&pin.profile))
            })
        })
        .ok_or_else(|| format!("profile `{}` not found in the file", pin.profile))?;
    let layout = profile
        .get_mut("layouts")
        .and_then(|l| l.as_array_of_tables_mut())
        .and_then(|a| {
            a.iter_mut()
                .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(pin.layout.as_str()))
        })
        .ok_or_else(|| {
            format!(
                "layout `{}` not found in profile `{}`",
                pin.layout, pin.profile
            )
        })?;

    if !layout.contains_key("rules") {
        layout.insert("rules", toml_edit::Item::ArrayOfTables(Default::default()));
    }
    let rules = layout["rules"]
        .as_array_of_tables_mut()
        .ok_or("`rules` is not an array of tables")?;
    match pin.rule_index.and_then(|i| rules.get_mut(i)) {
        Some(rule) => {
            rule["zone"] = placement_value(&pin.placement);
        }
        None => {
            let mut rule = toml_edit::Table::new();
            let mut apps = toml_edit::Array::new();
            apps.push(pin.app_id.as_str());
            rule["apps"] = toml_edit::value(apps);
            rule["zone"] = placement_value(&pin.placement);
            rules.push(rule);
        }
    }
    std::fs::write(path, doc.to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("venecita-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_the_starter_file() {
        let path = temp_dir("create").join("sub/config.toml");
        let loaded = load_or_create(&path);
        assert!(loaded.created);
        assert!(loaded.error.is_none(), "{:?}", loaded.error);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG_TOML);
        // Second time: just read.
        assert!(!load_or_create(&path).created);
    }

    #[test]
    fn invalid_file_falls_back_to_defaults_with_an_error() {
        let path = temp_dir("invalid").join("config.toml");
        std::fs::write(&path, "[general]\ngap = -5\n").unwrap();
        let loaded = load_or_create(&path);
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.error.unwrap().contains("general.gap"));
    }

    fn pin(rule_index: Option<usize>, app: &str, zone: venecita_core::Zone) -> PinnedRule {
        PinnedRule {
            profile: "casa".into(),
            layout: "Notebook abierta".into(),
            rule_index,
            app_id: app.into(),
            placement: Placement::new("lg43", zone),
        }
    }

    const EXAMPLE: &str = include_str!("../../../examples/config.toml");

    #[test]
    fn pinning_updates_an_existing_rule() {
        let path = temp_dir("pin-update").join("config.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        // Rule 0 of "Casa / Notebook abierta" is VS Code.
        save_pinned_rule(
            &path,
            &pin(
                Some(0),
                "com.microsoft.VSCode",
                venecita_core::Zone::new(4, 0, 4, 8),
            ),
        )
        .unwrap();
        let config = read(&path).unwrap();
        let rule = &config.profile("Casa").unwrap().layouts[0].rules[0];
        assert_eq!(
            rule.zone,
            Placement::new("lg43", venecita_core::Zone::new(4, 0, 4, 8))
        );
        assert_eq!(rule.apps[0], "com.microsoft.VSCode", "apps untouched");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# Example configuration with two desk setups"),
            "comments survive"
        );
    }

    #[test]
    fn pinning_appends_a_new_rule() {
        let path = temp_dir("pin-new").join("config.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        save_pinned_rule(
            &path,
            &pin(
                None,
                "com.apple.Terminal",
                venecita_core::Zone::new(4, 6, 4, 2),
            ),
        )
        .unwrap();
        let config = read(&path).unwrap();
        let layout = &config.profile("Casa").unwrap().layouts[0];
        let last = layout.rules.last().unwrap();
        assert_eq!(last.apps, vec!["com.apple.Terminal".to_string()]);
        assert_eq!(
            last.zone,
            Placement::new("lg43", venecita_core::Zone::new(4, 6, 4, 2))
        );
        // The other layouts and profiles are unchanged.
        assert_eq!(config.profile("Casa").unwrap().layouts[1].rules.len(), 2);
    }

    #[test]
    fn pinning_into_a_layout_without_rules() {
        let path = temp_dir("pin-empty").join("config.toml");
        std::fs::write(&path, DEFAULT_CONFIG_TOML).unwrap();
        let p = PinnedRule {
            profile: "Default".into(),
            layout: "Laptop only".into(),
            rule_index: None,
            app_id: "com.apple.Notes".into(),
            placement: Placement::new("laptop", venecita_core::Zone::new(0, 0, 4, 4)),
        };
        save_pinned_rule(&path, &p).unwrap();
        let config = read(&path).unwrap();
        assert_eq!(config.profile("Default").unwrap().layouts[2].rules.len(), 1);
    }

    #[test]
    fn pinning_reports_unknown_profiles() {
        let path = temp_dir("pin-unknown").join("config.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        let mut p = pin(None, "x", venecita_core::Zone::new(0, 0, 1, 1));
        p.profile = "Nope".into();
        assert!(save_pinned_rule(&path, &p).unwrap_err().contains("Nope"));
    }

    #[test]
    fn launch_at_login_edit_keeps_comments() {
        let path = temp_dir("edit").join("config.toml");
        std::fs::write(&path, DEFAULT_CONFIG_TOML).unwrap();
        set_launch_at_login(&path, true).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("launch_at_login = true"));
        assert!(
            text.contains("# Venecita Dock configuration"),
            "comments survive"
        );
        assert!(read(&path).unwrap().general.launch_at_login);
    }
}
