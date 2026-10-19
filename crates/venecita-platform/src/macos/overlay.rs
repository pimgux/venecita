//! Overlays drawn by Venecita: tab bars above zones that hold several windows, and the
//! layer that dims every window except the focused one.
//!
//! Each tab bar is a borderless, **non-activating** `NSPanel` whose content is a native
//! `NSSegmentedControl` (one segment per window). Being non-activating, a click on a tab
//! doesn't steal the focus from the app the user is working in; the click is reported as
//! [`PlatformEvent::TabClicked`] and the engine focuses the chosen window.
//!
//! Clicks reach Rust through a tiny Objective-C class, [`TabTarget`], used as the control's
//! target/action.
//!
//! ## Stacking
//!
//! The overlays live at the **normal** window level, ordered relative to other apps'
//! windows with `orderWindow:relativeTo:` (window numbers are system-wide). A tab bar sits
//! just above its zone's active window, so another app's window in front covers it like
//! any other window would; the dim layer sits just below the focused window. Whenever the
//! focus changes the app raises its window, so the engine asks for a re-stack
//! ([`Layering`](crate::Layering)).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSControlSize, NSNormalWindowLevel,
    NSPanel, NSScreen, NSSegmentStyle, NSSegmentSwitchTracking, NSSegmentedControl,
    NSWindowCollectionBehavior, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::{EventSink, PlatformEvent, Rect, TabBar, WindowId};

/// The event sink shared between the platform and the tab bar targets.
pub(super) type SharedSink = Rc<RefCell<Option<EventSink>>>;

/// Instance variables of [`TabTarget`].
pub struct TabTargetIvars {
    /// Window of each segment, in order.
    windows: RefCell<Vec<WindowId>>,
    sink: SharedSink,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and TabTarget doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "VenecitaTabTarget"]
    #[ivars = TabTargetIvars]
    /// Receives clicks from a tab bar's segmented control.
    pub struct TabTarget;

    impl TabTarget {
        /// Action of the segmented control.
        #[unsafe(method(tabClicked:))]
        fn tab_clicked(&self, sender: &NSSegmentedControl) {
            let index = sender.selectedSegment();
            let window = usize::try_from(index)
                .ok()
                .and_then(|i| self.ivars().windows.borrow().get(i).copied());
            let sink = self.ivars().sink.borrow().clone();
            if let (Some(id), Some(sink)) = (window, sink) {
                sink(PlatformEvent::TabClicked { id });
            }
        }
    }

    unsafe impl NSObjectProtocol for TabTarget {}
);

impl TabTarget {
    fn new(mtm: MainThreadMarker, sink: SharedSink) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TabTargetIvars {
            windows: RefCell::new(Vec::new()),
            sink,
        });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// One visible tab bar.
struct BarView {
    panel: Retained<NSPanel>,
    control: Retained<NSSegmentedControl>,
    target: Retained<TabTarget>,
    /// Window number of the zone's active window; the bar is kept just above it.
    above: isize,
}

/// The system-wide window number of a window (what `orderWindow:relativeTo:` expects).
fn window_number(id: WindowId) -> isize {
    super::windows::split_window_id(id).1 as isize
}

/// A borderless, non-activating panel at the normal window level that follows the user
/// never hides when Venecita isn't the active app (it never is).
///
/// With `all_spaces` the panel shows on every Space, full-screen apps included; otherwise it
/// stays on the Space it was shown on, like the windows it belongs to.
fn overlay_panel(mtm: MainThreadMarker, frame: NSRect, all_spaces: bool) -> Retained<NSPanel> {
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    panel.setLevel(NSNormalWindowLevel);
    let mut behavior =
        NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::IgnoresCycle;
    if all_spaces {
        behavior |= NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary;
    }
    panel.setCollectionBehavior(behavior);
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    // SAFETY: we keep our own reference to the panel.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setHasShadow(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel
}

/// All tab bars of the application.
pub(super) struct TabBars {
    mtm: MainThreadMarker,
    sink: SharedSink,
    bars: RefCell<HashMap<String, BarView>>,
}

/// Converts a top-left rectangle to Cocoa's bottom-left screen coordinates.
fn to_cocoa(r: Rect) -> NSRect {
    let primary_height = CGDisplayBounds(CGMainDisplayID()).size.height;
    NSRect::new(
        NSPoint::new(r.x, primary_height - (r.y + r.h)),
        NSSize::new(r.w, r.h),
    )
}

impl TabBars {
    pub(super) fn new(mtm: MainThreadMarker, sink: SharedSink) -> Self {
        Self {
            mtm,
            sink,
            bars: RefCell::new(HashMap::new()),
        }
    }

