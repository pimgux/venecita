//! The Windows side of the tray app (see `shell.rs`): the Win32 message loop, running code
//! on the UI thread, and a few system services.
//!
//! Code is handed to the UI thread through a queue and a message posted to a hidden
//! message-only window. A window message (unlike `PostThreadMessage`) is still delivered
//! while a modal loop runs on the thread, such as the tray menu being open.

use std::collections::VecDeque;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

use tracing::warn;
use venecita_platform::windows::WinPlatform;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetMessageW, GetWindow,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, ShowWindow, TranslateMessage, GW_OWNER, HWND_MESSAGE, MB_OK, MSG,
    SW_RESTORE, WM_APP, WNDCLASSW,
};

/// The platform backend.
pub type Native = WinPlatform;

/// Proof that [`init`] ran (on the UI thread).
pub struct Ui;

/// Message telling the hidden window to run the queued closures.
const WM_RUN: u32 = WM_APP + 1;

type Job = Box<dyn FnOnce() + Send>;

static QUEUE: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static RUNNER: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());

/// Creates the hidden window that runs posted code. Call once, on the UI thread.
pub fn init() -> Ui {
    // The manifest already declares per-monitor DPI awareness (v2); this covers running
    // the binary without it (e.g. `cargo run`). It fails harmlessly if already set.
    // SAFETY: plain call.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let class: Vec<u16> = "VenecitaDockRunner\0".encode_utf16().collect();
    // SAFETY: the class name lives for the whole call; the window is never destroyed.
    let runner = unsafe {
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(runner_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        )
    };
    assert!(!runner.is_null(), "cannot create the UI message window");
    RUNNER.store(runner, Ordering::SeqCst);
    Ui
}

/// Creates the platform backend.
pub fn new_platform(_ui: &Ui) -> Native {
    WinPlatform::new()
}

/// Runs the message loop until [`quit`].
pub fn run_loop(_ui: Ui) {
    let mut msg = MSG::default();
    // SAFETY: the standard message loop.
    unsafe {
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Schedules `f` on the UI thread.
pub fn run_on_main(f: Box<dyn FnOnce() + Send>) {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push_back(f);
    let runner = RUNNER.load(Ordering::SeqCst);
    if !runner.is_null() {
        // SAFETY: posting to our own window from any thread is allowed.
        unsafe { PostMessageW(runner, WM_RUN, 0, 0) };
    }
}

unsafe extern "system" fn runner_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_RUN {
        // One at a time, without holding the lock: a job may post more jobs.
        loop {
            let job = QUEUE.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
            match job {
                Some(job) => job(),
                None => break,
            }
        }
        return 0;
    }
    // SAFETY: default handling.
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

/// Ends the message loop.
pub fn quit() {
    // SAFETY: called on the UI thread (from a menu command).
    unsafe { PostQuitMessage(0) };
}

/// The system alert sound.
pub fn beep() {
    // SAFETY: plain call.
    unsafe { MessageBeep(MB_OK) };
}

/// The user's locale, e.g. `"es-AR"`.
pub fn os_language() -> Option<String> {
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
                              // SAFETY: the buffer length is passed.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

/// Opens a folder in Explorer.
pub fn open_folder(path: &Path) {
    if let Err(e) = std::process::Command::new("explorer.exe").arg(path).spawn() {
        warn!(error = %e, path = %path.display(), "cannot open folder");
    }
}

/// Windows needs no permission to control windows: nothing to open.
pub fn open_permission_settings() {}

/// Brings a process's main window to the front.
pub fn bring_to_front(pid: u32) {
    struct Search {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn check(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `Search` below, alive for the whole enumeration.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0u32;
        // SAFETY: plain queries.
        unsafe {
            GetWindowThreadProcessId(h, &mut pid);
            if pid == search.pid && IsWindowVisible(h) != 0 && GetWindow(h, GW_OWNER).is_null() {
                search.found = h;
                return 0; // stop
            }
        }
        1
    }
    let mut search = Search {
        pid,
        found: null_mut(),
    };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        EnumWindows(Some(check), &mut search as *mut Search as LPARAM);
        if !search.found.is_null() {
            ShowWindow(search.found, SW_RESTORE);
            SetForegroundWindow(search.found);
        }
    }
}

/// White on the dark taskbar (the Windows 11 default), black on a light one.
pub fn tray_icon_color() -> [u8; 3] {
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();
    let mut light = 0u32;
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
    if err == 0 && light != 0 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}
