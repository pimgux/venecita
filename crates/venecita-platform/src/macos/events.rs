//! Turning macOS notifications into [`PlatformEvent`]s.
//!
//! Three sources are combined:
//!
//! 1. **NSWorkspace notifications** (app launched / terminated / activated, wake from
//!    sleep), delivered on the main queue.
//! 2. **One `AXObserver` per application**, attached to the main run loop, for window
//!    created / focused / moved / resized on the application element, plus "destroyed"
//!    on every window element (AX only reports destruction per element).
//! 3. **The CoreGraphics display reconfiguration callback**, debounced because one lid
//!    close produces a burst of callbacks.
//!
//! All state lives in a single-threaded [`Hub`] behind `Rc<RefCell<…>>`. AX callbacks get a
//! raw pointer to a [`Ctx`] that holds a weak reference to the hub. Events are collected
//! while the hub is borrowed and sent to the sink only after the borrow ends, so the sink
//! may freely call back into the platform.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationDidChangeScreenParametersNotification, NSRunningApplication,
    NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidHideApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification, NSWorkspaceDidUnhideApplicationNotification,
    NSWorkspaceDidWakeNotification,
};
use objc2_application_services::{AXError, AXObserver, AXUIElement};
use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFAbsoluteTimeGetCurrent, CFRetained, CFRunLoop, CFRunLoopTimer,
    CFString,
};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayChangeSummaryFlags, CGDisplayRegisterReconfigurationCallback,
    CGDisplayRemoveReconfigurationCallback,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSOperationQueue};

use super::ax::{attr, check, notification, Element};
use super::windows;
use crate::{
    AppIdentity, Debouncer, EventSink, Pid, PlatformError, PlatformEvent, Rect, Result, WindowKind,
};

/// How long to wait after the last display callback before reporting `DisplaysChanged`.
const DISPLAY_DEBOUNCE: Duration = Duration::from_millis(500);
/// A freshly launched app may not answer AX requests yet; retry this often…
const OBSERVE_RETRY_SECONDS: f64 = 0.5;
/// …this many times.
const OBSERVE_MAX_ATTEMPTS: u32 = 20;

/// Context passed to AX callbacks through the `refcon` pointer.
struct Ctx {
    hub: Weak<RefCell<Hub>>,
    pid: Pid,
    /// `Some` for per-window registrations (destroyed), `None` for app-level ones.
    window: Option<u32>,
}

/// An `AXObserver` watching one application.
struct AppObserver {
    observer: CFRetained<AXObserver>,
    app_element: Element,
    app: AppIdentity,
    /// Context for app-level notifications. Boxed so its address is stable.
    app_ctx: Box<Ctx>,
    /// Per-window "destroyed" registrations, by window number.
    windows: HashMap<u32, (Element, Box<Ctx>)>,
    /// Last reported frame per window, to drop duplicate move/resize notifications.
    frames: HashMap<u32, Rect>,
}

impl AppObserver {
    fn add(&self, element: &Element, name: &str, ctx: &Ctx) -> Result<(), AXError> {
        let name = CFString::from_str(name);
        let refcon = ctx as *const Ctx as *mut c_void;
        // SAFETY: valid observer/element/name; `ctx` outlives the registration because it is
        // owned by this struct, which removes the run loop source before dropping it.
        check(unsafe { self.observer.add_notification(&element.0, &name, refcon) })
    }

    /// Starts watching a window for destruction. Returns `false` if it was already watched.
    fn watch_window(&mut self, hub: &Weak<RefCell<Hub>>, pid: Pid, el: &Element) -> bool {
        let Some(number) = el.window_number() else {
            return false;
        };
        if self.windows.contains_key(&number) {
            return false;
        }
        let ctx = Box::new(Ctx {
            hub: hub.clone(),
            pid,
            window: Some(number),
        });
        if self.add(el, notification::ELEMENT_DESTROYED, &ctx).is_ok() {
            self.windows.insert(number, (el.clone(), ctx));
        }
        true
    }

    fn unwatch_window(&mut self, number: u32) {
        self.frames.remove(&number);
        if let Some((el, _ctx)) = self.windows.remove(&number) {
            let name = CFString::from_static_str(notification::ELEMENT_DESTROYED);
            // SAFETY: valid observer/element/name. Errors (element already gone) are fine.
            let _ = unsafe { self.observer.remove_notification(&el.0, &name) };
        }
    }
}