    /// Creates the panel, control and target of a new bar.
    fn create(&self, frame: NSRect) -> BarView {
        let mtm = self.mtm;
        // Not a "floating" panel: that would put it above every other app's windows. And
        // only on its windows' Space: on any other Space there is no window to stay just
        // above, so it would cover everything there.
        let panel = overlay_panel(mtm, frame, false);

        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
        let control = NSSegmentedControl::initWithFrame(NSSegmentedControl::alloc(mtm), bounds);
        control.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        control.setSegmentStyle(NSSegmentStyle::Automatic);
        control.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
        if frame.size.height < 22.0 {
            control.setControlSize(NSControlSize::Small);
        }

        let target = TabTarget::new(mtm, self.sink.clone());
        // SAFETY: the target outlives the control (both are kept in `BarView`).
        unsafe {
            control.setTarget(Some(&target));
            control.setAction(Some(sel!(tabClicked:)));
        }
        panel.setContentView(Some(&control));
        BarView {
            panel,
            control,
            target,
            above: 0,
        }
    }

    /// Shows or updates a bar.
    pub(super) fn show(&self, bar: &TabBar) {
        let frame = to_cocoa(bar.frame);
        let mut bars = self.bars.borrow_mut();
        let view = bars
            .entry(bar.id.clone())
            .or_insert_with(|| self.create(frame));
        view.panel.setFrame_display(frame, true);

        let count = bar.tabs.len();
        view.control.setSegmentCount(count as isize);
        let width = ((frame.size.width - 4.0) / count.max(1) as f64).max(20.0);
        for (i, tab) in bar.tabs.iter().enumerate() {
            let i = i as isize;
            let label = NSString::from_str(&tab.label);
            view.control.setLabel_forSegment(&label, i);
            view.control.setToolTip_forSegment(Some(&label), i);
            view.control.setWidth_forSegment(width, i);
            if tab.active {
                view.control.setSelectedSegment(i);
            }
        }
        *view.target.ivars().windows.borrow_mut() = bar.tabs.iter().map(|t| t.window).collect();
        let active = bar.tabs.iter().find(|t| t.active).or(bar.tabs.first());
        view.above = active.map_or(0, |t| window_number(t.window));
        view.panel
            .orderWindow_relativeTo(NSWindowOrderingMode::Above, view.above);
    }

    /// Puts every bar back just above its zone's active window.
    pub(super) fn restack(&self) {
        for view in self.bars.borrow().values() {
            view.panel
                .orderWindow_relativeTo(NSWindowOrderingMode::Above, view.above);
        }
    }

    /// Hides and releases a bar.
    pub(super) fn hide(&self, id: &str) {
        if let Some(view) = self.bars.borrow_mut().remove(id) {
            view.panel.orderOut(None);
        }
    }
}

/// The layer that darkens every window below the focused one: one click-through, black,
/// translucent panel per display, ordered just below the focused window.
pub(super) struct DimLayer {
    mtm: MainThreadMarker,
    /// One panel per display, with the display frames they were made for.
    panels: RefCell<Vec<(NSRect, Retained<NSPanel>)>>,
}

impl DimLayer {
    pub(super) fn new(mtm: MainThreadMarker) -> Self {
        Self {
            mtm,
            panels: RefCell::new(Vec::new()),
        }
    }

    /// Shows the layer with `opacity`, just below the window `below`.
    pub(super) fn show(&self, opacity: f64, below: WindowId) {
        let frames: Vec<NSRect> = NSScreen::screens(self.mtm)
            .iter()
            .map(|s| s.frame())
            .collect();
        let mut panels = self.panels.borrow_mut();
        let same_displays =
            panels.len() == frames.len() && panels.iter().zip(&frames).all(|((f, _), g)| f == g);
        if !same_displays {
            for (_, panel) in panels.drain(..) {
                panel.orderOut(None);
            }
            for frame in frames {
                // Every Space: it is re-stacked below the focused window on each focus change,
                // including the change that comes with switching Spaces.
                let panel = overlay_panel(self.mtm, frame, true);
                // Clicks go through to the window underneath.
                panel.setIgnoresMouseEvents(true);
                panels.push((frame, panel));
            }
        }
        let color = NSColor::colorWithCalibratedWhite_alpha(0.0, opacity);
        let number = window_number(below);
        for (_, panel) in panels.iter() {
            panel.setBackgroundColor(Some(&color));
            panel.orderWindow_relativeTo(NSWindowOrderingMode::Below, number);
        }
    }

    /// Removes the layer.
    pub(super) fn hide(&self) {
        for (_, panel) in self.panels.borrow().iter() {
            panel.orderOut(None);
        }
    }
}
