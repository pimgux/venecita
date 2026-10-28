//! End-to-end placement scenarios: engine + driver + fake platform, using the example
//! configuration (`examples/config.toml`) and the author's real desk setups.

use std::sync::{Arc, Mutex};

use venecita_app::driver::{Driver, UiRequest};
use venecita_core::grid::{zone_rect, Zone};
use venecita_core::{Action, Config, Rect};
use venecita_platform::fake::{setups, FakeCommand, FakePlatform};
use venecita_platform::{AppIdentity, DisplayInfo, Platform, PlatformEvent, WindowId, WindowKind};

const EXAMPLE: &str = include_str!("../../../examples/config.toml");

/// A driver wired to a fake platform, with the fake's events queued until `pump`.
struct Desk {
    fake: FakePlatform,
    driver: Driver<FakePlatform>,
    queue: Arc<Mutex<Vec<PlatformEvent>>>,
}

impl Desk {
    fn new(displays: Vec<DisplayInfo>) -> Self {
        Self::with_config(displays, Config::from_toml_str(EXAMPLE).unwrap())
    }

    fn with_config(displays: Vec<DisplayInfo>, config: Config) -> Self {
        let fake = FakePlatform::new(displays);
        let queue = Arc::new(Mutex::new(Vec::new()));
        let q = queue.clone();
        fake.start_events(Arc::new(move |e| q.lock().unwrap().push(e)))
            .unwrap();
        let mut driver = Driver::new(fake.clone(), config);
        driver.resync().unwrap();
        Self {
            fake,
            driver,
            queue,
        }
    }

    /// Delivers queued platform events to the driver (like the main run loop would).
    fn pump(&mut self) -> Vec<UiRequest> {
        let events: Vec<_> = std::mem::take(&mut *self.queue.lock().unwrap());
        events
            .into_iter()
            .flat_map(|e| self.driver.on_event(e))
            .collect()
    }

    fn open(&mut self, app_id: &str) -> WindowId {
        self.open_at(app_id, Rect::new(100.0, 100.0, 640.0, 480.0))
    }

    fn open_at(&mut self, app_id: &str, frame: Rect) -> WindowId {
        let pid = app_id.len() as i32; // any stable fake pid per app
        let id = self
            .fake
            .open_window(pid, AppIdentity::new(app_id, app_id), "window", frame);
        self.pump();
        id
    }

    fn frame(&self, id: WindowId) -> Rect {
        self.fake.window(id).unwrap().frame
    }

    fn set_frames(&self) -> Vec<(WindowId, Rect)> {
        self.fake
            .commands()
            .into_iter()
            .filter_map(|c| match c {
                FakeCommand::SetFrame(id, r) => Some((id, r)),
                _ => None,
            })
            .collect()
    }
}

/// Expected rectangle for a zone on a fake display (gap 8 as in the example config).
fn zone_on(display: &DisplayInfo, cols: u8, rows: u8, zone: Zone) -> Rect {
    use venecita_core::GridSize;
    zone_rect(display.work_area, GridSize { cols, rows }, zone, 8.0)
}

const VSCODE: &str = "com.microsoft.VSCode";
const FIREFOX: &str = "org.mozilla.firefox";
const CHROME: &str = "com.google.Chrome";
const SLACK: &str = "com.tinyspeck.slackmacgap";

#[test]
fn home_lid_open_places_firefox_and_vscode_on_the_lg() {
    let mut desk = Desk::new(setups::home_open());
    let status = desk.driver.status();
    assert_eq!(status.profile.as_deref(), Some("Casa"));
    assert_eq!(status.layout.as_deref(), Some("Notebook abierta"));

    let code = desk.open(VSCODE);
    let firefox = desk.open(FIREFOX);
    let lg = setups::lg43();
    // 8 x 8 grid on the LG: VS Code top-left quarter, Firefox bottom-left quarter.
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(0, 0, 4, 4)));
    assert_eq!(
        desk.frame(firefox),
        zone_on(&lg, 8, 8, Zone::new(0, 4, 4, 4))
    );
}

#[test]
fn apps_without_rule_go_to_the_dynamic_zone() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    assert_eq!(
        desk.frame(slack),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4)),
        "lid open: the whole laptop screen is the dynamic zone"
    );
}

/// Lid closed with a 1080p and a 4K: the starter config uses both externals.
fn two_externals() -> Vec<DisplayInfo> {
    vec![
        setups::display(
            "fhd",
            "DELL P2419H",
            false,
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            1.0,
        ),
        setups::display(
            "4k",
            "LG HDR 4K",
            false,
            Rect::new(1920.0, 0.0, 2560.0, 1440.0),
            1.5,
        ),
    ]
}

#[test]
fn starter_config_uses_every_external_display() {
    let config = Config::from_toml_str(venecita_core::config::DEFAULT_CONFIG_TOML).unwrap();
    let desk = Desk::with_config(two_externals(), config);
    let status = desk.driver.status();
    assert_eq!(status.layout.as_deref(), Some("2 externals"));
    assert!(
        status.unused_displays.is_empty(),
        "{:?}",
        status.unused_displays
    );
}