impl Drop for AppObserver {
    fn drop(&mut self) {
        // Stop callbacks before the contexts are freed.
        if let Some(run_loop) = CFRunLoop::main() {
            // SAFETY: valid observer.
            let source = unsafe { self.observer.run_loop_source() };
            // SAFETY: reading an immutable extern static.
            run_loop.remove_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
        }
    }
}

/// Shared, single-threaded event state.
struct Hub {
    sink: EventSink,
    apps: HashMap<Pid, AppObserver>,
}

/// Emits events to the sink without holding the hub borrow.
fn emit(hub: &Rc<RefCell<Hub>>, events: Vec<PlatformEvent>) {
    if events.is_empty() {
        return;
    }
    let sink = hub.borrow().sink.clone();
    for e in events {
        sink(e);
    }
}

/// Result of trying to observe an application.
enum Observe {
    /// Observing; contains the windows discovered.
    Ok(Vec<crate::WindowInfo>),
    /// The app isn't ready yet; try again later.
    NotReady,
    /// Give up (no permission, app gone…).
    Failed,
}

/// Creates an `AXObserver` for `pid` and registers it in the hub.
fn observe_app(hub_rc: &Rc<RefCell<Hub>>, pid: Pid, app: AppIdentity) -> Observe {
    if hub_rc.borrow().apps.contains_key(&pid) {
        return Observe::Ok(Vec::new());
    }
    let mut raw: *mut AXObserver = std::ptr::null_mut();
    // SAFETY: valid callback and out pointer.
    let err = unsafe { AXObserver::create(pid, Some(ax_callback), NonNull::from(&mut raw)) };
    let Some(observer) = (err == AXError::Success)
        .then(|| NonNull::new(raw))
        .flatten()
        // SAFETY: "Create" rule: we own the returned reference.
        .map(|p| unsafe { CFRetained::from_raw(p) })
    else {
        return Observe::Failed;
    };

    let weak = Rc::downgrade(hub_rc);
    let mut obs = AppObserver {
        observer,
        app_element: Element::application(pid),
        app: app.clone(),
        app_ctx: Box::new(Ctx {
            hub: weak.clone(),
            pid,
            window: None,
        }),
        windows: HashMap::new(),
        frames: HashMap::new(),
    };

    // The first registration tells us whether the app is ready for AX.
    match obs.add(&obs.app_element, notification::WINDOW_CREATED, &obs.app_ctx) {
        Ok(()) => {}
        Err(AXError::CannotComplete) | Err(AXError::NotImplemented) => return Observe::NotReady,
        Err(_) => return Observe::Failed,
    }
    for name in [
        notification::FOCUSED_WINDOW_CHANGED,
        notification::WINDOW_MOVED,
        notification::WINDOW_RESIZED,
        notification::WINDOW_MINIATURIZED,
        notification::WINDOW_DEMINIATURIZED,
    ] {
        if let Err(e) = obs.add(&obs.app_element, name, &obs.app_ctx) {
            tracing::debug!(pid, name, error = e.0, "AX notification not available");
        }
    }

    let mut found = Vec::new();
    for el in windows::window_elements(&obs.app_element) {
        obs.watch_window(&weak, pid, &el);
        if let Some(info) = windows::window_info(pid, &app, &el) {
            found.push(info);
        }
    }

    if let Some(run_loop) = CFRunLoop::main() {
        // SAFETY: valid observer; reading an immutable extern static.
        let source = unsafe { obs.observer.run_loop_source() };
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
    }
    hub_rc.borrow_mut().apps.insert(pid, obs);
    Observe::Ok(found)
}

/// Observes a newly launched app, retrying on the main run loop until it is ready.
fn observe_with_retry(hub: &Rc<RefCell<Hub>>, pid: Pid, app: AppIdentity, attempt: u32) {
    match observe_app(hub, pid, app.clone()) {
        Observe::Ok(found) => {
            // Windows that appeared before we were listening.
            emit(
                hub,
                found
                    .into_iter()
                    .map(PlatformEvent::WindowCreated)
                    .collect(),
            );
        }
        Observe::Failed => tracing::debug!(pid, app = %app.id, "cannot observe application"),
        Observe::NotReady if attempt >= OBSERVE_MAX_ATTEMPTS => {
            tracing::warn!(pid, app = %app.id, "application never became ready for accessibility");
        }
        Observe::NotReady => {
            let weak = Rc::downgrade(hub);
            let block = RcBlock::new(move |_timer: *mut CFRunLoopTimer| {
                if let Some(hub) = weak.upgrade() {
                    observe_with_retry(&hub, pid, app.clone(), attempt + 1);
                }
            });
            let fire = CFAbsoluteTimeGetCurrent() + OBSERVE_RETRY_SECONDS;
            // SAFETY: valid block; one-shot timer (interval 0).
            let timer =
                unsafe { CFRunLoopTimer::with_handler(None, fire, 0.0, 0, 0, Some(&block)) };
            if let (Some(timer), Some(run_loop)) = (timer, CFRunLoop::main()) {
                // SAFETY: reading an immutable extern static.
                run_loop.add_timer(Some(&timer), unsafe { kCFRunLoopDefaultMode });
            }
        }
    }
}

