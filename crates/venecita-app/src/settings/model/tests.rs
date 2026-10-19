use super::*;

const EXAMPLE: &str = include_str!("../../../../../examples/config.toml");
const DEFAULT: &str = include_str!("../../../../venecita-core/src/config/default_config.toml");

fn model(text: &str) -> SettingsModel {
    SettingsModel::from_text(text, None).expect("valid config")
}

#[test]
fn zone_from_drag_handles_any_corner_order() {
    // Dragging from (1,1) to (3,2) and the reverse both give the same zone.
    assert_eq!(zone_from_drag((1, 1), (3, 2)), Zone::new(1, 1, 3, 2));
    assert_eq!(zone_from_drag((3, 2), (1, 1)), Zone::new(1, 1, 3, 2));
    // A single cell is a 1x1 zone.
    assert_eq!(zone_from_drag((5, 0), (5, 0)), Zone::new(5, 0, 1, 1));
}

#[test]
fn no_change_saves_byte_for_byte() {
    for text in [EXAMPLE, DEFAULT] {
        let m = model(text);
        assert!(!m.is_dirty());
        assert_eq!(m.to_toml_string(), text, "untouched round-trip must match");
    }
}

#[test]
fn editing_a_scalar_keeps_comments() {
    let mut m = model(DEFAULT);
    m.set_gap(16.0);
    assert!(m.is_dirty());
    let out = m.to_toml_string();
    assert!(out.contains("gap = 16"), "value updated: {out}");
    assert!(
        out.contains("# Venecita Dock configuration"),
        "comments survive"
    );
    assert_eq!(m.config().general.gap, 16.0);
}

#[test]
fn whole_numbers_have_no_trailing_decimal() {
    let mut m = model(DEFAULT);
    m.set_gap(12.0);
    assert!(m.to_toml_string().contains("gap = 12"));
    m.set_focus_size(0.75);
    assert!(m.to_toml_string().contains("size = 0.75"));
}

#[test]
fn set_language_and_grid() {
    let mut m = model(DEFAULT);
    m.set_language(Language::Es);
    m.set_grid(GridSize { cols: 12, rows: 6 });
    assert_eq!(m.config().general.language, Language::Es);
    assert_eq!(m.config().general.grid, GridSize { cols: 12, rows: 6 });
    let out = m.to_toml_string();
    assert!(out.contains("language = \"es\""), "{out}");
    assert!(out.contains("cols = 12"));
}

#[test]
fn api_settings_round_trip() {
    let mut m = model(DEFAULT);
    m.set_api_enabled(false);
    m.set_api_bind("0.0.0.0:9000");
    m.set_api_allow_lan(true);
    assert!(!m.config().api.enabled);
    assert_eq!(m.config().api.bind, "0.0.0.0:9000");
    assert!(m.config().api.allow_lan);
}

#[test]
fn invalid_values_surface_as_errors_and_block_save() {
    let mut m = model(DEFAULT);
    m.set_gap(-5.0);
    assert!(!m.is_valid());
    assert!(
        m.errors().iter().any(|e| e.path == "general.gap"),
        "{:?}",
        m.errors()
    );
    // Saving a model with no path and invalid config both fail; the invalid message wins.
    let err = m.save().unwrap_err();
    assert!(err.contains("problem"), "{err}");
    // Fixing it clears the error.
    m.set_gap(8.0);
    assert!(m.is_valid());
}

#[test]
fn add_rename_duplicate_delete_profile() {
    let mut m = model(DEFAULT);
    let before = m.config().profiles.len();

    m.add_profile("Road").unwrap();
    assert_eq!(m.config().profiles.len(), before + 1);
    assert!(m.config().profile("Road").is_some());

    // Duplicate names are rejected (case-insensitive).
    assert!(m.add_profile("road").is_err());

    let idx = m.config().profiles.len() - 1;
    m.rename_profile(idx, "Travel").unwrap();
    assert!(m.config().profile("Travel").is_some());
    assert!(m.config().profile("Road").is_none());

    m.duplicate_profile(idx).unwrap();
    assert_eq!(m.config().profiles.len(), before + 2);
    assert!(m.config().profile("Travel copy").is_some());

    m.delete_profile(idx).unwrap();
    assert!(m.config().profile("Travel").is_none());
    assert!(m.config().profile("Travel copy").is_some());
}

