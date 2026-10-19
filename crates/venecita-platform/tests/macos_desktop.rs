//! Contract tests against the real macOS desktop.
//!
//! macOS window APIs must be used on the main thread, and libtest always runs tests on
//! secondary threads, so this file uses `harness = false` and runs the checks from `main`.
//! They are opt-in because they need the Accessibility permission and briefly move the
//! focused window:
//!
//! ```sh
//! VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test macos_desktop
//! ```

#[cfg(target_os = "macos")]
fn main() {
    use objc2::MainThreadMarker;
    use venecita_platform::macos::MacPlatform;
    use venecita_platform::{contract, PermissionStatus, Platform, Rect};

    if std::env::var_os("VENECITA_DESKTOP_TESTS").is_none() {
        println!("macos_desktop: skipped (set VENECITA_DESKTOP_TESTS=1 to run)");
        return;
    }

    let mtm = MainThreadMarker::new().expect("main thread");
    let p = MacPlatform::new(mtm);
    assert_eq!(
        p.permissions(true),
        PermissionStatus::Granted,
        "grant the Accessibility permission to the app running the tests"
    );

    contract::check_all(&p);
    println!("ok: contract (displays and windows)");

    let id = p
        .focused_window()
        .expect("focused_window")
        .expect("focus a normal window before running the tests");
    let display = p.displays().expect("displays").remove(0);
    let target = Rect::new(
        display.work_area.x + 40.0,
        display.work_area.y + 40.0,
        900.0,
        600.0,
    );
    contract::check_set_frame(&p, id, target, 2.0);
    println!("ok: set_frame on the focused window (restored afterwards)");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