#[test]
fn a_display_outside_the_layout_is_reported_and_its_windows_go_to_the_dynamic_zone() {
    // Lid closed at home, plus a second monitor the configuration doesn't know.
    let mut displays = setups::home_closed();
    displays.push(setups::display(
        "extra",
        "DELL P2419H",
        false,
        Rect::new(4712.0, 0.0, 1920.0, 1080.0),
        1.0,
    ));
    let mut desk = Desk::new(displays);
    let status = desk.driver.status();
    assert_eq!(status.layout.as_deref(), Some("Notebook cerrada"));
    assert_eq!(status.unused_displays, vec!["DELL P2419H".to_string()]);

    let slack = desk.open_at(SLACK, Rect::new(4800.0, 100.0, 640.0, 480.0));
    assert_eq!(
        desk.frame(slack),
        zone_on(&setups::lg43(), 8, 8, Zone::new(4, 0, 4, 8)),
        "the dynamic zone of the layout in use"
    );
}

#[test]
fn closing_the_lid_switches_layout_and_rearranges() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let slack = desk.open(SLACK);

    desk.fake.set_displays(setups::home_closed());
    desk.pump();

    let status = desk.driver.status();
    assert_eq!(status.layout.as_deref(), Some("Notebook cerrada"));
    let lg = setups::lg43();
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(0, 0, 4, 4)));
    assert_eq!(
        desk.frame(slack),
        zone_on(&lg, 8, 8, Zone::new(4, 0, 4, 8)),
        "the dynamic zone moved to the right half of the LG"
    );

    // Opening the lid again brings Slack back to the laptop.
    desk.fake.set_displays(setups::home_open());
    desk.pump();
    assert_eq!(
        desk.driver.status().layout.as_deref(),
        Some("Notebook abierta")
    );
    assert_eq!(
        desk.frame(slack),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4))
    );
}

#[test]
fn display_changes_are_idempotent() {
    // macOS often reports two DisplaysChanged in a row for one lid close.
    let mut desk = Desk::new(setups::home_open());
    desk.open(VSCODE);
    desk.open(SLACK);
    desk.fake.set_displays(setups::home_closed());
    desk.pump();
    desk.fake.take_commands();

    desk.fake.set_displays(setups::home_closed());
    desk.pump();
    assert!(
        desk.set_frames().is_empty(),
        "second event must not move anything"
    );
}

#[test]
fn work_setup_tells_identical_monitors_apart() {
    let mut desk = Desk::new(setups::work_open());
    assert_eq!(desk.driver.status().profile.as_deref(), Some("Trabajo"));
    let chrome = desk.open(CHROME);
    let code = desk.open(VSCODE);
    let [left, right] = setups::work_pair();
    assert_eq!(
        desk.frame(chrome),
        zone_on(&left, 8, 4, Zone::new(0, 0, 8, 4))
    );
    assert_eq!(
        desk.frame(code),
        zone_on(&right, 8, 4, Zone::new(0, 0, 6, 4))
    );
}

#[test]
fn laptop_alone_uses_the_travel_profile() {
    let mut desk = Desk::new(setups::laptop_only());
    assert_eq!(desk.driver.status().profile.as_deref(), Some("Viaje"));
    let chrome = desk.open(CHROME);
    assert_eq!(
        desk.frame(chrome),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 5, 4))
    );
}

#[test]
fn ignored_apps_and_special_windows_are_left_alone() {
    let mut desk = Desk::new(setups::home_open());
    let original = Rect::new(10.0, 50.0, 300.0, 200.0);
    let calc = desk.open_at("com.apple.calculator", original);
    let dialog = desk.fake.open_window_of_kind(
        99,
        AppIdentity::new(VSCODE, "Code"),
        "Save?",
        original,
        WindowKind::Dialog,
    );
    desk.pump();
    assert_eq!(desk.frame(calc), original);
    assert_eq!(desk.frame(dialog), original);
    assert!(desk.set_frames().is_empty());
}

#[test]
fn startup_arranges_existing_windows() {
    let fake = FakePlatform::new(setups::home_open());
    let code = fake.open_window(
        1,
        AppIdentity::new(VSCODE, "Code"),
        "x",
        Rect::new(0.0, 0.0, 100.0, 100.0),
    );
    let mut driver = Driver::new(fake.clone(), Config::from_toml_str(EXAMPLE).unwrap());
    driver.resync().unwrap();
    assert_eq!(
        fake.window(code).unwrap().frame,
        zone_on(&setups::lg43(), 8, 8, Zone::new(0, 0, 4, 4))
    );
}

