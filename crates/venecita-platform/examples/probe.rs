//! Manual test tool for the platform backend.
//!
//! ```sh
//! cargo run -p venecita-platform --example probe              # list displays and windows
//! cargo run -p venecita-platform --example probe -- move TextEdit   # left half of its display
//! cargo run -p venecita-platform --example probe -- watch     # print events (Ctrl-C to stop)
//! cargo run -p venecita-platform --example probe -- tabbar    # draw a test tab bar for 6 s
//! ```
//!
//! The terminal running it needs the Accessibility permission
//! (System Settings → Privacy & Security → Accessibility).

#[cfg(target_os = "macos")]
fn main() {
    use std::sync::Arc;

    use objc2::MainThreadMarker;
    use std::time::Instant;

    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use venecita_core::grid::{zone_rect, GridSize, Zone};
    use venecita_core::rules::app_matches;
    use venecita_platform::macos::MacPlatform;
    use venecita_platform::{PermissionStatus, Platform};

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let platform = MacPlatform::new(mtm);
    let args: Vec<String> = std::env::args().skip(1).collect();

    let granted = platform.permissions(true) == PermissionStatus::Granted;
    let list_only = matches!(args.first().map(String::as_str), None | Some("list"));
    if !granted && !list_only {
        eprintln!(
            "Accessibility permission missing.\n\
             Grant it to your terminal in System Settings → Privacy & Security → Accessibility,\n\
             then run the probe again."
        );
        std::process::exit(1);
    }

    match args.first().map(String::as_str) {
        None | Some("list") => {
            println!("== Displays ==");
            for d in platform.displays().expect("displays") {
                let (pw, ph) = d.pixel_size();
                println!(
                    "{:>10}  {:<28} builtin={:<5} {}x{}px scale={} vendor={:?} model={:?} serial={:?}",
                    d.handle, d.name, d.builtin, pw, ph, d.scale, d.vendor, d.model, d.serial
                );
                println!(
                    "            frame={:?}\n            work ={:?}",
                    d.frame, d.work_area
                );
            }
            if !granted {
                eprintln!(
                    "\nWindows can't be listed: grant the Accessibility permission to your terminal\n\
                     (System Settings → Privacy & Security → Accessibility)."
                );
                return;
            }
            println!("\n== Windows ==");
            let focused = platform.focused_window().ok().flatten();
            for w in platform.windows().expect("windows") {
                let mark = if Some(w.id) == focused { "*" } else { " " };
                println!(
                    "{mark} {:>15} pid={:<6} {:<10} {:<32} {:<30} {:?}",
                    w.id.to_string(),
                    w.pid,
                    format!("{:?}", w.kind),
                    w.app.id,
                    w.title.chars().take(30).collect::<String>(),
                    w.frame
                );
            }
        }
        Some("move") => {
            let name = args.get(1).expect("usage: probe move <app id or name>");
            let window = platform
                .windows()
                .expect("windows")
                .into_iter()
                .find(|w| w.is_manageable() && app_matches(name, &w.app))
                .unwrap_or_else(|| panic!("no normal window of `{name}` found"));
            let (cx, cy) = window.frame.center();
            let displays = platform.displays().expect("displays");
            let display = displays
                .iter()
                .find(|d| d.frame.contains_point(cx, cy))
                .unwrap_or(&displays[0]);
            let target = zone_rect(
                display.work_area,
                GridSize::default(),
                Zone::new(0, 0, 4, 4),
                8.0,
            );
            platform.set_frame(window.id, target).expect("set_frame");
            let actual = platform.window(window.id).expect("window").frame;
            println!("requested {target:?}\nactual    {actual:?}");
            platform.focus(window.id).expect("focus");
        }
        Some("watch") => {
            // Run a real (menu-bar style) NSApplication event loop, exactly like the app
            // will: some notifications are only delivered to a running NSApplication.
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            let start = Instant::now();
            platform
                .start_events(Arc::new(move |event| {
                    println!("[{:>7.3}s] {event:?}", start.elapsed().as_secs_f64())
                }))
                .expect("start_events");
            println!(
                "Watching events. Open/close apps and windows, (un)plug displays… Ctrl-C to stop."
            );
            println!("(set RUST_LOG=venecita_platform=debug to see raw display callbacks)");
            app.run();
        }
        Some("tabbar") => {
            // Draws a tab bar with up to 3 real windows for a few seconds (nothing moves).
            use venecita_core::engine::{Tab, TabBar};
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            let windows: Vec<_> = platform
                .windows()
                .expect("windows")
                .into_iter()
                .filter(|w| w.is_manageable())
                .take(3)
                .collect();
            let Some(display) = platform.displays().expect("displays").into_iter().next() else {
                eprintln!("no active display (screens asleep?)");
                std::process::exit(1);
            };
            let wa = display.work_area;
            let bar = TabBar {
                id: "probe".into(),
                frame: venecita_platform::Rect::new(wa.x + 8.0, wa.y + 8.0, wa.w / 2.0, 24.0),
                tabs: windows
                    .iter()
                    .enumerate()
                    .map(|(i, w)| Tab {
                        window: w.id,
                        label: format!("{} — {}", w.app.name, w.title),
                        active: i == 0,
                    })
                    .collect(),
            };
            println!("tab bar at {:?}", bar.frame);
            platform
                .start_events(Arc::new(|e| {
                    if let venecita_platform::PlatformEvent::TabClicked { id } = e {
                        println!("clicked {id}");
                    }
                }))
                .expect("start_events");
            platform.show_tab_bar(&bar).expect("show");
            let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(seconds));
                std::process::exit(0);
            });
            app.run();
        }
        Some(other) => {
            eprintln!("unknown command `{other}` (use list, move <app>, watch, tabbar [seconds])");
            std::process::exit(2);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The probe currently supports macOS only.");
}
