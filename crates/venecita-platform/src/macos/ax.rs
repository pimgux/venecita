//! Thin, safe wrappers around the macOS Accessibility (AX) API.
//!
//! The AX API lets us read and change other applications' windows. Every call goes
//! through IPC to the target app, so calls can fail (app busy, window closed…); every
//! wrapper therefore returns `Option`/`Result` and the callers treat failures as "skip".

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType, CGPoint, CGSize};

use crate::{Pid, Rect};

/// Attribute names (the `kAX…Attribute` constants are C macros, so we spell them out).
#[allow(missing_docs)] // names are self-explanatory AX constants
pub mod attr {
    pub const WINDOWS: &str = "AXWindows";
    pub const POSITION: &str = "AXPosition";
    pub const SIZE: &str = "AXSize";
    pub const TITLE: &str = "AXTitle";
    pub const ROLE: &str = "AXRole";
    pub const SUBROLE: &str = "AXSubrole";
    pub const MINIMIZED: &str = "AXMinimized";
    pub const FULLSCREEN: &str = "AXFullScreen";
    pub const MAIN: &str = "AXMain";
    pub const FOCUSED_WINDOW: &str = "AXFocusedWindow";
    /// Set by assistive tools (and some apps for themselves). While enabled, apps animate
    /// frame changes, which makes programmatic moves slow and inaccurate.
    pub const ENHANCED_USER_INTERFACE: &str = "AXEnhancedUserInterface";
}

/// Notification names (`kAX…Notification`).
#[allow(missing_docs)]
pub mod notification {
    pub const WINDOW_CREATED: &str = "AXWindowCreated";
    pub const ELEMENT_DESTROYED: &str = "AXUIElementDestroyed";
    pub const FOCUSED_WINDOW_CHANGED: &str = "AXFocusedWindowChanged";
    pub const WINDOW_MOVED: &str = "AXWindowMoved";
    pub const WINDOW_RESIZED: &str = "AXWindowResized";
    pub const WINDOW_MINIATURIZED: &str = "AXWindowMiniaturized";
    pub const WINDOW_DEMINIATURIZED: &str = "AXWindowDeminiaturized";
}

/// Role and subrole values.
#[allow(missing_docs)]
pub mod role {
    pub const WINDOW: &str = "AXWindow";
    pub const STANDARD_WINDOW: &str = "AXStandardWindow";
    pub const DIALOG: &str = "AXDialog";
    pub const SYSTEM_DIALOG: &str = "AXSystemDialog";
}

/// The "raise" action.
const ACTION_RAISE: &str = "AXRaise";

extern "C" {
    /// Private but stable API (used by yabai, Hammerspoon, Rectangle, AeroSpace…) that maps
    /// an AX window element to its CoreGraphics window number.
    fn _AXUIElementGetWindow(element: &AXUIElement, out: *mut u32) -> AXError;
}

/// An accessibility element (an application or a window).
#[derive(Clone)]
pub struct Element(pub CFRetained<AXUIElement>);

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Element({:?})", self.window_number())
    }
}

/// Converts an `AXError` into a `Result`.
pub fn check(err: AXError) -> Result<(), AXError> {
    if err == AXError::Success {
        Ok(())
    } else {
        Err(err)
    }
}

impl Element {
    /// The accessibility element of an application.
    pub fn application(pid: Pid) -> Self {
        // SAFETY: creating an element for any pid is allowed; it may just be invalid.
        let app = unsafe { AXUIElement::new_application(pid) };
        // Don't let a hung app block us for the default 6 seconds.
        // SAFETY: valid element.
        unsafe { app.set_messaging_timeout(1.0) };
        Element(app)
    }

    /// Reads an attribute as a generic CF object.
    fn copy(&self, name: &str) -> Result<CFRetained<CFType>, AXError> {
        let key = CFString::from_str(name);
        let mut value: *const CFType = std::ptr::null();
        // SAFETY: `value` is a valid out pointer; on success it holds a +1 reference.
        check(unsafe { self.0.copy_attribute_value(&key, NonNull::from(&mut value)) })?;
        NonNull::new(value as *mut CFType)
            // SAFETY: the "Copy" rule: we own the returned reference.
            .map(|p| unsafe { CFRetained::from_raw(p) })
            .ok_or(AXError::NoValue)
    }

    /// Reads a string attribute.
    pub fn string(&self, name: &str) -> Option<String> {
        let v = self.copy(name).ok()?.downcast::<CFString>().ok()?;
        Some(v.to_string())
    }

