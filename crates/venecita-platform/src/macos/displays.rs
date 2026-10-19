//! Display enumeration on macOS.
//!
//! CoreGraphics gives us the display list, identity (built-in, EDID numbers) and frames in
//! global top-left coordinates. AppKit's `NSScreen` adds the human readable name, the
//! backing scale and the *visible frame* (the area not covered by the menu bar and Dock),
//! but in Cocoa's bottom-left coordinate system, which we convert.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayIsBuiltin, CGDisplayMirrorsDisplay,
    CGDisplayModelNumber, CGDisplaySerialNumber, CGDisplayVendorNumber, CGGetActiveDisplayList,
    CGMainDisplayID,
};
use objc2_foundation::{NSNumber, NSString};

use crate::{DisplayInfo, PlatformError, Rect, Result};

/// Converts a rectangle from Cocoa coordinates (origin at the bottom-left of the primary
/// display, y up) to global top-left coordinates (y down).
pub fn cocoa_to_top_left(x: f64, y: f64, w: f64, h: f64, primary_height: f64) -> Rect {
    Rect::new(x, primary_height - (y + h), w, h)
}

/// Returns `Some(n)` unless `n` is zero (CoreGraphics uses 0 for "unknown").
fn non_zero(n: u32) -> Option<u32> {
    (n != 0).then_some(n)
}

/// The `CGDirectDisplayID` of an `NSScreen`.
fn screen_number(screen: &NSScreen) -> Option<CGDirectDisplayID> {
    let desc = screen.deviceDescription();
    let key = NSString::from_str("NSScreenNumber");
    let value = desc.objectForKey(&key)?;
    let number: Retained<NSNumber> = value.downcast().ok()?;
    Some(number.unsignedIntValue())
}

/// Lists the active displays. Mirrored displays are reported once.
pub fn list(mtm: MainThreadMarker) -> Result<Vec<DisplayInfo>> {
    let mut ids = [0 as CGDirectDisplayID; 32];
    let mut count = 0u32;
    // SAFETY: the buffer holds 32 ids and we pass its length.
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if err.0 != 0 {
        return Err(PlatformError::Os {
            operation: "CGGetActiveDisplayList",
            code: i64::from(err.0),
        });
    }

    let primary_height = CGDisplayBounds(CGMainDisplayID()).size.height;
    let screens = NSScreen::screens(mtm);

    let mut out = Vec::new();
    for &id in &ids[..count as usize] {
        // A display that mirrors another shows the same content: skip it.
        if CGDisplayMirrorsDisplay(id) != 0 {
            continue;
        }
        let b = CGDisplayBounds(id);
        let frame = Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height);

        let screen = screens.iter().find(|s| screen_number(s) == Some(id));
        let (name, work_area, scale) = match &screen {
            Some(s) => {
                let vf = s.visibleFrame();
                let work = cocoa_to_top_left(
                    vf.origin.x,
                    vf.origin.y,
                    vf.size.width,
                    vf.size.height,
                    primary_height,
                );
                (s.localizedName().to_string(), work, s.backingScaleFactor())
            }
            // Can happen briefly while displays reconfigure.
            None => (format!("Display {id}"), frame, 1.0),
        };

        out.push(DisplayInfo {
            handle: id.to_string(),
            name,
            builtin: CGDisplayIsBuiltin(id),
            vendor: non_zero(CGDisplayVendorNumber(id)),
            model: non_zero(CGDisplayModelNumber(id)),
            serial: non_zero(CGDisplaySerialNumber(id)),
            frame,
            // Guard against rounding making the work area poke outside the frame.
            work_area: work_area.intersection(&frame).unwrap_or(frame),
            scale,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cocoa_conversion() {
        // Primary display 1512x982, menu bar 25 pt: visibleFrame = (0, 0, 1512, 957) in Cocoa.
        assert_eq!(
            cocoa_to_top_left(0.0, 0.0, 1512.0, 957.0, 982.0),
            Rect::new(0.0, 25.0, 1512.0, 957.0)
        );
        // A 4K display above the primary one: Cocoa y = 982, height 2160 → top = -2160.
        assert_eq!(
            cocoa_to_top_left(-1164.0, 982.0, 3840.0, 2160.0, 982.0),
            Rect::new(-1164.0, -2160.0, 3840.0, 2160.0)
        );
    }
}