#[test]
fn add_edit_delete_rules() {
    let mut m = model(EXAMPLE);
    // "Casa" is profile 0, "Notebook abierta" is layout 0, starting with 2 rules.
    assert_eq!(m.config().profiles[0].layouts[0].rules.len(), 2);

    let apps = vec!["com.apple.Terminal".to_string()];
    let zone = Placement::new("lg43", Zone::new(4, 0, 4, 4));
    let idx = m.add_rule(0, 0, &apps, &zone).unwrap();
    assert_eq!(idx, 2);
    assert_eq!(m.config().profiles[0].layouts[0].rules.len(), 3);
    assert_eq!(m.config().profiles[0].layouts[0].rules[2].apps, apps);

    // Edit it.
    let apps2 = vec!["com.apple.Terminal".to_string(), "terminal".to_string()];
    let zone2 = Placement::new("lg43", Zone::new(4, 4, 4, 4));
    m.set_rule(0, 0, 2, &apps2, &zone2).unwrap();
    assert_eq!(m.config().profiles[0].layouts[0].rules[2].apps, apps2);
    assert_eq!(m.config().profiles[0].layouts[0].rules[2].zone, zone2);

    // Delete it; the original two remain, untouched.
    m.delete_rule(0, 0, 2).unwrap();
    assert_eq!(m.config().profiles[0].layouts[0].rules.len(), 2);
    // Comments elsewhere in the file still there.
    assert!(m.to_toml_string().contains("# Example configuration"));
}

#[test]
fn a_rule_needs_an_app() {
    let mut m = model(EXAMPLE);
    let zone = Placement::new("lg43", Zone::new(0, 0, 1, 1));
    assert!(m
        .add_rule(0, 0, &[String::new(), "  ".into()], &zone)
        .is_err());
}

#[test]
fn set_and_clear_dynamic_zone() {
    let mut m = model(DEFAULT);
    // "Default" / "External only" (layout 1) already has a dynamic zone; clear it.
    assert!(m.config().profiles[0].layouts[1].dynamic_zone.is_some());
    m.set_dynamic_zone(0, 1, None).unwrap();
    assert!(m.config().profiles[0].layouts[1].dynamic_zone.is_none());

    // Set a new one.
    let p = Placement::new("laptop", Zone::new(0, 0, 8, 4));
    m.set_dynamic_zone(0, 1, Some(&p)).unwrap();
    assert_eq!(
        m.config().profiles[0].layouts[1].dynamic_zone.as_ref(),
        Some(&p)
    );
}

#[test]
fn add_layout_to_profile() {
    let mut m = model(DEFAULT);
    let before = m.config().profiles[0].layouts.len();
    m.add_layout(0, "Both screens", &["laptop".into(), "external".into()])
        .unwrap();
    assert_eq!(m.config().profiles[0].layouts.len(), before + 1);
    let added = m.config().profiles[0].layouts.last().unwrap();
    assert_eq!(added.name, "Both screens");
    assert_eq!(added.displays, vec!["laptop", "external"]);
}

#[test]
fn out_of_range_indices_error() {
    let mut m = model(DEFAULT);
    assert!(m.rename_profile(99, "x").is_err());
    assert!(m.delete_layout(0, 99).is_err());
    assert!(m
        .set_rule(
            0,
            0,
            99,
            &["x".into()],
            &Placement::new("laptop", Zone::new(0, 0, 1, 1))
        )
        .is_err());
}