/// The AX observer callback (runs on the main run loop).
unsafe extern "C-unwind" fn ax_callback(
    _observer: NonNull<AXObserver>,
    element: NonNull<AXUIElement>,
    notification_name: NonNull<CFString>,
    refcon: *mut c_void,
) {
    if refcon.is_null() {
        return;
    }
    // SAFETY: refcon always points to a live `Ctx` (see `AppObserver::add`). Copy what we
    // need right away: handling "destroyed" frees this very context.
    let (hub_weak, pid, window) = {
        let ctx = unsafe { &*(refcon as *const Ctx) };
        (ctx.hub.clone(), ctx.pid, ctx.window)
    };
    let Some(hub) = hub_weak.upgrade() else {
        return;
    };
    // SAFETY: the element is valid for the duration of the callback; retain it.
    let element = Element(unsafe { CFRetained::retain(element) });
    // SAFETY: valid CFString for the duration of the callback.
    let name = unsafe { notification_name.as_ref() }.to_string();

    let mut events = Vec::new();
    {
        let mut h = hub.borrow_mut();
        let Some(obs) = h.apps.get_mut(&pid) else {
            return;
        };
        match (name.as_str(), window) {
            (notification::ELEMENT_DESTROYED, Some(number)) => {
                obs.unwatch_window(number);
                events.push(PlatformEvent::WindowDestroyed {
                    id: windows::window_id(pid, number),
                });
            }
            (notification::WINDOW_CREATED, _) => {
                let app = obs.app.clone();
                if obs.watch_window(&hub_weak, pid, &element) {
                    if let Some(info) = windows::window_info(pid, &app, &element) {
                        events.push(PlatformEvent::WindowCreated(info));
                    }
                }
            }
            // Reported again with its new kind: a minimized window leaves its stack (and its
            // tab), a restored one goes back.
            (notification::WINDOW_MINIATURIZED | notification::WINDOW_DEMINIATURIZED, _) => {
                let app = obs.app.clone();
                if let Some(info) = windows::window_info(pid, &app, &element) {
                    events.push(PlatformEvent::WindowCreated(info));
                }
            }
            (notification::FOCUSED_WINDOW_CHANGED, _) => {
                let app = obs.app.clone();
                events.extend(focused_window_events(obs, &hub_weak, pid, &app, &element));
            }
            (notification::WINDOW_MOVED | notification::WINDOW_RESIZED, _) => {
                let changed = element
                    .window_number()
                    .zip(element.frame())
                    .filter(|&(n, frame)| {
                        // AX often repeats the same frame several times; report real changes only.
                        obs.frames.insert(n, frame) != Some(frame)
                    });
                if let Some((n, frame)) = changed {
                    events.push(PlatformEvent::WindowMoved {
                        id: windows::window_id(pid, n),
                        frame,
                    });
                }
            }
            _ => {}
        }
    }
    emit(&hub, events);
}

/// The CoreGraphics display reconfiguration callback.
unsafe extern "C-unwind" fn display_callback(
    _display: CGDirectDisplayID,
    flags: CGDisplayChangeSummaryFlags,
    user_info: *mut c_void,
) {
    // Every change is reported twice: once before (BeginConfiguration) and once after.
    tracing::debug!(
        display = _display,
        flags = flags.0,
        "CoreGraphics display reconfiguration"
    );
    if flags.contains(CGDisplayChangeSummaryFlags::BeginConfigurationFlag) || user_info.is_null() {
        return;
    }
    // SAFETY: user_info points to the `Debouncer` kept alive by the `EventHub`, which
    // unregisters this callback before releasing it.
    let debouncer = unsafe { &*(user_info as *const Debouncer) };
    debouncer.trigger();
}

/// Running event subscription. Dropping it unregisters everything.
pub(super) struct EventHub {
    hub: Rc<RefCell<Hub>>,
    workspace_tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    /// Observers registered on the default notification center.
    app_tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    display_debouncer: Arc<Debouncer>,
}