#[test]
fn user_moves_are_respected_until_relayout() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let target = desk.frame(code);
    let moved = Rect::new(200.0, 200.0, 500.0, 400.0);
    desk.fake.user_move(code, moved);
    desk.pump();
    assert_eq!(desk.frame(code), moved, "Venecita never fights the user");

    // A duplicate "created" notification must not snap it back either.
    let info = desk.fake.window(code).unwrap();
    desk.driver.on_event(PlatformEvent::WindowCreated(info));
    assert_eq!(desk.frame(code), moved);

    desk.driver.on_action(Action::RelayoutAll);
    assert_eq!(desk.frame(code), target);
}

#[test]
fn pause_and_resume() {
    let mut desk = Desk::new(setups::home_open());
    desk.driver.on_action(Action::TogglePause);
    assert!(desk.driver.status().paused);
    let original = Rect::new(100.0, 100.0, 640.0, 480.0);
    let code = desk.open_at(VSCODE, original);
    assert_eq!(desk.frame(code), original, "paused: nothing moves");

    desk.driver.on_action(Action::TogglePause);
    assert_eq!(
        desk.frame(code),
        zone_on(&setups::lg43(), 8, 8, Zone::new(0, 0, 4, 4)),
        "resuming re-arranges"
    );
}

#[test]
fn profiles_can_be_chosen_by_hand_and_cycled() {
    let mut desk = Desk::new(setups::home_open());
    let chrome = desk.open(CHROME);

    desk.driver.on_action(Action::ActivateProfile {
        name: "viaje".into(),
    });
    let s = desk.driver.status();
    assert_eq!(s.profile.as_deref(), Some("Viaje"));
    assert!(s.manual);
    assert_eq!(
        desk.frame(chrome),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 5, 4))
    );

    // Casa and Viaje both fit at home: next_profile cycles between them.
    desk.driver.on_action(Action::NextProfile);
    assert_eq!(desk.driver.status().profile.as_deref(), Some("Casa"));
    desk.driver.on_action(Action::NextProfile);
    assert_eq!(desk.driver.status().profile.as_deref(), Some("Viaje"));

    desk.driver.on_action(Action::AutoProfile);
    let s = desk.driver.status();
    assert_eq!(s.profile.as_deref(), Some("Casa"));
    assert!(!s.manual);
    let fits: Vec<_> = s
        .profiles
        .iter()
        .filter(|p| p.fits)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(fits, vec!["Casa", "Viaje"]);
}

#[test]
fn quitting_an_app_forgets_its_windows() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let pid = desk.fake.window(code).unwrap().pid;
    assert_eq!(desk.driver.status().windows, 1);
    desk.fake.quit_app(pid);
    desk.pump();
    assert_eq!(desk.driver.status().windows, 0);
}

#[test]
fn config_reload_rearranges_with_the_new_rules() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let new_config = EXAMPLE.replacen(
        "zone = { display = \"lg43\", col = 0, row = 0, w = 4, h = 4 }",
        "zone = { display = \"lg43\", col = 4, row = 0, w = 4, h = 8 }",
        1,
    );
    desk.driver
        .reload_config(Config::from_toml_str(&new_config).unwrap());
    assert_eq!(
        desk.frame(code),
        zone_on(&setups::lg43(), 8, 8, Zone::new(4, 0, 4, 8))
    );
}

#[test]
fn open_settings_is_a_ui_request() {
    let mut desk = Desk::new(setups::home_open());
    assert_eq!(
        desk.driver.on_action(Action::OpenSettings),
        vec![UiRequest::OpenSettings]
    );
}

#[test]
fn unknown_desk_places_nothing() {
    let other = setups::display(
        "x",
        "Projector",
        false,
        Rect::new(0.0, 0.0, 1920.0, 1080.0),
        1.0,
    );
    let mut desk = Desk::new(vec![other]);
    assert_eq!(desk.driver.status().profile, None);
    let original = Rect::new(100.0, 100.0, 640.0, 480.0);
    let code = desk.open_at(VSCODE, original);
    assert_eq!(desk.frame(code), original);
}

// --- Phase 4: keyboard actions on the focused window ---------------------------------------

use venecita_core::action::DisplayTarget;
use venecita_core::config::Placement;
use venecita_core::engine::{Notice, PinnedRule};
use venecita_core::Direction;

impl Desk {
    fn focus(&mut self, id: WindowId) {
        self.fake.user_focus(id);
        self.pump();
    }
}

#[test]
fn move_grow_and_shrink_by_cells() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    let lg = setups::lg43();

    desk.driver.on_action(Action::MoveZone {
        dir: Direction::Right,
    });
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(1, 0, 4, 4)));

    desk.driver.on_action(Action::Grow {
        dir: Direction::Down,
    });
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(1, 0, 4, 5)));

    desk.driver.on_action(Action::Shrink {
        dir: Direction::Left,
    });
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(2, 0, 3, 5)));
}