#[test]
fn save_writes_the_file() {
    let dir = std::env::temp_dir().join(format!("venecita-settings-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("config.toml");
    std::fs::write(&path, DEFAULT).unwrap();

    let mut m = SettingsModel::load(&path).unwrap();
    m.set_gap(20.0);
    assert!(m.is_dirty());
    m.save().unwrap();
    assert!(!m.is_dirty());

    let reread = SettingsModel::load(&path).unwrap();
    assert_eq!(reread.config().general.gap, 20.0);
}

#[test]
fn hotkeys_can_be_rebound() {
    let mut m = model(DEFAULT);
    let mut keys = m.config().hotkeys.clone();
    let spec = keys.remove("ctrl+alt+f").expect("default focus hotkey");
    keys.insert("ctrl+alt+shift+f".into(), spec.clone());
    m.set_hotkeys(&keys).unwrap();
    assert!(m.is_valid(), "{:?}", m.errors());
    assert_eq!(m.config().hotkeys.get("ctrl+alt+shift+f"), Some(&spec));
    assert!(!m.config().hotkeys.contains_key("ctrl+alt+f"));
    // Comments outside the hotkeys table survive.
    assert!(m.to_toml_string().contains("# Venecita Dock configuration"));
}

#[test]
fn invalid_hotkey_is_reported() {
    let mut m = model(DEFAULT);
    let mut keys = m.config().hotkeys.clone();
    let spec = keys.values().next().unwrap().clone();
    keys.insert("ctrl+notakey".into(), spec);
    m.set_hotkeys(&keys).unwrap();
    assert!(
        m.errors().iter().any(|e| e.path.contains("hotkeys")),
        "{:?}",
        m.errors()
    );
}

fn info(name: &str, builtin: bool, serial: Option<u32>) -> DisplayInfo {
    let r = venecita_core::Rect::new(0.0, 0.0, 1920.0, 1080.0);
    DisplayInfo {
        handle: "1".into(),
        name: name.into(),
        builtin,
        vendor: None,
        model: None,
        serial,
        frame: r,
        work_area: r,
        scale: 2.0,
    }
}

#[test]
fn matcher_for_builtin_and_external() {
    let m = matcher_for(&info("Built-in Retina Display", true, None));
    assert_eq!(m.builtin, Some(true));
    assert!(m.name.is_none());

    let m = matcher_for(&info("DELL U2723QE", false, Some(0)));
    assert_eq!(m.name.as_deref(), Some("DELL U2723QE"));
    assert_eq!(m.serial, None, "a zero serial is not a real serial");

    let m = matcher_for(&info("DELL U2723QE", false, Some(1234)));
    assert_eq!(m.serial, Some(1234));
}

#[test]
fn suggested_ids_are_slugs_and_unique() {
    let m = model(DEFAULT); // defines `laptop` and `external`
    let existing = &m.config().displays;
    assert_eq!(
        suggest_display_id(&info("LG HDR 4K", false, None), existing),
        "lg_hdr_4k"
    );
    assert_eq!(
        suggest_display_id(&info("Built-in", true, None), existing),
        "laptop_2"
    );
    assert_eq!(
        suggest_display_id(&info("???", false, None), existing),
        "display"
    );
}

#[test]
fn add_edit_delete_displays() {
    let mut m = model(DEFAULT);
    let before = m.config().displays.len();
    let matcher = matcher_for(&info("LG HDR 4K", false, None));
    let idx = m.add_display("lg", &matcher).unwrap();
    assert_eq!(m.config().displays.len(), before + 1);
    assert_eq!(m.config().displays[idx].matcher, matcher);
    assert!(m.add_display("lg", &matcher).is_err(), "duplicate id");

    m.set_display_grid(idx, Some(GridSize { cols: 8, rows: 8 }))
        .unwrap();
    assert_eq!(m.config().grid_for("lg"), GridSize { cols: 8, rows: 8 });
    m.set_display_grid(idx, None).unwrap();
    assert_eq!(m.config().grid_for("lg"), m.config().general.grid);

    m.delete_display(idx).unwrap();
    assert_eq!(m.config().displays.len(), before);
}

#[test]
fn home_profile_from_scratch_with_only_the_model() {
    // Manual checklist item: "You can create the home profile from scratch with only the GUI."
    let mut m = model("");
    m.add_display("lg43", &matcher_for(&info("LG Monitor", false, None)))
        .unwrap();
    m.add_display("laptop", &matcher_for(&info("Built-in", true, None)))
        .unwrap();
    m.set_display_grid(0, Some(GridSize { cols: 8, rows: 8 }))
        .unwrap();
    m.add_profile("Casa").unwrap();
    m.add_layout(0, "Notebook abierta", &["lg43".into(), "laptop".into()])
        .unwrap();
    m.set_dynamic_zone(0, 0, Some(&Placement::new("laptop", Zone::new(0, 0, 8, 4))))
        .unwrap();
    m.add_rule(
        0,
        0,
        &["com.microsoft.VSCode".into()],
        &Placement::new("lg43", zone_from_drag((0, 0), (3, 3))),
    )
    .unwrap();
    assert!(m.is_valid(), "{:?}", m.errors());

    // The result loads back like a hand-written file.
    let reparsed = Config::from_toml_str(&m.to_toml_string()).unwrap();
    assert_eq!(&reparsed, m.config());
}

#[test]
fn dim_settings_round_trip() {
    let mut m = model(DEFAULT);
    assert!(!m.config().dim.enabled);
    m.set_dim_unfocused(true);
    m.set_dim_unfocused_opacity(0.45);
    assert!(m.config().dim.enabled);
    assert_eq!(m.config().dim.opacity, 0.45);
    assert!(m.is_valid());
    let out = m.to_toml_string();
    assert!(
        out.contains("enabled = true") && out.contains("opacity = 0.45"),
        "{out}"
    );
    assert!(
        out.contains("# darken every window except the focused one"),
        "comments survive"
    );
}

#[test]
fn end_of_line_comments_survive_scalar_edits() {
    let mut m = model(DEFAULT);
    m.set_gap(12.0);
    m.set_language(Language::Es);
    let out = m.to_toml_string();
    let line = |key: &str| out.lines().find(|l| l.starts_with(key)).unwrap();
    let gap = line("gap =");
    assert!(
        gap.starts_with("gap = 12 ") && gap.contains("# points between windows"),
        "{gap}"
    );
    let language = line("language =");
    assert!(
        language.contains("\"es\"") && language.contains("# \"auto\", \"en\" or \"es\""),
        "{language}"
    );
}

#[test]
fn windows_line_endings_are_kept() {
    let crlf = DEFAULT.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut m = model(&crlf);
    assert_eq!(m.to_toml_string(), crlf, "untouched round-trip must match");
    m.set_gap(12.0);
    m.add_profile("Road").unwrap();
    let out = m.to_toml_string();
    assert!(
        !out.replace("\r\n", "").contains('\n'),
        "every line ends in CRLF"
    );
    assert!(out.contains("gap = 12"));
}

const GRID_8X4: GridSize = GridSize { cols: 8, rows: 4 };

#[test]
fn moving_a_zone_keeps_its_size_and_stays_inside() {
    let z = Zone::new(0, 0, 4, 2);
    // Grabbed at its second column, dropped with the pointer on column 5: starts at 4.
    assert_eq!(
        zone_moved(z, (1, 0), (5, 1), GRID_8X4),
        Zone::new(4, 1, 4, 2)
    );
    // Pushed past the right/bottom edges: stops at the edge.
    assert_eq!(
        zone_moved(z, (0, 0), (7, 3), GRID_8X4),
        Zone::new(4, 2, 4, 2)
    );
    // Pushed past the left/top edges.
    assert_eq!(
        zone_moved(z, (3, 1), (0, 0), GRID_8X4),
        Zone::new(0, 0, 4, 2)
    );
    // Onto a smaller grid: the zone shrinks to fit.
    let tiny = GridSize { cols: 2, rows: 2 };
    assert_eq!(zone_moved(z, (0, 0), (1, 1), tiny), Zone::new(0, 0, 2, 2));
}

#[test]
fn resizing_moves_only_the_held_edges() {
    let z = Zone::new(2, 1, 3, 2); // columns 2..=4, rows 1..=2
    let right = Edges {
        right: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, right, (6, 0)), Zone::new(2, 1, 5, 2));
    let left = Edges {
        left: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, left, (0, 3)), Zone::new(0, 1, 5, 2));
    let top_left = Edges {
        left: true,
        top: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, top_left, (3, 2)), Zone::new(3, 2, 2, 1));
    let bottom = Edges {
        bottom: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, bottom, (4, 3)), Zone::new(2, 1, 3, 3));
}

