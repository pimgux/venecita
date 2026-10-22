//! Top-level windows: which ones count, what app they belong to, their visible frame, and
//! moving and focusing them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, SetForegroundWindow,
    SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
    SWP_NOZORDER, SW_RESTORE, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_THICKFRAME,
};

use super::util::{from_wide, hwnd, last_error, rect, wide, window_id};
use crate::{AppIdentity, Pid, PlatformError, Rect, Result, WindowId, WindowInfo, WindowKind};

/// Window classes of the shell itself (taskbars, desktop), never arranged.
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "Windows.UI.Core.CoreWindow",
];

/// Every top-level window handle, front to back.
pub fn handles() -> Vec<HWND> {
    unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `Vec` passed below, alive for the whole enumeration.
        unsafe { (*(data as *mut Vec<HWND>)).push(h) };
        1
    }
    let mut out: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `out`, which outlives the call.
    unsafe { EnumWindows(Some(collect), &mut out as *mut Vec<HWND> as LPARAM) };
    out
}

/// Whether `h` is a window a user would call an application window: visible, not owned
/// by another window (dialogs), not a tool window, not hidden by DWM ("cloaked" UWP
/// windows and windows on other virtual desktops) and not part of the shell.
///
/// An owned window still counts if it asks for its own taskbar button
/// (`WS_EX_APPWINDOW`): Delphi apps such as MobaXterm own every form, main one included,
/// by a hidden `TApplication` window.
pub fn eligible(h: HWND) -> bool {
    // SAFETY: these functions accept any handle and fail gracefully on stale ones.
    unsafe {
        if IsWindow(h) == 0 || IsWindowVisible(h) == 0 {
            return false;
        }
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        if !GetWindow(h, GW_OWNER).is_null() && ex & WS_EX_APPWINDOW == 0 {
            return false;
        }
        if GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_CHILD != 0 {
            return false;
        }
        if ex & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0 {
            return false;
        }
    }
    !cloaked(h) && !SHELL_CLASSES.contains(&class_name(h).as_str())
}

fn cloaked(h: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: DWMWA_CLOAKED writes a DWORD into the 4-byte buffer.
    let hr = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED as u32,
            (&mut value as *mut u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    };
    hr == 0 && value != 0
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer length is passed.
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    from_wide(&buf[..n.max(0) as usize])
}

/// Process id of the window's owner.
pub fn pid(h: HWND) -> Pid {
    let mut pid = 0u32;
    // SAFETY: plain out-parameter.
    unsafe { GetWindowThreadProcessId(h, &mut pid) };
    pid as Pid
}

/// Details of an eligible window, or `None` if it isn't one.
pub fn info(h: HWND) -> Option<WindowInfo> {
    if !eligible(h) {
        return None;
    }
    let pid = pid(h);
    let frame = visible_frame(h)?;
    Some(WindowInfo {
        id: window_id(h),
        pid,
        app: identity(pid),
        title: title(h),
        frame,
        kind: kind(h, frame),
    })
}

fn title(h: HWND) -> String {
    // SAFETY: the buffer is sized from GetWindowTextLengthW (+1 for the NUL).
    unsafe {
        let len = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        from_wide(&buf[..n.max(0) as usize])
    }
}

fn kind(h: HWND, frame: Rect) -> WindowKind {
    // SAFETY: plain queries on a window handle.
    let (iconic, style) = unsafe { (IsIconic(h) != 0, GetWindowLongPtrW(h, GWL_STYLE) as u32) };
    if iconic {
        return WindowKind::Minimized;
    }
    let caption = style & WS_CAPTION == WS_CAPTION;
    if !caption && monitor_rect(h).is_some_and(|m| m.approx_eq(&frame, 1.0)) {
        return WindowKind::Fullscreen;
    }
    match (style & WS_THICKFRAME != 0, caption) {
        (true, _) => WindowKind::Normal,
        // Fixed-size windows with a title bar are dialogs (About boxes, installers…).
        (false, true) => WindowKind::Dialog,
        (false, false) => WindowKind::Floating,
    }
}