#[test]
fn snap_to_grid_aligns_a_hand_placed_window() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    let lg = setups::lg43();
    // Dragged by hand to roughly columns 2..6, rows 1..5 of the LG.
    let rough = zone_on(&lg, 8, 8, Zone::new(2, 1, 4, 4));
    let rough = Rect::new(
        rough.x + 30.0,
        rough.y - 20.0,
        rough.w - 25.0,
        rough.h + 15.0,
    );
    desk.fake.user_move(code, rough);
    desk.pump();

    desk.driver.on_action(Action::SnapToGrid);
    assert_eq!(desk.frame(code), zone_on(&lg, 8, 8, Zone::new(2, 1, 4, 4)));
}

#[test]
fn move_to_display_keeps_the_proportion() {
    let mut desk = Desk::new(setups::home_open());
    let firefox = desk.open(FIREFOX); // bottom-left quarter of the LG (8x8 grid)
    desk.focus(firefox);

    // Displays left to right: laptop (x = 0), LG (x = 1512). Next from the LG wraps around.
    desk.driver.on_action(Action::MoveToDisplay {
        target: DisplayTarget::Next,
    });
    assert_eq!(
        desk.frame(firefox),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 2, 4, 2)),
        "bottom-left quarter of the laptop (8x4 grid)"
    );

    desk.driver.on_action(Action::MoveToDisplay {
        target: DisplayTarget::Prev,
    });
    assert_eq!(
        desk.frame(firefox),
        zone_on(&setups::lg43(), 8, 8, Zone::new(0, 4, 4, 4))
    );
}

#[test]
fn pin_here_creates_a_rule_for_an_app_without_one() {
    let mut desk = Desk::new(setups::home_open());
    let term = desk.open("com.apple.Terminal"); // no rule: dynamic zone on the laptop
    desk.focus(term);
    let lg = setups::lg43();
    let spot = zone_on(&lg, 8, 8, Zone::new(4, 6, 4, 2));
    desk.fake.user_move(
        term,
        Rect::new(spot.x + 10.0, spot.y + 5.0, spot.w - 30.0, spot.h),
    );
    desk.pump();

    let ui = desk.driver.on_action(Action::PinHere);
    assert_eq!(desk.frame(term), spot, "snapped to the grid");
    assert_eq!(
        ui,
        vec![UiRequest::SaveRule(PinnedRule {
            profile: "Casa".into(),
            layout: "Notebook abierta".into(),
            rule_index: None,
            app_id: "com.apple.Terminal".into(),
            placement: Placement::new("lg43", Zone::new(4, 6, 4, 2)),
        })]
    );

    // The next Terminal window goes straight to the pinned zone.
    desk.fake.close_window(term);
    desk.pump();
    let again = desk.open("com.apple.Terminal");
    assert_eq!(desk.frame(again), spot);
}

#[test]
fn pin_here_updates_the_existing_rule() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    desk.driver.on_action(Action::MoveZone {
        dir: Direction::Right,
    });
    let ui = desk.driver.on_action(Action::PinHere);
    match &ui[..] {
        [UiRequest::SaveRule(rule)] => {
            assert_eq!(rule.rule_index, Some(0), "VS Code is rule 0 of the layout");
            assert_eq!(
                rule.placement,
                Placement::new("lg43", Zone::new(1, 0, 4, 4))
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    // Re-layout keeps it in the new place.
    desk.driver.on_action(Action::RelayoutAll);
    assert_eq!(
        desk.frame(code),
        zone_on(&setups::lg43(), 8, 8, Zone::new(1, 0, 4, 4))
    );
}

#[test]
fn window_actions_without_a_focused_window_explain_why() {
    let mut desk = Desk::new(setups::home_open());
    assert_eq!(
        desk.driver.on_action(Action::PinHere),
        vec![UiRequest::Notice(Notice::NoFocusedWindow)]
    );
    assert_eq!(
        desk.driver.on_action(Action::MoveZone {
            dir: Direction::Left
        }),
        vec![UiRequest::Notice(Notice::NoFocusedWindow)]
    );
}

#[test]
fn pinning_on_a_display_outside_the_layout_is_refused() {
    let mut desk = Desk::new(setups::home_open());
    desk.driver.on_action(Action::ActivateProfile {
        name: "Viaje".into(), // its layout only uses the laptop
    });
    let code = desk.open_at(VSCODE, Rect::new(2000.0, 0.0, 800.0, 600.0)); // on the LG
    desk.fake
        .user_move(code, Rect::new(2000.0, 0.0, 800.0, 600.0));
    desk.focus(code);
    assert_eq!(
        desk.driver.on_action(Action::PinHere),
        vec![UiRequest::Notice(Notice::DisplayNotInLayout)]
    );
}

// --- Phase 5: stacks, dynamic zone and tab bars --------------------------------------------

use venecita_core::engine::TabBar;

const TERMINAL: &str = "com.apple.Terminal";
const TAB_H: f64 = 24.0;

/// The frame of a stacked window: the zone minus the tab bar.
fn below_tabs(r: Rect) -> Rect {
    r.inset(0.0, TAB_H, 0.0, 0.0)
}

impl Desk {
    fn bars(&self) -> Vec<TabBar> {
        self.fake.tab_bars().into_values().collect()
    }

    fn open_titled(&mut self, app_id: &str, title: &str) -> WindowId {
        let id = self.fake.open_window(
            app_id.len() as i32,
            AppIdentity::new(app_id, app_id),
            title,
            Rect::new(100.0, 100.0, 640.0, 480.0),
        );
        self.pump();
        id
    }
}

#[test]
fn windows_of_the_same_app_stack_with_a_tab_bar() {
    let mut desk = Desk::new(setups::work_open());
    let a = desk.open_titled(CHROME, "Mail");
    assert!(desk.bars().is_empty(), "one window: no tab bar");
    let [left, _] = setups::work_pair();
    let zone = zone_on(&left, 8, 4, Zone::new(0, 0, 8, 4));
    assert_eq!(desk.frame(a), zone);

    let b = desk.open_titled(CHROME, "Docs");
    assert_eq!(
        desk.frame(a),
        below_tabs(zone),
        "both windows make room for the bar"
    );
    assert_eq!(desk.frame(b), below_tabs(zone));

    let bars = desk.bars();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].frame, Rect::new(zone.x, zone.y, zone.w, TAB_H));
    let labels: Vec<_> = bars[0]
        .tabs
        .iter()
        .map(|t| (t.label.as_str(), t.active))
        .collect();
    assert_eq!(
        labels,
        vec![
            ("com.google.Chrome — Mail", false),
            ("com.google.Chrome — Docs", true)
        ],
        "the new window is on top"
    );

    // Closing one: the bar disappears and the other window gets its full height back.
    desk.fake.close_window(b);
    desk.pump();
    assert!(desk.bars().is_empty());
    assert_eq!(desk.frame(a), zone);
}