    /// Reads a boolean attribute.
    pub fn bool(&self, name: &str) -> Option<bool> {
        let v = self.copy(name).ok()?.downcast::<CFBoolean>().ok()?;
        Some(v.as_bool())
    }

    /// Reads an attribute holding another element (e.g. the focused window).
    pub fn element(&self, name: &str) -> Option<Element> {
        self.copy(name)
            .ok()?
            .downcast::<AXUIElement>()
            .ok()
            .map(Element)
    }

    /// Reads an attribute holding an array of elements (e.g. the windows of an app).
    pub fn elements(&self, name: &str) -> Vec<Element> {
        let Ok(array) = self
            .copy(name)
            .and_then(|v| v.downcast::<CFArray>().map_err(|_| AXError::Failure))
        else {
            return Vec::new();
        };
        // SAFETY: AX element arrays contain CF objects.
        let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
        array
            .iter()
            .filter_map(|v| v.downcast::<AXUIElement>().ok().map(Element))
            .collect()
    }

    /// Reads an `AXValue` attribute into a plain struct of type `T`.
    fn value<T: Default>(&self, name: &str, ty: AXValueType) -> Option<T> {
        let v = self.copy(name).ok()?.downcast::<AXValue>().ok()?;
        let mut out = T::default();
        // SAFETY: `ty` matches `T` at every call site below.
        let ok = unsafe { v.value(ty, NonNull::from(&mut out).cast::<c_void>()) };
        ok.then_some(out)
    }

    /// Writes an `AXValue` attribute from a plain struct.
    fn set_value<T>(&self, name: &str, ty: AXValueType, mut v: T) -> Result<(), AXError> {
        // SAFETY: `ty` matches `T` at every call site below.
        let value = unsafe { AXValue::new(ty, NonNull::from(&mut v).cast::<c_void>()) }
            .ok_or(AXError::Failure)?;
        let key = CFString::from_str(name);
        // SAFETY: valid element, key and value.
        check(unsafe { self.0.set_attribute_value(&key, &value) })
    }

    /// Writes a boolean attribute.
    pub fn set_bool(&self, name: &str, v: bool) -> Result<(), AXError> {
        let key = CFString::from_str(name);
        // SAFETY: valid element, key and value.
        check(unsafe { self.0.set_attribute_value(&key, CFBoolean::new(v)) })
    }

    /// Top-left position in global top-left coordinates.
    pub fn position(&self) -> Option<CGPoint> {
        self.value(attr::POSITION, AXValueType::CGPoint)
    }

    /// Size in points.
    pub fn size(&self) -> Option<CGSize> {
        self.value(attr::SIZE, AXValueType::CGSize)
    }

    /// Position and size as a [`Rect`].
    pub fn frame(&self) -> Option<Rect> {
        let p = self.position()?;
        let s = self.size()?;
        Some(Rect::new(p.x, p.y, s.width, s.height))
    }

    /// Moves the element.
    pub fn set_position(&self, x: f64, y: f64) -> Result<(), AXError> {
        self.set_value(attr::POSITION, AXValueType::CGPoint, CGPoint { x, y })
    }

    /// Resizes the element.
    pub fn set_size(&self, w: f64, h: f64) -> Result<(), AXError> {
        self.set_value(
            attr::SIZE,
            AXValueType::CGSize,
            CGSize {
                width: w,
                height: h,
            },
        )
    }

    /// Brings the window to the front of its application.
    pub fn raise(&self) -> Result<(), AXError> {
        let action = CFString::from_static_str(ACTION_RAISE);
        // SAFETY: valid element and action name.
        check(unsafe { self.0.perform_action(&action) })
    }

    /// CoreGraphics window number of a window element.
    pub fn window_number(&self) -> Option<u32> {
        let mut id = 0u32;
        // SAFETY: valid element and out pointer.
        let err = unsafe { _AXUIElementGetWindow(&self.0, &mut id) };
        (err == AXError::Success && id != 0).then_some(id)
    }
}

/// Checks (and optionally prompts for) the Accessibility permission.
pub fn is_trusted(prompt: bool) -> bool {
    use objc2_application_services::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
    use objc2_core_foundation::CFDictionary;

    // SAFETY: reading an immutable extern static.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let value = CFBoolean::new(prompt);
    let options = CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[value]);
    // SAFETY: valid dictionary with the documented key.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) }
}
