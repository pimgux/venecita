//! Overlays drawn by Venecita on Windows: tab bars above zones that hold several windows,
//! and the layer that dims every window except the focused one.
//!
//! Both are layered popup windows that never take the focus (`WS_EX_NOACTIVATE`) and stay
//! out of the taskbar and Alt-Tab (`WS_EX_TOOLWINDOW`). They are not "always on top":
//! like on macOS they are placed in the z-order relative to other apps' windows, a tab bar
//! just above its zone's active window and the dim layer just below the focused window,
//! and re-stacked on every focus change ([`Layering`](crate::Layering)).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::{null, null_mut};
use std::rc::Rc;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
    BLACK_BRUSH, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER,
    DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindow, GetWindowLongPtrW,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWLP_USERDATA, GWL_EXSTYLE, GW_HWNDPREV, HTTRANSPARENT, HWND_TOP, LWA_ALPHA, MA_NOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::util::{hwnd, to_rect, wide};
use crate::{EventSink, PlatformEvent, Rect, TabBar, WindowId};

/// The event sink shared between the platform and the tab bars.
pub(super) type SharedSink = Rc<RefCell<Option<EventSink>>>;

const BAR_CLASS: &str = "VenecitaDockTabBar";
const DIM_CLASS: &str = "VenecitaDockDim";

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// The tab bar's colors, after Windows 11's own tab and segmented controls.
struct Palette {
    background: COLORREF,
    active: COLORREF,
    border: COLORREF,
    separator: COLORREF,
    text: COLORREF,
    accent: COLORREF,
}

const LIGHT: Palette = Palette {
    background: rgb(243, 243, 243),
    active: rgb(255, 255, 255),
    border: rgb(229, 229, 229),
    separator: rgb(218, 218, 218),
    text: rgb(26, 26, 26),
    accent: rgb(0, 95, 184),
};

const DARK: Palette = Palette {
    background: rgb(32, 32, 32),
    active: rgb(56, 56, 56),
    border: rgb(64, 64, 64),
    separator: rgb(58, 58, 58),
    text: rgb(255, 255, 255),
    accent: rgb(96, 205, 255),
};

/// Light or dark, as the user set Windows' app mode.
fn palette() -> &'static Palette {
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut light = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the DWORD buffer and its size are passed.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut light as *mut u32).cast(),
            &mut size,
        )
    };
    if err != 0 || light != 0 {
        &LIGHT
    } else {
        &DARK
    }
}

/// What a bar window draws and reports clicks to. Owned by [`Bar`], reached from the
/// window procedure through `GWLP_USERDATA`.
struct BarData {
    tabs: Vec<(WindowId, String, bool)>,
    sink: SharedSink,
}

struct Bar {
    hwnd: HWND,
    data: Box<RefCell<BarData>>,
    /// The zone's active window; the bar is kept just above it.
    above: HWND,
}

/// All tab bars of the application.
pub(super) struct TabBars {
    sink: SharedSink,
    bars: RefCell<HashMap<String, Bar>>,
}

impl TabBars {
    pub(super) fn new(sink: SharedSink) -> Self {
        Self {
            sink,
            bars: RefCell::new(HashMap::new()),
        }
    }

    /// Shows or updates a bar.
    pub(super) fn show(&self, bar: &TabBar) {
        let mut bars = self.bars.borrow_mut();
        let view = match bars.get_mut(&bar.id) {
            Some(view) => view,
            None => {
                let data = Box::new(RefCell::new(BarData {
                    tabs: Vec::new(),
                    sink: self.sink.clone(),
                }));
                // Opaque (not layered), so Windows 11 can round its corners like a window's.
                let Some(h) = create_popup(
                    BAR_CLASS,
                    bar_proc,
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    false,
                ) else {
                    return;
                };
                // SAFETY: the pointer stays valid until the window is destroyed in `hide`
                // (the box is dropped after DestroyWindow). The corner preference is a
                // DWORD; Windows 10 ignores it.
                unsafe {
                    SetWindowLongPtrW(h, GWLP_USERDATA, &*data as *const _ as isize);
                    let round = DWMWCP_ROUND;
                    DwmSetWindowAttribute(
                        h,
                        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                        (&round as *const i32).cast(),
                        std::mem::size_of::<i32>() as u32,
                    );
                }
                bars.entry(bar.id.clone()).or_insert(Bar {
                    hwnd: h,
                    data,
                    above: null_mut(),
                })
            }
        };
        view.data.borrow_mut().tabs = bar
            .tabs
            .iter()
            .map(|t| (t.window, t.label.clone(), t.active))
            .collect();
        let active = bar.tabs.iter().find(|t| t.active).or(bar.tabs.first());
        view.above = active.map_or(null_mut(), |t| hwnd(t.window));
        place_above(view.hwnd, view.above, Some(bar.frame));
        // SAFETY: our window; repaint with the new tabs.
        unsafe { InvalidateRect(view.hwnd, null(), 1) };
    }

