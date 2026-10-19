use super::*;
use crate::geometry::Direction;

const EXAMPLE: &str = include_str!("../../../../examples/config.toml");

fn errors_of(toml: &str) -> Vec<String> {
    match Config::from_toml_str(toml) {
        Ok(_) => Vec::new(),
        Err(ConfigLoadError::Invalid(errors)) => errors.iter().map(ToString::to_string).collect(),
        Err(other) => panic!("expected validation errors, got {other}"),
    }
}

#[test]
fn empty_file_is_valid_and_uses_defaults() {
    let c = Config::from_toml_str("").unwrap();
    assert_eq!(c, Config::default());
    assert_eq!(c.general.grid, GridSize { cols: 8, rows: 4 });
    assert_eq!(c.api.bind, DEFAULT_API_BIND);
    assert!(!c.hotkeys.is_empty());
}

#[test]
fn default_config_file_is_valid_and_matches_built_in_defaults() {
    let c = Config::from_toml_str(DEFAULT_CONFIG_TOML).unwrap();
    assert_eq!(
        c.hotkeys,
        default_hotkeys(),
        "the starter file lists exactly the default hotkeys"
    );
    assert_eq!(c.general, General::default());
    assert_eq!(c.focus, FocusSettings::default());
    assert_eq!(c.tabs, TabSettings::default());
    assert_eq!(c.api, ApiSettings::default());
}

#[test]
fn example_config_is_valid() {
    let c = Config::from_toml_str(EXAMPLE).unwrap();
    assert_eq!(c.profiles.len(), 3);
    assert_eq!(c.displays.len(), 4);
    let casa = c.profile("casa").unwrap();
    assert_eq!(casa.layouts[0].rules[1].zone.zone(), Zone::new(0, 4, 4, 4));
    assert_eq!(c.grid_for("lg43"), GridSize { cols: 8, rows: 8 });
    assert_eq!(c.grid_for("laptop"), GridSize { cols: 8, rows: 4 });
}

#[test]
fn round_trip_through_toml() {
    let c = Config::from_toml_str(EXAMPLE).unwrap();
    let text = c.to_toml_string().unwrap();
    let back = Config::from_toml_str(&text).unwrap();
    assert_eq!(back, c);
}

#[test]
fn unknown_keys_are_rejected() {
    let err = Config::from_toml_str("[general]\ngapp = 3\n").unwrap_err();
    assert!(matches!(err, ConfigLoadError::Parse(_)));
    assert!(err.to_string().contains("gapp"), "{err}");
}

#[test]
fn parsed_hotkeys() {
    let c = Config::from_toml_str(
        "[hotkeys]\n\"Option+Control+Left\" = { action = \"move_zone\", dir = \"left\" }\n",
    )
    .unwrap();
    let hk = c.parsed_hotkeys();
    assert_eq!(hk.len(), 1);
    assert_eq!(hk[0].0.to_string(), "ctrl+alt+left");
    assert_eq!(
        hk[0].1,
        Action::MoveZone {
            dir: Direction::Left
        }
    );
}

#[test]
fn hotkeys_table_replaces_defaults() {
    let c = Config::from_toml_str("[hotkeys]\n\"ctrl+alt+f\" = \"focus_mode_toggle\"\n").unwrap();
    assert_eq!(c.hotkeys.len(), 1);
}

