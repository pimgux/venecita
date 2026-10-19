//! Platform events from `SetWinEventHook` (windows shown, hidden, destroyed, focused,
//! moved by the user, minimized and restored) and from a hidden window that receives the
//! broadcasts about displays (`WM_DISPLAYCHANGE`, work area changes) and waking up.
//!
//! Hooks registered with `WINEVENT_OUTOFCONTEXT` call back on the thread that registered
//! them, from its message loop, so everything here runs on the UI thread and state lives in
//! a thread-local.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, CHILDID_SELF,
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND, OBJID_WINDOW,
    PBT_APMRESUMEAUTOMATIC, SPI_SETWORKAREA, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    WM_DISPLAYCHANGE, WM_POWERBROADCAST, WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPED,
};

use super::util::{last_error, wide, window_id};
use super::window;
use crate::{Debouncer, EventSink, PlatformEvent, Result};

/// Display changes come in bursts (one per monitor, then the work areas): wait for quiet.
const DISPLAY_DEBOUNCE: Duration = Duration::from_millis(800);

struct Hub {
    sink: EventSink,
    /// Windows reported as created, so "destroyed"/"hidden" is only sent for those.
    known: HashSet<usize>,
    display_debouncer: Arc<Debouncer>,
}

thread_local! {
    static HUB: RefCell<Option<Hub>> = const { RefCell::new(None) };
}

/// Running event subscription. Dropping it unhooks everything.
pub(super) struct EventHub {
    hooks: Vec<HWINEVENTHOOK>,
    listener: HWND,
}

impl EventHub {
    pub(super) fn start(sink: EventSink) -> Result<Self> {
        let display_sink = sink.clone();
        let display_debouncer = Arc::new(Debouncer::new(DISPLAY_DEBOUNCE, move || {
            display_sink(PlatformEvent::DisplaysChanged)
        }));
        // Windows that already exist are listed by the consumer itself at startup
        // (`Platform::windows`), so mark them known for their later "destroyed".
        let known = window::handles()
            .into_iter()
            .filter(|&h| window::eligible(h))
            .map(|h| h as usize)
            .collect();
        HUB.with(|hub| {
            *hub.borrow_mut() = Some(Hub {
                sink,
                known,
                display_debouncer,
            })
        });

        let mut hooks = Vec::new();
        for (min, max) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
        ] {
            // SAFETY: the callback is a plain function; hooks are removed in `drop`.
            let hook = unsafe {
                SetWinEventHook(
                    min,
                    max,
                    null_mut(),
                    Some(on_win_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if hook.is_null() {
                let err = last_error("SetWinEventHook");
                for h in hooks {
                    // SAFETY: hooks we registered above.
                    unsafe { UnhookWinEvent(h) };
                }
                HUB.with(|hub| hub.borrow_mut().take());
                return Err(err);
            }
            hooks.push(hook);
        }
        let listener = create_listener()?;
        Ok(Self { hooks, listener })
    }
}

impl Drop for EventHub {
    fn drop(&mut self) {
        for &h in &self.hooks {
            // SAFETY: hooks we registered.
            unsafe { UnhookWinEvent(h) };
        }
        // SAFETY: the window we created.
        unsafe { DestroyWindow(self.listener) };
        HUB.with(|hub| hub.borrow_mut().take());
    }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    h: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only events about whole windows, not their controls or carets.
    if h.is_null() || id_object != OBJID_WINDOW || id_child != CHILDID_SELF as i32 {
        return;
    }
    let events = HUB.with(|hub| {
        let mut hub = hub.borrow_mut();
        let Some(hub) = hub.as_mut() else {
            return Vec::new();
        };
        let key = h as usize;
        match event {
            // Shown, restored, minimized or focused: (re-)read it. The engine places
            // windows it doesn't know and ones that just became normal windows.
            EVENT_OBJECT_SHOW
            | EVENT_SYSTEM_MINIMIZESTART
            | EVENT_SYSTEM_MINIMIZEEND
            | EVENT_SYSTEM_FOREGROUND => {
                let mut events = Vec::new();
                if let Some(info) = window::info(h) {
                    hub.known.insert(key);
                    events.push(PlatformEvent::WindowCreated(info));
                }
                if event == EVENT_SYSTEM_FOREGROUND {
                    events.push(PlatformEvent::WindowFocused { id: window_id(h) });
                }
                events
            }
            // Hidden (e.g. minimized to the tray) or closed.
            EVENT_OBJECT_HIDE | EVENT_OBJECT_DESTROY if hub.known.remove(&key) => {
                if event == EVENT_OBJECT_DESTROY {
                    window::forget_identity(window::pid(h));
                }
                vec![PlatformEvent::WindowDestroyed { id: window_id(h) }]
            }
            // The user finished dragging or resizing it.
            EVENT_SYSTEM_MOVESIZEEND if hub.known.contains(&key) => window::visible_frame(h)
                .map(|frame| PlatformEvent::WindowMoved {
                    id: window_id(h),
                    frame,
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    });
    emit(events);
}

fn emit(events: Vec<PlatformEvent>) {
    if events.is_empty() {
        return;
    }
    let Some(sink) = HUB.with(|hub| hub.borrow().as_ref().map(|h| h.sink.clone())) else {
        return;
    };
    for event in events {
        sink(event);
    }
}

/// A hidden top-level window: broadcasts such as `WM_DISPLAYCHANGE` only reach top-level
/// windows (not message-only ones).
fn create_listener() -> Result<HWND> {
    let class = wide("VenecitaDockEvents");
    // SAFETY: the class name and window procedure live for the whole process.
    unsafe {
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(listener_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        // Registering twice (after stop/start) fails harmlessly.
        RegisterClassW(&wc);
        let h = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if h.is_null() {
            return Err(last_error("CreateWindowExW (event listener)"));
        }
        Ok(h)
    }
}

unsafe extern "system" fn listener_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let display_change =
        msg == WM_DISPLAYCHANGE || (msg == WM_SETTINGCHANGE && wp == SPI_SETWORKAREA as WPARAM);
    if display_change {
        HUB.with(|hub| {
            if let Some(hub) = hub.borrow().as_ref() {
                hub.display_debouncer.trigger();
            }
        });
    } else if msg == WM_POWERBROADCAST && wp == PBT_APMRESUMEAUTOMATIC as WPARAM {
        emit(vec![PlatformEvent::SystemWoke]);
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}
