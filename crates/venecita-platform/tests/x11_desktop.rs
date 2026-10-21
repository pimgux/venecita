//! Contract tests against a real X11 desktop (CI runs them under Xvfb with Openbox). Opt-in:
//!
//! ```sh
//! VENECITA_DESKTOP_TESTS=1 cargo test -p venecita-platform --test x11_desktop
//! ```
//!
//! With `VENECITA_DESKTOP_MOVE=1` they also open a plain test window, move it and check where
//! it ended. (A plain window, because apps like xterm only resize in steps of a character.)
//! Window managers apply moves asynchronously, so the result is polled for a moment.

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
    // Keeps the test window alive while it is used.
    let _test_window = std::env::var_os("VENECITA_DESKTOP_MOVE").map(|_| open_test_window());
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
        // Wait until the window manager manages the test window.
        let start = Instant::now();
        let id = loop {
            let found = p.windows().expect("windows").into_iter().find(|w| {
                w.app.id == TEST_CLASS && w.kind == venecita_platform::WindowKind::Normal
            });
            if let Some(w) = found {
                break w.id;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "the test window never showed up in _NET_CLIENT_LIST"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
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

/// `WM_CLASS` of the window the test opens.
#[cfg(target_os = "linux")]
const TEST_CLASS: &str = "venecita-test";

/// Opens a plain, resizable top-level window (no size increments) and keeps the connection
/// that owns it: the window disappears when the connection is dropped.
#[cfg(target_os = "linux")]
fn open_test_window() -> x11rb::rust_connection::RustConnection {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ConnectionExt, CreateWindowAux, PropMode, WindowClass,
    };
    use x11rb::wrapper::ConnectionExt as _;

    let (conn, screen) = x11rb::connect(None).expect("connect to $DISPLAY");
    let root = conn.setup().roots[screen].root;
    let window = conn.generate_id().expect("window id");
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        300,
        200,
        640,
        480,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )
    .expect("create window");
    let class = format!("{TEST_CLASS}\0{TEST_CLASS}\0");
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        class.as_bytes(),
    )
    .expect("set WM_CLASS");
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"Venecita test window",
    )
    .expect("set WM_NAME");
    conn.map_window(window).expect("map window");
    conn.flush().expect("flush");
    conn
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("x11_desktop: only runs on Linux");
}