fn monitor_rect(h: HWND) -> Option<Rect> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` has its size set.
    unsafe {
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        (GetMonitorInfoW(m, &mut info) != 0).then(|| rect(&info.rcMonitor))
    }
}

/// What the user sees of the window: its frame without the invisible resize borders
/// Windows 10/11 add around most windows (`DWMWA_EXTENDED_FRAME_BOUNDS`).
pub fn visible_frame(h: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: the attribute writes a RECT into a RECT-sized buffer.
    let hr = unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut r as *mut RECT).cast::<c_void>(),
            size_of::<RECT>() as u32,
        )
    };
    if hr == 0 {
        return Some(rect(&r));
    }
    // SAFETY: plain out-parameter.
    (unsafe { GetWindowRect(h, &mut r) } != 0).then(|| rect(&r))
}

/// Moves and resizes a window so that its *visible* frame is `target`.
pub fn set_frame(id: WindowId, target: Rect) -> Result<()> {
    let h = hwnd(id);
    // SAFETY: plain calls on a window handle; failures are reported.
    unsafe {
        if IsWindow(h) == 0 {
            return Err(PlatformError::WindowNotFound(id));
        }
        // A maximized or minimized window ignores SetWindowPos's size until restored.
        if IsZoomed(h) != 0 || IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
    }
    // Twice: moving a window to a monitor with another DPI makes it rescale itself, which
    // changes its size (and its invisible borders); the second pass corrects that.
    for _ in 0..2 {
        let mut outer = RECT::default();
        // SAFETY: plain out-parameter.
        if unsafe { GetWindowRect(h, &mut outer) } == 0 {
            return Err(last_error("GetWindowRect"));
        }
        let outer = rect(&outer);
        let visible = visible_frame(h).unwrap_or(outer);
        let (left, top) = (visible.x - outer.x, visible.y - outer.y);
        let right = (outer.x + outer.w) - (visible.x + visible.w);
        let bottom = (outer.y + outer.h) - (visible.y + visible.h);
        // SAFETY: plain call; failures are reported.
        let ok = unsafe {
            SetWindowPos(
                h,
                null_mut(),
                (target.x - left).round() as i32,
                (target.y - top).round() as i32,
                (target.w + left + right).round() as i32,
                (target.h + top + bottom).round() as i32,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
        if ok == 0 {
            // Typically ERROR_ACCESS_DENIED: the window belongs to an elevated process.
            return Err(last_error("SetWindowPos"));
        }
        if visible_frame(h).is_some_and(|v| v.approx_eq(&target, 1.0)) {
            break;
        }
    }
    Ok(())
}

/// Brings a window to the front and gives it the keyboard focus.
pub fn focus(id: WindowId) -> Result<()> {
    let h = hwnd(id);
    // SAFETY: plain calls on a window handle.
    unsafe {
        if IsWindow(h) == 0 {
            return Err(PlatformError::WindowNotFound(id));
        }
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h) == 0 {
            // Windows only lets the process that received the last input change the
            // foreground window. A synthetic Alt press counts as that input.
            tap_alt();
            SetForegroundWindow(h);
        }
        BringWindowToTop(h);
    }
    Ok(())
}

fn tap_alt() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_MENU,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
    // SAFETY: the array length and element size are passed.
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
}

/// The foreground window.
pub fn foreground() -> Option<HWND> {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    (!h.is_null()).then_some(h)
}

thread_local! {
    /// App identities by process id: reading the version info of an exe is not free.
    static IDENTITIES: RefCell<HashMap<Pid, AppIdentity>> = RefCell::new(HashMap::new());
}

/// The app a process runs: its exe file name as id (`code.exe`) and the product name
/// from the exe's version info as name (`Visual Studio Code`).
pub fn identity(pid: Pid) -> AppIdentity {
    if let Some(id) = IDENTITIES.with(|c| c.borrow().get(&pid).cloned()) {
        return id;
    }
    let identity = match exe_path(pid) {
        Some(path) => {
            let file = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
            let name = product_name(&path).unwrap_or_else(|| {
                file.strip_suffix(".exe")
                    .or_else(|| file.strip_suffix(".EXE"))
                    .unwrap_or(&file)
                    .to_string()
            });
            AppIdentity::new(file, name)
        }
        None => AppIdentity::new(format!("pid{pid}"), format!("pid{pid}")),
    };
    IDENTITIES.with(|c| c.borrow_mut().insert(pid, identity.clone()));
    identity
}

/// Forgets cached identities (process ids get reused after an app quits).
pub fn forget_identity(pid: Pid) {
    IDENTITIES.with(|c| c.borrow_mut().remove(&pid));
}

fn exe_path(pid: Pid) -> Option<String> {
    // SAFETY: the handle is closed below; the buffer length is passed in `len`.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// `ProductName` (else `FileDescription`) from an exe's version resource.
fn product_name(path: &str) -> Option<String> {
    let path = wide(path);
    // SAFETY: buffer sizes come from GetFileVersionInfoSizeW; VerQueryValueW returns
    // pointers into `data`, which is alive while they are read.
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        // The first language/code page pair.
        let mut ptr: *mut c_void = null_mut();
        let mut len = 0u32;
        let translation = wide(r"\VarFileInfo\Translation");
        let (lang, codepage) = if VerQueryValueW(
            data.as_ptr().cast(),
            translation.as_ptr(),
            &mut ptr,
            &mut len,
        ) != 0
            && len >= 4
        {
            let pair = ptr as *const u16;
            (*pair, *pair.add(1))
        } else {
            (0x0409, 0x04b0) // English (US), Unicode
        };
        for key in ["ProductName", "FileDescription"] {
            let query = wide(&format!(r"\StringFileInfo\{lang:04x}{codepage:04x}\{key}"));
            if VerQueryValueW(data.as_ptr().cast(), query.as_ptr(), &mut ptr, &mut len) != 0
                && len > 1
            {
                let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
                let text = from_wide(text).trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}
