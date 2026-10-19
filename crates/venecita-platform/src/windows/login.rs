//! Launch at login: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
};

use super::util::wide;
use crate::{PlatformError, Result};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Venecita Dock";

/// Adds or removes the current executable from the user's startup programs.
pub fn set(enabled: bool) -> Result<()> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE);
    let err = if enabled {
        let exe = std::env::current_exe().map_err(|e| PlatformError::Os {
            operation: "current_exe",
            code: i64::from(e.raw_os_error().unwrap_or(0)),
        })?;
        let command = wide(&format!("\"{}\"", exe.display()));
        // SAFETY: the data pointer and its size in bytes (NUL included) are passed.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        }
    } else {
        // SAFETY: plain call with NUL-terminated strings.
        let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if err == ERROR_FILE_NOT_FOUND {
            ERROR_SUCCESS
        } else {
            err
        }
    };
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(PlatformError::Os {
            operation: "launch at login (registry)",
            code: i64::from(err),
        })
    }
}
