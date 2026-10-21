//! Contract tests against a real X11 desktop (CI runs them under Xvfb with Openbox and an
//! xterm). Opt-in:
//!
//! ```sh
//! VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test x11_desktop
//! ```
//!
//! With `VENECITA_DESKTOP_MOVE=1` they also move the focused window and put it back. Window
//! managers apply moves asynchronously, so the result is polled for a moment.

#[cfg(target_os = "linux")]
fn main() {
    use std::time::{Duration, Instant};

    use venecita_platform::linux::X11Platform;
    use venecita_platform::{contract, Platform, Rect};

    if std::env::var_os("VENECITA_DESKTOP_TESTS").is_none() {
        println!("x11_desktop: skipped (set VENECITA_DESKTOP_TESTS=1 to run)");
        return;
    }

    let p = X11Platform::connect().expect("connect to $DISPLAY");
    contract::check_all(&p);
    let displays = p.displays().expect("displays");
    let windows = p.windows().expect("windows");
    println!(
        "ok: contract ({} display(s), {} window(s))",
        displays.len(),
        windows.len()
    );
    for d in &displays {
        println!(
            "   {:<24} frame={:?} work={:?}",
            d.name, d.frame, d.work_area
        );
    }
    for w in &windows {
        println!(
            "   {:<12} {:?} {:?} {:?}",
            w.app.id, w.kind, w.title, w.frame
        );
    }

    if std::env::var_os("VENECITA_DESKTOP_MOVE").is_some() {
        let id = p
            .focused_window()
            .expect("focused_window")
            .or_else(|| windows.first().map(|w| w.id))
            .expect("a window to move (start an xterm first)");
        let original = p.window(id).expect("window").frame;
        let wa = displays[0].work_area;
        let target = Rect::new(wa.x + 40.0, wa.y + 40.0, 500.0, 360.0);

        let settle = |want: Rect| {
            let start = Instant::now();
            loop {
                let now = p.window(id).expect("window vanished").frame;
                if now.approx_eq(&want, 2.0) || start.elapsed() > Duration::from_secs(3) {
                    return now;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        p.set_frame(id, target).expect("set_frame");
        let actual = settle(target);
        p.set_frame(id, original).expect("restore");
        settle(original);
        assert!(
            actual.approx_eq(&target, 2.0),
            "window ended at {actual:?}, expected {target:?}"
        );
        println!("ok: set_frame ({actual:?})");
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("x11_desktop: only runs on Linux");
}