impl EventHub {
    pub(super) fn start(mtm: MainThreadMarker, sink: EventSink) -> Result<Self> {
        // Display reconfiguration callbacks are only delivered to processes connected to
        // the window server; creating the shared NSApplication guarantees that connection
        // (it is a no-op inside a regular app).
        let _ = NSApplication::sharedApplication(mtm);

        let hub = Rc::new(RefCell::new(Hub {
            sink: sink.clone(),
            apps: HashMap::new(),
        }));

        // Existing applications. Their current windows are not reported: the consumer
        // lists them itself at startup with `Platform::windows`.
        for app in windows::regular_apps() {
            let pid = app.processIdentifier();
            if let Observe::NotReady = observe_app(&hub, pid, windows::identity(&app)) {
                observe_with_retry(&hub, pid, windows::identity(&app), 1);
            }
        }

        // Display changes (debounced, delivered from the debouncer thread).
        let display_sink = sink.clone();
        let display_debouncer = Arc::new(Debouncer::new(DISPLAY_DEBOUNCE, move || {
            display_sink(PlatformEvent::DisplaysChanged)
        }));
        let user_info = Arc::as_ptr(&display_debouncer) as *mut c_void;
        // SAFETY: the callback only reads `user_info`, which lives as long as `self`.
        let err =
            unsafe { CGDisplayRegisterReconfigurationCallback(Some(display_callback), user_info) };
        if err.0 != 0 {
            return Err(PlatformError::Os {
                operation: "CGDisplayRegisterReconfigurationCallback",
                code: i64::from(err.0),
            });
        }

        let mut this = Self {
            hub,
            workspace_tokens: Vec::new(),
            app_tokens: Vec::new(),
            display_debouncer,
        };
        this.observe_workspace();
        this.observe_screen_parameters();
        Ok(this)
    }

    /// AppKit's own "screens changed" notification, as a second source for display changes
    /// (both feed the same debouncer, so only one `DisplaysChanged` is emitted).
    fn observe_screen_parameters(&mut self) {
        let debouncer = self.display_debouncer.clone();
        let block = RcBlock::new(move |_n: NonNull<NSNotification>| {
            tracing::debug!("AppKit screen parameters changed");
            debouncer.trigger();
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: reading an immutable extern static; valid name, queue and block.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        };
        self.app_tokens.push(token);
    }

    /// Subscribes to NSWorkspace notifications on the main queue.
    fn observe_workspace(&mut self) {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let queue = NSOperationQueue::mainQueue();

        type Handler = fn(&Rc<RefCell<Hub>>, &NSNotification);
        // SAFETY: reading immutable extern statics.
        let subscriptions: [(&_, Handler); 6] = unsafe {
            [
                (
                    NSWorkspaceDidLaunchApplicationNotification,
                    on_launch as Handler,
                ),
                (NSWorkspaceDidTerminateApplicationNotification, on_terminate),
                (NSWorkspaceDidActivateApplicationNotification, on_activate),
                (NSWorkspaceDidWakeNotification, on_wake),
                (NSWorkspaceDidHideApplicationNotification, on_hide),
                (NSWorkspaceDidUnhideApplicationNotification, on_unhide),
            ]
        };
        for (name, handler) in subscriptions {
            let weak = Rc::downgrade(&self.hub);
            let block = RcBlock::new(move |n: NonNull<NSNotification>| {
                if let Some(hub) = weak.upgrade() {
                    // SAFETY: the notification is valid for the duration of the block.
                    handler(&hub, unsafe { n.as_ref() });
                }
            });
            // SAFETY: valid name, queue and block.
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    Some(&queue),
                    &block,
                )
            };
            self.workspace_tokens.push(token);
        }
    }
}

impl Drop for EventHub {
    fn drop(&mut self) {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        for token in self.workspace_tokens.drain(..) {
            // SAFETY: token returned by addObserverForName.
            unsafe { center.removeObserver(token.as_ref()) };
        }
        let default_center = NSNotificationCenter::defaultCenter();
        for token in self.app_tokens.drain(..) {
            // SAFETY: token returned by addObserverForName.
            unsafe { default_center.removeObserver(token.as_ref()) };
        }
        let user_info = Arc::as_ptr(&self.display_debouncer) as *mut c_void;
        // SAFETY: same pointer as registered.
        unsafe { CGDisplayRemoveReconfigurationCallback(Some(display_callback), user_info) };
        self.hub.borrow_mut().apps.clear();
    }
}

/// The application a workspace notification refers to.
fn notification_app(n: &NSNotification) -> Option<Retained<NSRunningApplication>> {
    let info = n.userInfo()?;
    // SAFETY: reading an immutable extern static.
    let key = unsafe { NSWorkspaceApplicationKey };
    info.objectForKey(key)?.downcast().ok()
}