#[test]
fn apps_without_rule_share_the_dynamic_zone() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    let term = desk.open(TERMINAL);
    let dz = zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4));
    assert_eq!(desk.frame(slack), below_tabs(dz));
    assert_eq!(desk.frame(term), below_tabs(dz));
    assert_eq!(desk.bars()[0].tabs.len(), 2);
    assert_eq!(desk.bars()[0].id, "laptop:0,0,8,4");
}

#[test]
fn cycling_stacks_and_the_dynamic_zone() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    let term = desk.open(TERMINAL);
    let code = desk.open(VSCODE); // has its own zone on the LG
    desk.focus(term);

    desk.fake.take_commands();
    desk.driver.on_action(Action::StackNext);
    assert_eq!(desk.fake.focused_window().unwrap(), Some(slack));
    assert!(desk.bars()[0].tabs[0].active, "the tab bar follows");

    // dynamic_next works from anywhere, e.g. while VS Code has the focus.
    desk.focus(code);
    desk.driver.on_action(Action::DynamicNext);
    assert_eq!(desk.fake.focused_window().unwrap(), Some(term));
    desk.driver.on_action(Action::DynamicPrev);
    assert_eq!(desk.fake.focused_window().unwrap(), Some(slack));

    // VS Code is alone in its zone: nothing to cycle.
    desk.focus(code);
    assert_eq!(
        desk.driver.on_action(Action::StackNext),
        vec![UiRequest::Notice(Notice::NoStack)]
    );
}

#[test]
fn clicking_a_tab_focuses_its_window() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    let _term = desk.open(TERMINAL);
    desk.fake.click_tab(slack);
    desk.pump();
    assert_eq!(desk.fake.focused_window().unwrap(), Some(slack));
    assert!(desk.bars()[0].tabs[0].active);
}

#[test]
fn focusing_a_stacked_window_by_other_means_updates_the_tabs() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    desk.open(TERMINAL);
    desk.focus(slack); // e.g. Cmd-Tab
    assert!(desk.bars()[0].tabs[0].active);
}

#[test]
fn dragging_a_window_out_of_its_zone_leaves_the_stack() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    let term = desk.open(TERMINAL);
    desk.fake
        .user_move(term, Rect::new(2500.0, 300.0, 600.0, 400.0)); // onto the LG
    desk.pump();
    assert!(desk.bars().is_empty());
    assert_eq!(
        desk.frame(slack),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4))
    );
    assert_eq!(desk.frame(term), Rect::new(2500.0, 300.0, 600.0, 400.0));
}

#[test]
fn moving_a_stacked_window_by_keyboard_leaves_the_stack() {
    let mut desk = Desk::new(setups::home_open());
    let slack = desk.open(SLACK);
    let term = desk.open(TERMINAL);
    desk.focus(term);
    desk.driver.on_action(Action::MoveToDisplay {
        target: DisplayTarget::Next,
    });
    assert!(desk.bars().is_empty());
    assert_eq!(
        desk.frame(slack),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4))
    );
}

