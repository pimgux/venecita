//! Invariants every [`Platform`] implementation must satisfy.
//!
//! The fake backend runs these in normal unit tests; real backends run them in
//! `#[ignore]` tests on a real desktop (`cargo test -p venecita-platform -- --ignored`).
//! Every check panics with a descriptive message on failure.

use std::collections::HashSet;

use crate::{Platform, Rect};

/// Runs every check that does not modify the desktop.
pub fn check_all<P: Platform>(platform: &P) {
    check_displays(platform);
    check_windows(platform);
}

/// Displays: at least one, unique handles, sane geometry, at most one built-in panel.
pub fn check_displays<P: Platform>(platform: &P) {
    let displays = platform.displays().expect("displays() failed");
    assert!(
        !displays.is_empty(),
        "at least one display must be reported"
    );

    let handles: HashSet<_> = displays.iter().map(|d| &d.handle).collect();
    assert_eq!(
        handles.len(),
        displays.len(),
        "display handles must be unique"
    );

    let builtin = displays.iter().filter(|d| d.builtin).count();
    assert!(
        builtin <= 1,
        "at most one built-in display, found {builtin}"
    );

    for d in &displays {
        assert!(
            d.frame.w > 0.0 && d.frame.h > 0.0,
            "{}: empty frame",
            d.name
        );
        assert!(
            d.scale >= 1.0,
            "{}: scale must be >= 1, got {}",
            d.name,
            d.scale
        );
        assert!(
            d.frame.contains_rect(&d.work_area),
            "{}: work area {:?} must lie inside frame {:?}",
            d.name,
            d.work_area,
            d.frame
        );
    }

    for (i, a) in displays.iter().enumerate() {
        for b in &displays[i + 1..] {
            let overlap = a.frame.intersection(&b.frame).map_or(0.0, |r| r.area());
            assert!(
                overlap == 0.0,
                "displays {} and {} overlap (mirroring should report one display)",
                a.name,
                b.name
            );
        }
    }
}

/// Windows: unique ids, `window(id)` agrees with `windows()`, focused window is listed.
pub fn check_windows<P: Platform>(platform: &P) {
    let windows = platform.windows().expect("windows() failed");
    let ids: HashSet<_> = windows.iter().map(|w| w.id).collect();
    assert_eq!(ids.len(), windows.len(), "window ids must be unique");

    for w in &windows {
        let again = platform
            .window(w.id)
            .expect("window(id) failed for a listed window");
        assert_eq!(again.id, w.id);
        assert_eq!(again.pid, w.pid);
        assert_eq!(again.app, w.app);
    }

    if let Some(focused) = platform.focused_window().expect("focused_window() failed") {
        // The focused window may belong to an app we don't list (e.g. a menu-bar app),
        // but if it's a regular one it must be listed.
        if let Ok(info) = platform.window(focused) {
            assert!(
                ids.contains(&info.id),
                "focused window {} is not listed",
                info.id
            );
        }
    }
}

/// Moves a window to `target` and checks it got there (within `tolerance` points), then
/// restores its original frame. Modifies the desktop: only call from opt-in tests.
pub fn check_set_frame<P: Platform>(
    platform: &P,
    id: crate::WindowId,
    target: Rect,
    tolerance: f64,
) {
    let original = platform
        .window(id)
        .expect("window to move must exist")
        .frame;
    platform.set_frame(id, target).expect("set_frame failed");
    let actual = platform.window(id).expect("window vanished").frame;
    platform
        .set_frame(id, original)
        .expect("restoring the frame failed");
    assert!(
        actual.approx_eq(&target, tolerance),
        "window ended at {actual:?}, expected {target:?}"
    );
}
