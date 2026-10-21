//! Platform events on X11, from a thread with its own connection:
//!
//! * `_NET_CLIENT_LIST` changes on the root window → windows created and destroyed;
//! * `_NET_ACTIVE_WINDOW` changes → the window is re-read and reported focused;
//! * `ConfigureNotify` on a client → it was moved or resized;
//! * `_NET_WM_STATE` changes on a client → re-read (minimized, restored, fullscreen);
//! * RandR screen changes → displays changed (debounced).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, NotifyMask};
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _, EventMask, Window};
use x11rb::protocol::Event;

use super::X11;
use crate::{Debouncer, EventSink, PlatformEvent, Rect, Result, WindowId};

/// Display changes come in bursts (one per output, then the work area): wait for quiet.
const DISPLAY_DEBOUNCE: Duration = Duration::from_millis(800);

/// The running event thread. Dropping it stops delivering events (the thread itself ends
/// at its next X event).
pub(crate) struct EventThread {
    stop: Arc<AtomicBool>,
}

impl Drop for EventThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl EventThread {
    pub(crate) fn start(sink: EventSink) -> Result<Self> {
        let x = X11::connect()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::Builder::new()
            .name("venecita-x11-events".into())
            .spawn(move || run(x, sink, flag))
            .map_err(|_| crate::PlatformError::Os {
                operation: "start the X11 event thread",
                code: 0,
            })?;
        Ok(Self { stop })
    }
}

fn watch(x: &X11, window: Window) {
    let aux = ChangeWindowAttributesAux::new()
        .event_mask(EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE);
    let _ = x.conn.change_window_attributes(window, &aux);
}

fn run(x: X11, sink: EventSink, stop: Arc<AtomicBool>) {
    let display_sink = sink.clone();
    let displays = Debouncer::new(DISPLAY_DEBOUNCE, move || {
        display_sink(PlatformEvent::DisplaysChanged)
    });

    let root = ChangeWindowAttributesAux::new()
        .event_mask(EventMask::PROPERTY_CHANGE | EventMask::SUBSTRUCTURE_NOTIFY);
    let _ = x.conn.change_window_attributes(x.root, &root);
    let _ = x.conn.randr_select_input(
        x.root,
        NotifyMask::SCREEN_CHANGE | NotifyMask::CRTC_CHANGE | NotifyMask::OUTPUT_CHANGE,
    );
    // Windows that already exist are listed by the consumer itself at startup.
    let mut known: HashSet<Window> = x.client_list().into_iter().collect();
    for &w in &known {
        watch(&x, w);
    }
    let _ = x.conn.flush();
    let mut frames: HashMap<Window, Rect> = HashMap::new();

    while !stop.load(Ordering::SeqCst) {
        let Ok(event) = x.conn.wait_for_event() else {
            tracing::warn!("lost the X connection; no more window events");
            return;
        };
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let a = &x.atoms;
        let mut events = Vec::new();
        match event {
            Event::PropertyNotify(e) if e.window == x.root && e.atom == a._NET_CLIENT_LIST => {
                let now: HashSet<Window> = x.client_list().into_iter().collect();
                for &w in now.difference(&known) {
                    watch(&x, w);
                    if let Some(info) = x.info(w) {
                        events.push(PlatformEvent::WindowCreated(info));
                    }
                }
                for &w in known.difference(&now) {
                    frames.remove(&w);
                    events.push(PlatformEvent::WindowDestroyed {
                        id: WindowId(u64::from(w)),
                    });
                }
                known = now;
                let _ = x.conn.flush();
            }
            Event::PropertyNotify(e) if e.window == x.root && e.atom == a._NET_ACTIVE_WINDOW => {
                if let Some(w) = x.active_window() {
                    // Re-read it on focus, like the other backends: it may have changed kind.
                    if let Some(info) = x.info(w) {
                        events.push(PlatformEvent::WindowCreated(info));
                        events.push(PlatformEvent::WindowFocused {
                            id: WindowId(u64::from(w)),
                        });
                    }
                }
            }
            Event::PropertyNotify(e) if known.contains(&e.window) && e.atom == a._NET_WM_STATE => {
                if let Some(info) = x.info(e.window) {
                    events.push(PlatformEvent::WindowCreated(info));
                }
            }
            Event::ConfigureNotify(e) if known.contains(&e.window) => {
                if let Some(frame) = x.frame(e.window) {
                    // X repeats the same geometry often; report real changes only.
                    if frames.insert(e.window, frame) != Some(frame) {
                        events.push(PlatformEvent::WindowMoved {
                            id: WindowId(u64::from(e.window)),
                            frame,
                        });
                    }
                }
            }
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => displays.trigger(),
            _ => {}
        }
        for event in events {
            sink(event);
        }
    }
}