#[test]
fn closing_the_lid_moves_the_dynamic_stack_and_its_tab_bar() {
    let mut desk = Desk::new(setups::home_open());
    desk.open(SLACK);
    desk.open(TERMINAL);
    assert_eq!(desk.bars()[0].id, "laptop:0,0,8,4");

    desk.fake.set_displays(setups::home_closed());
    desk.pump();
    let bars = desk.bars();
    assert_eq!(bars.len(), 1);
    assert_eq!(
        bars[0].id, "lg43:4,0,4,8",
        "dynamic zone on the right half of the LG"
    );
    assert!(desk
        .fake
        .commands()
        .contains(&FakeCommand::HideTabBar("laptop:0,0,8,4".into())));
}

#[test]
fn quitting_an_app_removes_its_tabs() {
    let mut desk = Desk::new(setups::home_open());
    desk.open(SLACK);
    let term = desk.open(TERMINAL);
    let pid = desk.fake.window(term).unwrap().pid;
    desk.fake.quit_app(pid);
    desk.pump();
    assert!(desk.bars().is_empty());
}

#[test]
fn sleeping_screens_keep_the_current_layout() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.open(SLACK);
    desk.open(TERMINAL);
    let before = desk.frame(code);
    desk.fake.take_commands();

    desk.fake.set_displays(Vec::new()); // displays asleep: no active display
    desk.pump();
    assert_eq!(
        desk.driver.status().layout.as_deref(),
        Some("Notebook abierta")
    );
    assert_eq!(desk.bars().len(), 1, "tab bar kept");
    assert!(desk.fake.commands().is_empty(), "nothing moved or hidden");

    desk.fake.set_displays(setups::home_open()); // wake
    desk.pump();
    assert_eq!(desk.frame(code), before);
    assert!(desk.set_frames().is_empty());
}

// --- Overlay stacking and dimming --------------------------------------------------------

fn layerings(desk: &Desk) -> Vec<venecita_platform::Layering> {
    desk.fake
        .commands()
        .into_iter()
        .filter_map(|c| match c {
            FakeCommand::Layering(l) => Some(l),
            _ => None,
        })
        .collect()
}

fn dimmed_desk() -> Desk {
    let config = Config::from_toml_str(&format!(
        "{EXAMPLE}\n[dim]\nenabled = true\nopacity = 0.4\n"
    ))
    .unwrap();
    Desk::with_config(setups::home_open(), config)
}

#[test]
fn every_focus_change_restacks_the_overlays() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let fox = desk.open(FIREFOX);
    desk.focus(code);
    let before = layerings(&desk).len();
    desk.focus(fox);
    desk.focus(fox); // refocusing the same window after another app raised itself
    let all = layerings(&desk);
    assert_eq!(all.len(), before + 2, "one re-stack per focus event");
    assert_eq!(all.last().unwrap().focused, Some(fox));
    assert_eq!(all.last().unwrap().dim, None, "dimming is off by default");
}

#[test]
fn dimming_follows_the_focus_and_the_pause() {
    let mut desk = dimmed_desk();
    let code = desk.open(VSCODE);
    desk.focus(code);
    let l = desk.fake.layering().unwrap();
    assert_eq!((l.focused, l.dim), (Some(code), Some(0.4)));

    desk.driver.on_action(Action::TogglePause);
    assert_eq!(
        desk.fake.layering().unwrap().dim,
        None,
        "paused: nothing is dimmed"
    );
    desk.driver.on_action(Action::TogglePause);
    assert_eq!(desk.fake.layering().unwrap().dim, Some(0.4));

    desk.fake.close_window(code);
    desk.pump();
    let l = desk.fake.layering().unwrap();
    assert_eq!(
        (l.focused, l.dim),
        (None, None),
        "no focused window: no dim layer"
    );
}

#[test]
fn turning_dimming_on_in_the_config_applies_at_once() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    assert_eq!(desk.fake.layering().unwrap().dim, None);

    let mut config = Config::from_toml_str(EXAMPLE).unwrap();
    config.dim.enabled = true;
    desk.driver.reload_config(config);
    assert_eq!(desk.fake.layering().unwrap().dim, Some(0.3));
}

// --- Extra focus mode (phase 6) -----------------------------------------------------------

fn in_a_stack(desk: &Desk, id: WindowId) -> bool {
    desk.driver.engine().stacks().any(|(_, s)| s.contains(id))
}

/// The enlarged frame on the LG: 80 % of its work area, centered.
fn big_on_lg() -> Rect {
    setups::lg43().work_area.centered_scaled(0.8)
}

#[test]
fn focus_mode_enlarges_dims_and_restores_exactly() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    let home = desk.frame(code);

    desk.driver.on_action(Action::FocusModeToggle);
    assert!(desk.driver.engine().focus_mode());
    assert_eq!(desk.frame(code), big_on_lg());
    let l = desk.fake.layering().unwrap();
    assert_eq!(
        (l.focused, l.dim),
        (Some(code), Some(0.6)),
        "focus.dim_opacity of the example"
    );

    // The platform reports our resize: it must not count as dragging it out of its zone.
    desk.fake.user_move(code, big_on_lg());
    desk.pump();
    assert!(in_a_stack(&desk, code), "still in its zone's stack");

    desk.driver.on_action(Action::FocusModeToggle);
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(desk.frame(code), home, "back exactly where it was");
    assert_eq!(
        desk.fake.layering().unwrap().dim,
        None,
        "dimming is off in the example"
    );
}

