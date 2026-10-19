//! Connected displays: `EnumDisplayMonitors` for the geometry, DisplayConfig for the
//! monitor's real name, EDID ids and whether it is the laptop's internal panel.
//!
//! Geometry is reported in **physical pixels** of the virtual desktop. With per-monitor
//! DPI awareness that is the space `SetWindowPos` works in, and Windows has no single
//! "logical" space when monitors use different scaling, so `scale` is reported as 1 and
//! [`DisplayInfo::pixel_size`] is the frame size.

use std::collections::HashMap;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::core::BOOL;
use windows_sys::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use super::util::{from_wide, rect};
use crate::{DisplayInfo, PlatformError, Result};

/// What DisplayConfig knows about the monitor behind a GDI device (`\\.\DISPLAY1`).
#[derive(Debug, Default, Clone)]
struct Target {
    name: String,
    vendor: Option<u32>,
    model: Option<u32>,
    internal: bool,
}

/// Lists the active displays, the primary one first.
pub fn list() -> Result<Vec<DisplayInfo>> {
    let targets = targets();
    let mut out: Vec<(bool, DisplayInfo)> = monitors()?
        .into_iter()
        .filter_map(|m| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: `info` is a MONITORINFOEXW with its size set, as the API requires.
            let ok = unsafe {
                GetMonitorInfoW(m, (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>())
            };
            if ok == 0 {
                return None;
            }
            let device = from_wide(&info.szDevice);
            let target = targets.get(&device).cloned().unwrap_or_default();
            let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
            let name = if target.name.is_empty() {
                device.trim_start_matches(r"\\.\").to_string()
            } else {
                target.name
            };
            Some((
                primary,
                DisplayInfo {
                    handle: device,
                    name,
                    builtin: target.internal,
                    vendor: target.vendor,
                    model: target.model,
                    serial: None,
                    frame: rect(&info.monitorInfo.rcMonitor),
                    work_area: rect(&info.monitorInfo.rcWork),
                    scale: 1.0,
                },
            ))
        })
        .collect();
    out.sort_by_key(|(primary, _)| !primary);
    Ok(out.into_iter().map(|(_, d)| d).collect())
}

fn monitors() -> Result<Vec<HMONITOR>> {
    unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `Vec` passed below, alive for the whole enumeration.
        unsafe { (*(data as *mut Vec<HMONITOR>)).push(m) };
        1
    }
    let mut monitors: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `monitors`, which outlives the call.
    let ok = unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect),
            &mut monitors as *mut Vec<HMONITOR> as LPARAM,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Os {
            operation: "EnumDisplayMonitors",
            code: 0,
        });
    }
    Ok(monitors)
}

/// Monitor details for each active GDI device name. Empty if DisplayConfig fails (e.g.
/// in a remote desktop session); the displays are then named after their device.
fn targets() -> HashMap<String, Target> {
    let mut out = HashMap::new();
    let (mut n_paths, mut n_modes) = (0u32, 0u32);
    // SAFETY: plain out-parameters.
    let err =
        unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_paths, &mut n_modes) };
    if err != ERROR_SUCCESS {
        return out;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_paths as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_modes as usize];
    // SAFETY: the buffers have the sizes we pass.
    let err = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut n_paths,
            paths.as_mut_ptr(),
            &mut n_modes,
            modes.as_mut_ptr(),
            null_mut(),
        )
    };
    if err != ERROR_SUCCESS {
        return out;
    }
    for path in &paths[..n_paths as usize] {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        // SAFETY: the header describes the full struct it is embedded in.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
            continue;
        }
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        target.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        target.header.adapterId = path.targetInfo.adapterId;
        target.header.id = path.targetInfo.id;
        // SAFETY: as above.
        if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0 {
            continue;
        }
        let tech = target.outputTechnology;
        let nonzero = |v: u16| (v != 0).then_some(u32::from(v));
        out.insert(
            from_wide(&source.viewGdiDeviceName),
            Target {
                name: from_wide(&target.monitorFriendlyDeviceName),
                vendor: nonzero(target.edidManufactureId),
                model: nonzero(target.edidProductCodeId),
                internal: tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
            },
        );
    }
    out
}