fn on_launch(hub: &Rc<RefCell<Hub>>, n: &NSNotification) {
    let Some(app) = notification_app(n) else {
        return;
    };
    if !windows::is_regular(&app) {
        return;
    }
    let pid = app.processIdentifier();
    let identity = windows::identity(&app);
    emit(
        hub,
        vec![PlatformEvent::AppLaunched {
            pid,
            app: identity.clone(),
        }],
    );
    observe_with_retry(hub, pid, identity, 1);
}

fn on_terminate(hub: &Rc<RefCell<Hub>>, n: &NSNotification) {
    let Some(app) = notification_app(n) else {
        return;
    };
    let pid = app.processIdentifier();
    let removed = hub.borrow_mut().apps.remove(&pid);
    if removed.is_some() {
        drop(removed);
        emit(hub, vec![PlatformEvent::AppTerminated { pid }]);
    }
}

/// An app was hidden (Cmd-H): its windows are reported as minimized, so they leave their
/// stacks and tab bars like minimized windows do.
fn on_hide(hub: &Rc<RefCell<Hub>>, n: &NSNotification) {
    report_app_windows(hub, n, Some(WindowKind::Minimized));
}

/// An app was shown again: its windows are reported with their real kind.
fn on_unhide(hub: &Rc<RefCell<Hub>>, n: &NSNotification) {
    report_app_windows(hub, n, None);
}

/// Reports every watched window of the notification's app as created again, with `kind`
/// instead of the one AX gives if set.
fn report_app_windows(hub: &Rc<RefCell<Hub>>, n: &NSNotification, kind: Option<WindowKind>) {
    let Some(app) = notification_app(n) else {
        return;
    };
    let pid = app.processIdentifier();
    let events = {
        let h = hub.borrow();
        let Some(obs) = h.apps.get(&pid) else {
            return;
        };
        obs.windows
            .values()
            .filter_map(|(el, _)| windows::window_info(pid, &obs.app, el))
            .map(|mut info| {
                if let Some(kind) = kind {
                    info.kind = kind;
                }
                PlatformEvent::WindowCreated(info)
            })
            .collect()
    };
    emit(hub, events);
}

/// Switching apps doesn't change the focused window *inside* an app, so AX sends nothing;
/// report the newly active app's focused window ourselves.
fn on_activate(hub: &Rc<RefCell<Hub>>, n: &NSNotification) {
    let Some(app) = notification_app(n) else {
        return;
    };
    let pid = app.processIdentifier();
    let Some(window) = Element::application(pid).element(attr::FOCUSED_WINDOW) else {
        return;
    };
    let weak = Rc::downgrade(hub);
    let events = {
        let mut h = hub.borrow_mut();
        match h.apps.get_mut(&pid) {
            Some(obs) => {
                let app = obs.app.clone();
                focused_window_events(obs, &weak, pid, &app, &window)
            }
            None => window
                .window_number()
                .map(|n| PlatformEvent::WindowFocused {
                    id: windows::window_id(pid, n),
                })
                .into_iter()
                .collect(),
        }
    };
    emit(hub, events);
}

/// Events for a window that just got the focus: a fresh [`PlatformEvent::WindowCreated`]
/// with its current details, then [`PlatformEvent::WindowFocused`].
///
/// Re-reading the window on focus catches windows we missed or misjudged when they
/// appeared: apps that never report their windows as created (Finder), and windows that
/// start out as something else (a splash screen, a dialog) and only later become standard
/// windows (some wxWidgets/Qt apps). The engine places a window it didn't know, or one
/// that just became a normal window, and ignores the rest.
fn focused_window_events(
    obs: &mut AppObserver,
    hub: &Weak<RefCell<Hub>>,
    pid: Pid,
    app: &AppIdentity,
    window: &Element,
) -> Vec<PlatformEvent> {
    let Some(number) = window.window_number() else {
        return Vec::new();
    };
    // Make sure we hear when it closes.
    obs.watch_window(hub, pid, window);
    let mut events: Vec<PlatformEvent> = windows::window_info(pid, app, window)
        .map(PlatformEvent::WindowCreated)
        .into_iter()
        .collect();
    events.push(PlatformEvent::WindowFocused {
        id: windows::window_id(pid, number),
    });
    events
}

fn on_wake(hub: &Rc<RefCell<Hub>>, _n: &NSNotification) {
    emit(hub, vec![PlatformEvent::SystemWoke]);
}