#[test]
fn a_minimized_window_leaves_its_stack_and_its_tab_bar() {
    let mut desk = Desk::new(setups::home_open());
    let a = desk.open(TERMINAL);
    let b = desk.open(SLACK);
    assert_eq!(desk.fake.tab_bars().len(), 1);

    desk.fake.set_minimized(b, true);
    desk.pump();
    assert!(!in_a_stack(&desk, b));
    assert!(
        desk.fake.tab_bars().is_empty(),
        "one window left: no tab bar"
    );
    assert_eq!(
        desk.frame(a),
        zone_on(&setups::laptop(), 8, 4, Zone::new(0, 0, 8, 4)),
        "the window left takes the whole zone"
    );

    desk.fake.set_minimized(b, false);
    desk.pump();
    assert!(in_a_stack(&desk, b));
    assert_eq!(desk.fake.tab_bars().len(), 1, "the tab bar is back");

    // Both minimized: the stack is gone.
    desk.fake.set_minimized(a, true);
    desk.fake.set_minimized(b, true);
    desk.pump();
    assert_eq!(desk.driver.engine().stacks().count(), 0);
    assert!(desk.fake.tab_bars().is_empty());
}

#[test]
fn focus_mode_hides_every_tab_bar() {
    let mut desk = Desk::new(setups::home_open());
    let a = desk.open(TERMINAL);
    desk.open(SLACK);
    let code = desk.open(VSCODE);
    assert_eq!(desk.fake.tab_bars().len(), 1);

    desk.focus(code);
    desk.driver.on_action(Action::FocusModeToggle);
    assert!(desk.fake.tab_bars().is_empty());
    desk.driver.on_action(Action::FocusModeToggle);
    assert_eq!(desk.fake.tab_bars().len(), 1);

    // Also for a window of the stack itself.
    desk.focus(a);
    desk.driver.on_action(Action::FocusModeToggle);
    assert!(desk.fake.tab_bars().is_empty());
}

#[test]
fn focusing_another_window_ends_focus_mode() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let fox = desk.open(FIREFOX);
    desk.focus(code);
    let (code_home, fox_home) = (desk.frame(code), desk.frame(fox));

    desk.driver.on_action(Action::FocusModeToggle);
    assert_eq!(desk.frame(code), big_on_lg());
    desk.focus(code); // the enlarged window reporting its own focus keeps the mode on
    assert!(desk.driver.engine().focus_mode());

    desk.focus(fox); // e.g. a click on it, or Cmd-Tab
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(desk.frame(code), code_home, "the enlarged window goes back");
    assert_eq!(desk.frame(fox), fox_home, "the new one is not enlarged");
    assert_eq!(desk.fake.layering().unwrap().dim, None, "nothing is dimmed");

    // A late report of VS Code's enlarged frame arrives after it was restored, then the
    // report of the restore itself.
    desk.fake.user_move(code, big_on_lg());
    desk.pump();
    assert!(
        in_a_stack(&desk, code),
        "a stale report doesn't take it out of its zone"
    );
    desk.fake.user_move(code, code_home);
    desk.pump();
    assert!(in_a_stack(&desk, code));
}

#[test]
fn a_tab_of_the_same_stack_ends_focus_mode() {
    // Two windows of the dynamic zone share a stack (and a tab bar).
    let mut desk = Desk::new(setups::home_open());
    let a = desk.open(TERMINAL);
    let b = desk.open(SLACK);
    assert_eq!(
        desk.fake.tab_bars().len(),
        1,
        "both in the dynamic zone's stack"
    );
    desk.focus(a);
    let home = desk.frame(a);
    desk.driver.on_action(Action::FocusModeToggle);

    desk.fake.click_tab(b);
    desk.pump();
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(desk.frame(a), home);
    assert_ne!(desk.frame(b), big_on_lg());

    // Cycling the stack ends it too.
    desk.focus(a);
    desk.driver.on_action(Action::FocusModeToggle);
    desk.driver.on_action(Action::StackNext);
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(desk.frame(a), home);
}

#[test]
fn a_display_change_ends_focus_mode_cleanly() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    desk.driver.on_action(Action::FocusModeToggle);

    desk.fake.set_displays(setups::home_closed());
    desk.pump();
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(
        desk.frame(code),
        zone_on(&setups::lg43(), 8, 8, Zone::new(0, 0, 4, 4)),
        "laid out for the new displays"
    );
    assert_eq!(desk.fake.layering().unwrap().dim, None);
}

