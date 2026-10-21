//! Operating system services used by the tray app and the settings window: opening a
//! text file, the connected displays and the applications that have windows open.

use std::path::Path;

#[cfg(any(target_os = "macos", windows))]
use venecita_core::display::DisplayInfo;

/// Opens a text file in the user's text editor.
pub fn open_text_file(path: &Path) {
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open")
        .arg("-t")
        .arg(path)
        .spawn();
    #[cfg(windows)]
    let spawned = std::process::Command::new("notepad.exe").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let spawned = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = spawned {
        tracing::warn!(error = %e, path = %path.display(), "cannot open the file");
    }
}

/// Opens a folder in Finder / Explorer / the file manager.
pub fn open_folder(path: &Path) {
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(path).spawn();
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer.exe").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let spawned = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = spawned {
        tracing::warn!(error = %e, path = %path.display(), "cannot open the folder");
    }
}

/// The displays connected right now. On macOS this must run on the main thread.
#[cfg(target_os = "macos")]
pub fn connected_displays() -> Result<Vec<DisplayInfo>, String> {
    let mtm = objc2::MainThreadMarker::new().ok_or("not on the main thread")?;
    venecita_platform::macos::displays::list(mtm).map_err(|e| e.to_string())
}

/// The displays connected right now.
#[cfg(windows)]
pub fn connected_displays() -> Result<Vec<DisplayInfo>, String> {
    use venecita_platform::Platform;
    venecita_platform::windows::WinPlatform::new()
        .displays()
        .map_err(|e| e.to_string())
}

/// Regular (Dock) applications running now, as (bundle id, name), sorted by name.
#[cfg(target_os = "macos")]
pub fn running_apps() -> Vec<(String, String)> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
    let mut apps: Vec<(String, String)> = NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|a| {
            let bundle = a.bundleIdentifier()?.to_string();
            let name = a.localizedName().map(|n| n.to_string()).unwrap_or_default();
            Some((bundle, name))
        })
        .collect();
    sort_apps(&mut apps);
    apps
}

/// Applications with a window open now, as (exe name, product name), sorted by name.
#[cfg(windows)]
pub fn running_apps() -> Vec<(String, String)> {
    use venecita_platform::Platform;
    let mut apps: Vec<(String, String)> = venecita_platform::windows::WinPlatform::new()
        .windows()
        .unwrap_or_default()
        .into_iter()
        .map(|w| (w.app.id, w.app.name))
        .collect();
    sort_apps(&mut apps);
    apps
}

#[cfg(any(target_os = "macos", windows))]
fn sort_apps(apps: &mut Vec<(String, String)>) {
    apps.sort_by_key(|(id, name)| (name.to_lowercase(), id.to_lowercase()));
    apps.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
}

/// An application's icon, as straight-alpha RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIcon {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height` RGBA pixels, row by row.
    pub rgba: Vec<u8>,
}

/// The icon of the app with this bundle id (`com.microsoft.VSCode`), drawn at 64 × 64.
/// `None` if no installed app has that id. Must run on the main thread.
#[cfg(target_os = "macos")]
pub fn app_icon(app_id: &str) -> Option<AppIcon> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{
        NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext,
        NSWorkspace,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    const SIZE: u32 = 64;
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(app_id))?;
    let path = url.path()?;
    let image = workspace.iconForFile(&path);
    let n = SIZE as isize;
    // SAFETY: null planes make the rep allocate its own buffer of the given geometry;
    // reading an AppKit constant.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            n,
            n,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            32,
        )
    }?;
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    image.drawInRect_fromRect_operation_fraction(
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(f64::from(SIZE), f64::from(SIZE)),
        ),
        NSRect::ZERO,
        NSCompositingOperation::Copy,
        1.0,
    );
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();

    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    let stride = rep.bytesPerRow() as usize;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE as usize {
        // SAFETY: the rep owns `SIZE` rows of `stride` bytes.
        let row = unsafe { std::slice::from_raw_parts(data.add(y * stride), SIZE as usize * 4) };
        rgba.extend(row.chunks_exact(4).flat_map(unpremultiply));
    }
    Some(AppIcon {
        width: SIZE,
        height: SIZE,
        rgba,
    })
}

/// The icon of a running app, from its executable (`code.exe` or its product name).
/// `None` if no window of that app is open or its exe has no icon.
#[cfg(windows)]
pub fn app_icon(app_id: &str) -> Option<AppIcon> {
    use std::mem::size_of;
    use std::ptr::null_mut;

    use venecita_core::rules::app_matches;
    use venecita_platform::Platform;
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    let pid = venecita_platform::windows::WinPlatform::new()
        .windows()
        .ok()?
        .into_iter()
        .find(|w| app_matches(app_id, &w.app))?
        .pid;
    // SAFETY: handles are closed and GDI objects deleted below; buffer sizes are passed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len);
        windows_sys::Win32::Foundation::CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let mut large = null_mut();
        if ExtractIconExW(path.as_ptr(), 0, &mut large, null_mut(), 1) == 0 || large.is_null() {
            return None;
        }
        let mut info: ICONINFO = std::mem::zeroed();
        let got = GetIconInfo(large, &mut info) != 0;
        let mut result = None;
        if got && !info.hbmColor.is_null() {
            let mut bmp: BITMAP = std::mem::zeroed();
            GetObjectW(
                info.hbmColor,
                size_of::<BITMAP>() as i32,
                (&mut bmp as *mut BITMAP).cast(),
            );
            let (w, h) = (bmp.bmWidth.max(0) as u32, bmp.bmHeight.max(0) as u32);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(null_mut());
            let rows = GetDIBits(
                dc,
                info.hbmColor,
                0,
                h,
                bgra.as_mut_ptr().cast(),
                &mut bi,
                DIB_RGB_COLORS,
            );
            ReleaseDC(null_mut(), dc);
            if rows > 0 && w > 0 {
                // Old icons have no alpha channel: treat them as opaque.
                let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
                let rgba = bgra
                    .chunks_exact(4)
                    .flat_map(|p| [p[2], p[1], p[0], if has_alpha { p[3] } else { 255 }])
                    .collect();
                result = Some(AppIcon {
                    width: w,
                    height: h,
                    rgba,
                });
            }
        }
        if got {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        DestroyIcon(large);
        result
    }
}

/// Straight alpha from a premultiplied RGBA pixel.
#[cfg(target_os = "macos")]
fn unpremultiply(p: &[u8]) -> [u8; 4] {
    let a = p[3];
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let f = |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
    [f(p[0]), f(p[1]), f(p[2]), a]
}