#[test]
fn resizing_past_the_opposite_edge_stops_at_one_cell() {
    let z = Zone::new(2, 1, 3, 2);
    let left = Edges {
        left: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, left, (7, 1)), Zone::new(4, 1, 1, 2));
    let top = Edges {
        top: true,
        ..Default::default()
    };
    assert_eq!(zone_resized(z, top, (2, 3)), Zone::new(2, 2, 3, 1));
}

#[test]
fn hotkeys_are_written_inline_in_one_table() {
    let mut m = model(DEFAULT);
    let keys = m.config().hotkeys.clone();
    m.set_hotkeys(&keys).unwrap();
    let out = m.to_toml_string();
    assert!(
        !out.contains("[hotkeys."),
        "no per-shortcut sections:\n{out}"
    );
    assert!(
        out.contains(r#""ctrl+alt+left" = { action = "move_zone", dir = "left" }"#),
        "{out}"
    );
    assert!(out.contains(r#""ctrl+alt+s" = "snap_to_grid""#), "{out}");
    assert_eq!(m.config().hotkeys, keys);
}

#[test]
fn messy_hotkey_sections_are_tidied() {
    // What an earlier version wrote: one section per shortcut, mixed with the rules.
    let messy = r#"
[[displays]]
id = "laptop"
match = { builtin = true }

[[profiles]]
name = "Casa"

  [[profiles.layouts]]
  name = "Solo"
  displays = ["laptop"]

[hotkeys."ctrl+alt+left"]
action = "move_zone"
dir = "left"

[[profiles.layouts.rules]]
apps = ["com.apple.Terminal"]
zone = { display = "laptop", col = 0, row = 0, w = 4, h = 4 }

[hotkeys]
"ctrl+alt+s" = "snap_to_grid"
"#;
    let mut m = model(messy);
    let keys = m.config().hotkeys.clone();
    assert_eq!(keys.len(), 2);
    m.set_hotkeys(&keys).unwrap();
    let out = m.to_toml_string();
    assert!(!out.contains("[hotkeys."), "{out}");
    let reparsed = Config::from_toml_str(&out).unwrap();
    assert_eq!(reparsed.hotkeys, keys);
    assert_eq!(
        reparsed.profiles[0].layouts[0].rules.len(),
        1,
        "rules untouched"
    );
}
