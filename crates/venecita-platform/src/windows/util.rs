//! Small helpers shared by the Windows backend: UTF-16 strings, rectangles, handles.

use windows_sys::Win32::Foundation::{GetLastError, HWND, RECT};

use crate::{PlatformError, Rect, WindowId};

/// A NUL-terminated UTF-16 copy of `s`, for Win32 `*W` functions.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// The text of a UTF-16 buffer up to its first NUL.
pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// A Win32 rectangle (edges, in physical pixels) as a core [`Rect`].
pub fn rect(r: &RECT) -> Rect {
    Rect::new(
        f64::from(r.left),
        f64::from(r.top),
        f64::from(r.right - r.left),
        f64::from(r.bottom - r.top),
    )
}

/// A core [`Rect`] as a Win32 rectangle, rounded to whole pixels.
pub fn to_rect(r: Rect) -> RECT {
    RECT {
        left: r.x.round() as i32,
        top: r.y.round() as i32,
        right: (r.x + r.w).round() as i32,
        bottom: (r.y + r.h).round() as i32,
    }
}

/// The id of a top-level window: its handle, which is unique system-wide while it exists.
pub fn window_id(hwnd: HWND) -> WindowId {
    WindowId(hwnd as usize as u64)
}

/// Inverse of [`window_id`].
pub fn hwnd(id: WindowId) -> HWND {
    id.0 as usize as HWND
}

/// The error for a failed Win32 call, with `GetLastError`.
pub fn last_error(operation: &'static str) -> PlatformError {
    // SAFETY: no preconditions.
    let code = unsafe { GetLastError() };
    PlatformError::Os {
        operation,
        code: i64::from(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_round_trip() {
        let w = wide("Venecita ñ");
        assert_eq!(*w.last().unwrap(), 0);
        assert_eq!(from_wide(&w), "Venecita ñ");
        assert_eq!(from_wide(&[0x41, 0x42, 0, 0x43]), "AB");
    }

    #[test]
    fn rects_round_trip() {
        let r = Rect::new(-1920.0, 10.0, 1920.0, 1040.0);
        assert_eq!(rect(&to_rect(r)), r);
    }
}