    /// Hides and releases a bar.
    pub(super) fn hide(&self, id: &str) {
        if let Some(bar) = self.bars.borrow_mut().remove(id) {
            // SAFETY: our window. Its procedure no longer runs once destroyed, so the
            // data box can be dropped afterwards.
            unsafe { DestroyWindow(bar.hwnd) };
            drop(bar.data);
        }
    }

    /// Puts every bar back just above its zone's active window.
    pub(super) fn restack(&self) {
        for bar in self.bars.borrow().values() {
            place_above(bar.hwnd, bar.above, None);
        }
    }
}

impl Drop for TabBars {
    /// Destroys the remaining bars before their data boxes are freed, so the window
    /// procedure never reads a dangling `GWLP_USERDATA` pointer.
    fn drop(&mut self) {
        for (_, bar) in self.bars.get_mut().drain() {
            // SAFETY: our window; the data box is dropped after it is destroyed.
            unsafe { DestroyWindow(bar.hwnd) };
            drop(bar.data);
        }
    }
}

/// Places `h` in the z-order just above `target` (and, with `frame`, moves it there).
fn place_above(h: HWND, target: HWND, frame: Option<Rect>) {
    // "Insert after" puts a window *below* the given one, so insert after whatever is
    // directly above the target. If that is a topmost window (or nothing), the target is
    // the highest normal window and HWND_TOP puts us right above it.
    let (after, keep_order) = if target.is_null() {
        (HWND_TOP, false)
    } else {
        // SAFETY: plain z-order queries.
        let prev = unsafe { GetWindow(target, GW_HWNDPREV) };
        if prev == h {
            (null_mut(), true) // already there
        } else if prev.is_null()
            // SAFETY: as above.
            || unsafe { GetWindowLongPtrW(prev, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST != 0
        {
            (HWND_TOP, false)
        } else {
            (prev, false)
        }
    };
    let mut flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
    if keep_order {
        flags |= SWP_NOZORDER;
    }
    let r = frame.map(to_rect).unwrap_or_default();
    if frame.is_none() {
        flags |= SWP_NOMOVE | SWP_NOSIZE;
    }
    // SAFETY: our window.
    unsafe {
        SetWindowPos(
            h,
            after,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            flags,
        )
    };
}

unsafe extern "system" fn bar_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: set in `TabBars::show`; null until then.
    let data = unsafe { GetWindowLongPtrW(h, GWLP_USERDATA) } as *const RefCell<BarData>;
    match msg {
        // Clicking a tab must not take the focus away from the user's app.
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN if !data.is_null() => {
            // SAFETY: valid while the window exists (see `TabBars::show`).
            let data = unsafe { &*data };
            let x = f64::from((lp & 0xffff) as u16 as i16);
            let mut rc = RECT::default();
            // SAFETY: our window.
            unsafe { GetClientRect(h, &mut rc) };
            let (window, sink) = {
                let d = data.borrow();
                let n = d.tabs.len().max(1);
                let width = f64::from(rc.right.max(1)) / n as f64;
                let i = ((x / width) as usize).min(n - 1);
                let sink = d.sink.borrow().clone();
                (d.tabs.get(i).map(|t| t.0), sink)
            };
            if let (Some(id), Some(sink)) = (window, sink) {
                sink(PlatformEvent::TabClicked { id });
            }
            0
        }
        WM_PAINT => {
            // SAFETY: standard paint sequence on our window.
            unsafe { paint_bar(h, data) };
            0
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

/// Draws the tabs like Windows 11's: the system's light or dark background, the active tab
/// as a raised rounded "pill" with a short accent line under its title.
unsafe fn paint_bar(h: HWND, data: *const RefCell<BarData>) {
    let colors = palette();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: paired with EndPaint below; GDI objects are deleted after use.
    unsafe {
        let hdc = BeginPaint(h, &mut ps);
        let mut rc = RECT::default();
        GetClientRect(h, &mut rc);
        let fill = |r: &RECT, color: COLORREF| {
            let brush = CreateSolidBrush(color);
            FillRect(hdc, r, brush);
            DeleteObject(brush);
        };
        fill(&rc, colors.background);
        if !data.is_null() {
            let tabs = &(*data).borrow().tabs;
            let n = tabs.len().max(1) as i32;
            let height = rc.bottom - rc.top;
            let face = wide("Segoe UI");
            let font = CreateFontW(
                -(height * 9 / 20).max(9),
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(CLEARTYPE_QUALITY),
                (DEFAULT_PITCH | FF_DONTCARE) as u32,
                face.as_ptr(),
            );
            let old_font = SelectObject(hdc, font);
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, colors.text);
            let active_index = tabs.iter().position(|t| t.2);
            for (i, (_, label, active)) in tabs.iter().enumerate() {
                let i = i as i32;
                let tab = RECT {
                    left: rc.right * i / n,
                    right: rc.right * (i + 1) / n,
                    top: rc.top,
                    bottom: rc.bottom,
                };
                if *active {
                    // The raised pill, with a thin border.
                    let pen = CreatePen(PS_SOLID, 1, colors.border);
                    let brush = CreateSolidBrush(colors.active);
                    let (old_pen, old_brush) = (SelectObject(hdc, pen), SelectObject(hdc, brush));
                    RoundRect(
                        hdc,
                        tab.left + 3,
                        tab.top + 3,
                        tab.right - 3,
                        tab.bottom - 3,
                        10,
                        10,
                    );
                    SelectObject(hdc, old_pen);
                    SelectObject(hdc, old_brush);
                    DeleteObject(pen);
                    DeleteObject(brush);
                    // The accent line, centered under the title.
                    let mid = (tab.left + tab.right) / 2;
                    let half = ((tab.right - tab.left) / 6).clamp(6, 24);
                    fill(
                        &RECT {
                            left: mid - half,
                            right: mid + half,
                            top: tab.bottom - 6,
                            bottom: tab.bottom - 4,
                        },
                        colors.accent,
                    );
                } else if i > 0 && active_index != Some(i as usize - 1) {
                    // A short separator between two inactive tabs.
                    fill(
                        &RECT {
                            left: tab.left,
                            right: tab.left + 1,
                            top: tab.top + height / 4,
                            bottom: tab.bottom - height / 4,
                        },
                        colors.separator,
                    );
                }
                let mut text_rect = RECT {
                    left: tab.left + 10,
                    right: tab.right - 10,
                    ..tab
                };
                let mut text: Vec<u16> = label.encode_utf16().collect();
                DrawTextW(
                    hdc,
                    text.as_mut_ptr(),
                    text.len() as i32,
                    &mut text_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
            }
            SelectObject(hdc, old_font);
            DeleteObject(font);
        }
        EndPaint(h, &ps);
    }
}

/// The layer that darkens every window below the focused one: one click-through, black,
/// translucent window per display, placed just below the focused window.
pub(super) struct DimLayer {
    windows: RefCell<Vec<(Rect, HWND)>>,
}

impl DimLayer {
    pub(super) fn new() -> Self {
        Self {
            windows: RefCell::new(Vec::new()),
        }
    }

    /// Shows the layer with `opacity`, just below the window `below`.
    pub(super) fn show(&self, opacity: f64, below: WindowId) {
        let frames: Vec<Rect> = super::displays::list()
            .map(|ds| ds.into_iter().map(|d| d.frame).collect())
            .unwrap_or_default();
        let mut windows = self.windows.borrow_mut();
        let same =
            windows.len() == frames.len() && windows.iter().zip(&frames).all(|((f, _), g)| f == g);
        if !same {
            for (_, h) in windows.drain(..) {
                // SAFETY: our windows.
                unsafe { DestroyWindow(h) };
            }
            for frame in frames {
                if let Some(h) = create_popup(
                    DIM_CLASS,
                    dim_proc,
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    true,
                ) {
                    windows.push((frame, h));
                }
            }
        }
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let target = hwnd(below);
        for (frame, h) in windows.iter() {
            let r = to_rect(*frame);
            // SAFETY: our windows. "Insert after" the focused window = just below it.
            unsafe {
                SetLayeredWindowAttributes(*h, 0, alpha, LWA_ALPHA);
                SetWindowPos(
                    *h,
                    target,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    /// Removes the layer.
    pub(super) fn hide(&self) {
        for (_, h) in self.windows.borrow().iter() {
            // SAFETY: our windows.
            unsafe { ShowWindow(*h, SW_HIDE) };
        }
    }
}

impl Drop for DimLayer {
    fn drop(&mut self) {
        for (_, h) in self.windows.get_mut().drain(..) {
            // SAFETY: our windows.
            unsafe { DestroyWindow(h) };
        }
    }
}

unsafe extern "system" fn dim_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        // Clicks go through to the window underneath.
        return HTTRANSPARENT as LRESULT;
    }
    // SAFETY: default handling.
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// Creates a hidden popup window of `class` (registering the class on first use).
fn create_popup(class: &str, proc_: WndProc, ex_style: u32, black: bool) -> Option<HWND> {
    let name = wide(class);
    // SAFETY: the class name buffer outlives both calls; registering an existing class
    // fails harmlessly.
    unsafe {
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: instance,
            lpszClassName: name.as_ptr(),
            hbrBackground: if black {
                GetStockObject(BLACK_BRUSH)
            } else {
                null_mut()
            },
            ..Default::default()
        };
        RegisterClassW(&wc);
        let h = CreateWindowExW(
            ex_style,
            name.as_ptr(),
            null(),
            WS_POPUP,
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
            tracing::warn!(class, error = %super::util::last_error("CreateWindowExW"), "cannot create overlay");
            None
        } else {
            Some(h)
        }
    }
}