/// A config with one mistake of every kind: all of them must be reported at once.
#[test]
fn reports_every_error_with_its_path() {
    let bad = r#"
        [general]
        grid = { cols = 0, rows = 4 }
        gap = -1
        ignore = [""]

        [focus]
        size = 1.5
        dim_opacity = 2

        [tabs]
        height = 4

        [api]
        bind = "0.0.0.0:47800"

        [[displays]]
        id = "a"
        match = {}

        [[displays]]
        id = "a"
        match = { builtin = true }

        [[profiles]]
        name = "Home"

          [[profiles.layouts]]
          name = "L"
          displays = ["a", "a", "ghost"]
          dynamic_zone = { display = "b", col = 0, row = 0, w = 1, h = 1 }

            [[profiles.layouts.rules]]
            apps = []
            zone = { display = "a", col = 0, row = 0, w = 0, h = 1 }

        [[profiles]]
        name = "home"

        [hotkeys]
        "ctrl+alt+f" = "focus_mode_toggle"
        "alt+ctrl+F" = "relayout_all"
        "hyper+x" = "pin_here"
        "ctrl+alt+1" = { action = "activate_profile", name = "Nope" }
    "#;
    let errors = errors_of(bad);
    let expected = [
        "general.grid: cols and rows must be between 1 and 32, got 0x4",
        "general.gap: must be between 0 and 200, got -1",
        "focus.size: must be between 0.3 and 1, got 1.5",
        "focus.dim_opacity: must be between 0 and 1, got 2",
        "tabs.height: must be between 16 and 64, got 4",
        "general.ignore[0]: must not be empty",
        "api.bind: listening on a non-loopback address requires `allow_lan = true`",
        "displays[0].match: needs at least one criterion",
        "displays[1].id: duplicate display id `a`",
        "profiles[0].layouts[0].displays[1]: display `a` listed twice",
        "profiles[0].layouts[0].displays[2]: unknown display `ghost`",
        "profiles[0].layouts[0].dynamic_zone.display: display `b` is not part of this layout",
        "profiles[0].layouts[0].rules[0].apps: list at least one application",
        "profiles[0].layouts[0].rules[0].zone: w and h must be at least 1",
        "profiles[1].name: duplicate profile name `home`",
        "profiles[1].layouts: a profile needs at least one layout",
        "hotkeys.\"hyper+x\": unknown modifier `hyper`",
        "hotkeys.\"ctrl+alt+1\": unknown profile `Nope`",
        "same shortcut as",
    ];
    for want in expected {
        assert!(
            errors.iter().any(|e| e.contains(want)),
            "missing error containing `{want}`\nall errors:\n{}",
            errors.join("\n")
        );
    }
}

#[test]
fn zone_outside_grid() {
    let errors = errors_of(
        r#"
        [[displays]]
        id = "lg43"
        match = { name = "LG" }
        [[profiles]]
        name = "P"
        [[profiles.layouts]]
        name = "L"
        displays = ["lg43"]
        [[profiles.layouts.rules]]
        apps = ["x"]
        zone = { display = "lg43", col = 6, row = 0, w = 4, h = 1 }
        "#,
    );
    assert_eq!(
        errors,
        vec!["profiles[0].layouts[0].rules[0].zone: zone (col 6, row 0, w 4, h 1) does not fit the 8x4 grid of display `lg43`"]
    );
}

#[test]
fn lan_bind_allowed_when_opted_in() {
    let c = Config::from_toml_str("[api]\nbind = \"0.0.0.0:47800\"\nallow_lan = true\n");
    assert!(c.is_ok());
    let errors = errors_of("[api]\nbind = \"localhost\"\n");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].starts_with("api.bind"));
}

#[test]
fn per_display_grid() {
    let toml = r#"
        [[displays]]
        id = "lg43"
        match = { name = "LG" }
        grid = { cols = 8, rows = 8 }
        [[displays]]
        id = "laptop"
        match = { builtin = true }
        [[profiles]]
        name = "P"
        [[profiles.layouts]]
        name = "L"
        displays = ["lg43", "laptop"]
        [[profiles.layouts.rules]]
        apps = ["x"]
        zone = { display = "lg43", col = 0, row = 4, w = 4, h = 4 }
        [[profiles.layouts.rules]]
        apps = ["y"]
        zone = { display = "laptop", col = 0, row = 4, w = 4, h = 4 }
    "#;
    let errors = errors_of(toml);
    // Rows 4..8 exist on the 8x8 LG grid but not on the laptop's default 8x4 grid.
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("profiles[0].layouts[0].rules[1].zone"));
    assert!(errors[0].contains("grid of display `laptop`"));

    let errors = errors_of(&toml.replace("cols = 8, rows = 8", "cols = 0, rows = 8"));
    assert!(
        errors.iter().any(|e| e.starts_with("displays[0].grid")),
        "{errors:?}"
    );
}

#[test]
fn dim_section_defaults_and_range() {
    let c = Config::from_toml_str("").unwrap();
    assert!(!c.dim.enabled);
    assert_eq!(c.dim.opacity, 0.3);

    let c = Config::from_toml_str("[dim]\nenabled = true\nopacity = 0.5\n").unwrap();
    assert!(c.dim.enabled);
    assert_eq!(c.dim.opacity, 0.5);

    // Fully black would hide everything else; nearly transparent does nothing.
    for bad in ["0.95", "0.0"] {
        let err = Config::from_toml_str(&format!("[dim]\nopacity = {bad}\n")).unwrap_err();
        assert!(err.to_string().contains("dim.opacity"), "{err}");
    }
}
