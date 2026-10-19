//! Contract tests against the real Windows desktop.
//!
//! Like the macOS ones they run from `main` (`harness = false`) on a single thread, which
//! is what the Windows backend expects. They are opt-in because they look at the real
//! desktop:
//!
//! ```sh
//! VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test windows_desktop
//! ```
//!
//! With `VENECITA_DESKTOP_MOVE=1` they also move the focused window and put it back.

#[cfg(windows)]
fn main() {
    use venecita_platform::windows::WinPlatform;
    use venecita_platform::{contract, Platform, Rect};

    if std::env::var_os("VENECITA_DESKTOP_TESTS").is_none() {
        println!("windows_desktop: skipped (set VENECITA_DESKTOP_TESTS=1 to run)");
        return;
    }

    let p = WinPlatform::new();
    contract::check_all(&p);
    let displays = p.displays().expect("displays");
    println!(
        "ok: contract ({} display(s), {} window(s))",
        displays.len(),
        p.windows().expect("windows").len()
    );
    for d in &displays {
        println!(
            "   {:<24} builtin={} frame={:?} work={:?}",
            d.name, d.builtin, d.frame, d.work_area
        );
    }

    if std::env::var_os("VENECITA_DESKTOP_MOVE").is_some() {
        let id = p
            .focused_window()
            .expect("focused_window")
            .expect("focus a normal window before running the tests");
        let display = &displays[0];
        let target = Rect::new(
            display.work_area.x + 40.0,
            display.work_area.y + 40.0,
            800.0,
            600.0,
        );
        contract::check_set_frame(&p, id, target, 2.0);
        println!("ok: set_frame");
    }
}

#[cfg(not(windows))]
fn main() {
    println!("windows_desktop: only runs on Windows");
}