#[test]
fn arranging_actions_end_focus_mode_first() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    desk.focus(code);
    desk.driver.on_action(Action::FocusModeToggle);

    // Move one cell right from its real zone (0,0,4,4), not from the enlarged frame.
    desk.driver.on_action(Action::MoveZone {
        dir: venecita_core::Direction::Right,
    });
    assert!(!desk.driver.engine().focus_mode());
    assert_eq!(
        desk.frame(code),
        zone_on(&setups::lg43(), 8, 8, Zone::new(1, 0, 4, 4))
    );

    // Pausing ends it too.
    desk.driver.on_action(Action::FocusModeToggle);
    desk.driver.on_action(Action::TogglePause);
    assert!(!desk.driver.engine().focus_mode());
}

#[test]
fn a_window_joining_the_zone_keeps_the_enlarged_one_enlarged() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open_titled(VSCODE, "one");
    desk.focus(code);
    desk.driver.on_action(Action::FocusModeToggle);

    // A second VS Code window joins the same zone; its stack is re-placed.
    desk.open_titled(VSCODE, "two");
    assert_eq!(desk.frame(code), big_on_lg(), "still enlarged");

    desk.driver.on_action(Action::FocusModeToggle);
    let zone = zone_on(&setups::lg43(), 8, 8, Zone::new(0, 0, 4, 4));
    assert_eq!(
        desk.frame(code),
        below_tabs(zone),
        "goes back to its new place, under the tabs"
    );
}

#[test]
fn focus_mode_needs_a_focused_window() {
    let mut desk = Desk::new(setups::home_open());
    let requests = desk.driver.on_action(Action::FocusModeToggle);
    assert!(!desk.driver.engine().focus_mode());
    assert!(matches!(
        requests.as_slice(),
        [UiRequest::Notice(
            venecita_core::engine::Notice::NoFocusedWindow
        )]
    ));
}

// --- Windows that change kind after appearing ---------------------------------------------

#[test]
fn a_window_that_becomes_standard_later_is_placed_on_focus() {
    let mut desk = Desk::new(setups::home_open());
    // Appears as a dialog-like window (e.g. a wxWidgets app's splash/loading window).
    let start = Rect::new(354.0, 215.0, 1501.0, 1110.0);
    let id = desk.fake.open_window_of_kind(
        7,
        AppIdentity::new(VSCODE, "Code"),
        "loading",
        start,
        WindowKind::Dialog,
    );
    desk.pump();
    assert_eq!(desk.frame(id), start, "dialogs are left alone");

    // It turns into a standard window without telling anyone; the next focus re-reads it.
    desk.fake.set_window_kind(id, WindowKind::Normal);
    desk.focus(id);
    assert_eq!(
        desk.frame(id),
        zone_on(&setups::lg43(), 8, 8, Zone::new(0, 0, 4, 4)),
        "placed by its rule once it is a normal window"
    );
}

#[test]
fn refocusing_a_known_window_respects_where_the_user_put_it() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let moved = Rect::new(500.0, 400.0, 900.0, 700.0);
    desk.fake.user_move(code, moved);
    desk.pump();
    desk.focus(code); // re-read on focus: same kind, so nothing moves
    assert_eq!(desk.frame(code), moved);
}

// --- Automatic arranging --------------------------------------------------------------------

#[test]
fn only_the_users_moves_count_for_automatic_arranging() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let placed = desk.frame(code);
    let engine = desk.driver.engine();
    assert!(
        !engine.is_user_move(code, placed),
        "Venecita's own placement"
    );
    assert!(engine.is_user_move(code, Rect::new(500.0, 400.0, 900.0, 700.0)));

    // Windows Venecita leaves alone never trigger it.
    let calc = desk.open("com.apple.calculator"); // ignored in the example config
    assert!(!desk
        .driver
        .engine()
        .is_user_move(calc, Rect::new(10.0, 10.0, 300.0, 300.0)));

    // Nor does focus mode resizing the focused window.
    desk.focus(code);
    desk.driver.on_action(Action::FocusModeToggle);
    assert!(!desk.driver.engine().is_user_move(code, desk.frame(code)));
}

#[test]
fn arranging_puts_a_moved_window_back() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let home = desk.frame(code);
    desk.fake
        .user_move(code, Rect::new(2000.0, 900.0, 800.0, 600.0));
    desk.pump();
    // What automatic arranging runs once the user lets go.
    desk.driver.on_action(Action::RelayoutAll);
    assert_eq!(desk.frame(code), home);
}

#[test]
fn every_window_can_explain_where_it_went() {
    let mut desk = Desk::new(setups::home_open());
    let code = desk.open(VSCODE);
    let slack = desk.open(SLACK);
    let calc = desk.open("com.apple.calculator");
    let e = desk.driver.engine();
    assert!(
        e.explain(code)
            .starts_with("rule 1 of layout `Notebook abierta`"),
        "{}",
        e.explain(code)
    );
    assert!(
        e.explain(slack).contains("dynamic zone"),
        "{}",
        e.explain(slack)
    );
    assert_eq!(e.explain(calc), "left alone (ignored app)");
}
