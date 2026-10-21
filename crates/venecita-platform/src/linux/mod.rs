//! Linux backend for X11, with any EWMH window manager (GNOME/Mutter and KDE/KWin on Xorg,
//! Xfwm, Openbox, i3…).
//!
//! * Windows come from `_NET_CLIENT_LIST`; the app is the `WM_CLASS` class (`firefox`,
//!   `Code`), the kind comes from `_NET_WM_WINDOW_TYPE` and `_NET_WM_STATE`.
//! * Frames include the window manager's decorations (`_NET_FRAME_EXTENTS`), and windows are
//!   moved with `_NET_MOVERESIZE_WINDOW` (falling back to a plain configure request).
//! * Displays come from RandR, named from their EDID when it has a name.
//! * Events come from a thread with its own connection ([`events`]).
//!
//! Wayland is not supported: it lets apps position windows only through compositor-specific
//! APIs. Tab bars and the dim layer are not drawn on Linux yet.

mod events;
mod login;

use std::sync::Mutex;

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

use crate::{
    AppIdentity, DisplayInfo, EventSink, PermissionStatus, Pid, Platform, PlatformError, Rect,
    Result, WindowId, WindowInfo, WindowKind,
};

x11rb::atom_manager! {
    /// The atoms the backend uses.
    pub(crate) Atoms: AtomsCookie {
        _NET_SUPPORTED,
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        _NET_MOVERESIZE_WINDOW,
        _NET_WORKAREA,
        _NET_FRAME_EXTENTS,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_TOOLBAR,
        _NET_WM_WINDOW_TYPE_MENU,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WM_WINDOW_TYPE_SPLASH,
        _NET_WM_WINDOW_TYPE_DIALOG,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_STATE,
        _NET_WM_STATE_MODAL,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_WM_STATE_SKIP_TASKBAR,
        UTF8_STRING,
        EDID,
    }
}

/// An X connection with the root window and the atoms.
pub(crate) struct X11 {
    pub(crate) conn: RustConnection,
    pub(crate) root: Window,
    pub(crate) atoms: Atoms,
}

fn x_error(operation: &'static str) -> impl Fn(Box<dyn std::error::Error>) -> PlatformError {
    move |e| {
        tracing::debug!(operation, error = %e, "X11 request failed");
        PlatformError::Os { operation, code: 0 }
    }
}

impl X11 {
    /// Connects to the display in `$DISPLAY`.
    pub(crate) fn connect() -> Result<Self> {
        let (conn, screen) =
            x11rb::connect(None).map_err(|e| x_error("connect to the X server")(Box::new(e)))?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)
            .map_err(|e| x_error("intern atoms")(Box::new(e)))?
            .reply()
            .map_err(|e| x_error("intern atoms")(Box::new(e)))?;
        Ok(Self { conn, root, atoms })
    }

    /// A property as 32-bit values (empty if missing).
    pub(crate) fn prop32(&self, window: Window, property: u32) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, AtomEnum::ANY, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    /// A property as bytes (empty if missing).
    fn prop_bytes(&self, window: Window, property: u32) -> Vec<u8> {
        self.conn
            .get_property(false, window, property, AtomEnum::ANY, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.value)
            .unwrap_or_default()
    }

    /// The managed top-level windows, in stacking order of the window manager's list.
    pub(crate) fn client_list(&self) -> Vec<Window> {
        self.prop32(self.root, self.atoms._NET_CLIENT_LIST)
    }

    pub(crate) fn active_window(&self) -> Option<Window> {
        self.prop32(self.root, self.atoms._NET_ACTIVE_WINDOW)
            .first()
            .copied()
            .filter(|&w| w != 0)
    }

    fn supports(&self, atom: u32) -> bool {
        self.prop32(self.root, self.atoms._NET_SUPPORTED)
            .contains(&atom)
    }

    /// Details of a window, or `None` for windows Venecita doesn't list (docks, desktops,
    /// menus, notifications, windows kept off the taskbar).
    pub(crate) fn info(&self, window: Window) -> Option<WindowInfo> {
        let kind = self.kind(window)?;
        let frame = self.frame(window)?;
        let pid = self
            .prop32(window, self.atoms._NET_WM_PID)
            .first()
            .copied()
            .unwrap_or(0) as Pid;
        Some(WindowInfo {
            id: WindowId(u64::from(window)),
            pid,
            app: self.app(window),
            title: self.title(window),
            frame,
            kind,
        })
    }

    fn kind(&self, window: Window) -> Option<WindowKind> {
        let a = &self.atoms;
        let types = self.prop32(window, a._NET_WM_WINDOW_TYPE);
        let states = self.prop32(window, a._NET_WM_STATE);
        let skip = [
            a._NET_WM_WINDOW_TYPE_DESKTOP,
            a._NET_WM_WINDOW_TYPE_DOCK,
            a._NET_WM_WINDOW_TYPE_MENU,
            a._NET_WM_WINDOW_TYPE_NOTIFICATION,
        ];
        if types.iter().any(|t| skip.contains(t)) || states.contains(&a._NET_WM_STATE_SKIP_TASKBAR)
        {
            return None;
        }
        if states.contains(&a._NET_WM_STATE_HIDDEN) {
            return Some(WindowKind::Minimized);
        }
        if states.contains(&a._NET_WM_STATE_FULLSCREEN) {
            return Some(WindowKind::Fullscreen);
        }
        let transient = !self
            .prop32(window, u32::from(AtomEnum::WM_TRANSIENT_FOR))
            .is_empty();
        if types.contains(&a._NET_WM_WINDOW_TYPE_DIALOG)
            || states.contains(&a._NET_WM_STATE_MODAL)
            || transient
        {
            return Some(WindowKind::Dialog);
        }
        let floating = [
            a._NET_WM_WINDOW_TYPE_UTILITY,
            a._NET_WM_WINDOW_TYPE_TOOLBAR,
            a._NET_WM_WINDOW_TYPE_SPLASH,
        ];
        if types.iter().any(|t| floating.contains(t)) {
            return Some(WindowKind::Floating);
        }
        Some(WindowKind::Normal)
    }

    /// `WM_CLASS` is "instance\0class\0": the class names the app (`firefox`, `Code`).
    fn app(&self, window: Window) -> AppIdentity {
        let raw = self.prop_bytes(window, u32::from(AtomEnum::WM_CLASS));
        let mut parts = raw
            .split(|&b| b == 0)
            .map(|p| String::from_utf8_lossy(p).into_owned());
        let instance = parts.next().unwrap_or_default();
        let class = parts.next().filter(|c| !c.is_empty()).unwrap_or(instance);
        AppIdentity::new(class.clone(), class)
    }

    fn title(&self, window: Window) -> String {
        let utf8 = self.prop_bytes(window, self.atoms._NET_WM_NAME);
        let raw = if utf8.is_empty() {
            self.prop_bytes(window, u32::from(AtomEnum::WM_NAME))
        } else {
            utf8
        };
        String::from_utf8_lossy(&raw).into_owned()
    }

    /// The window manager's decorations around the window: left, right, top, bottom.
    fn extents(&self, window: Window) -> [f64; 4] {
        let e = self.prop32(window, self.atoms._NET_FRAME_EXTENTS);
        if e.len() >= 4 {
            [e[0], e[1], e[2], e[3]].map(f64::from)
        } else {
            [0.0; 4]
        }
    }

    /// What the user sees of the window: its client area plus the decorations.
    pub(crate) fn frame(&self, window: Window) -> Option<Rect> {
        let geometry = self.conn.get_geometry(window).ok()?.reply().ok()?;
        let origin = self
            .conn
            .translate_coordinates(window, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let [left, right, top, bottom] = self.extents(window);
        Some(Rect::new(
            f64::from(origin.dst_x) - left,
            f64::from(origin.dst_y) - top,
            f64::from(geometry.width) + left + right,
            f64::from(geometry.height) + top + bottom,
        ))
    }

    /// Sends an EWMH client message about `window` to the window manager.
    fn client_message(&self, window: Window, kind: u32, data: [u32; 5]) -> Result<()> {
        let event = ClientMessageEvent::new(32, window, kind, data);
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|e| x_error("send a message to the window manager")(Box::new(e)))?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.conn
            .flush()
            .map_err(|e| x_error("flush the X connection")(Box::new(e)))
    }
}

/// "Source indication" of EWMH messages: we act like a pager (on behalf of the user).
const SOURCE_PAGER: u32 = 2;

/// The X11 implementation of [`Platform`].
pub struct X11Platform {
    x: X11,
    events: Mutex<Option<events::EventThread>>,
}

impl X11Platform {
    /// Connects to the X server in `$DISPLAY`.
    pub fn connect() -> Result<Self> {
        Ok(Self {
            x: X11::connect()?,
            events: Mutex::new(None),
        })
    }
}

fn window_of(id: WindowId) -> Window {
    id.0 as Window
}

impl Platform for X11Platform {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(displays(&self.x))
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(self
            .x
            .client_list()
            .into_iter()
            .filter_map(|w| self.x.info(w))
            .collect())
    }

    fn window(&self, id: WindowId) -> Result<WindowInfo> {
        let window = window_of(id);
        if !self.x.client_list().contains(&window) {
            return Err(PlatformError::WindowNotFound(id));
        }
        self.x.info(window).ok_or(PlatformError::WindowNotFound(id))
    }

    fn focused_window(&self) -> Result<Option<WindowId>> {
        Ok(self
            .x
            .active_window()
            .filter(|&w| self.x.info(w).is_some())
            .map(|w| WindowId(u64::from(w))))
    }

    fn set_frame(&self, id: WindowId, frame: Rect) -> Result<()> {
        let window = window_of(id);
        let a = &self.x.atoms;
        // Window managers ignore moves of maximized windows: un-maximize first.
        self.x.client_message(
            window,
            a._NET_WM_STATE,
            [
                0, // remove
                a._NET_WM_STATE_MAXIMIZED_VERT,
                a._NET_WM_STATE_MAXIMIZED_HORZ,
                SOURCE_PAGER,
                0,
            ],
        )?;
        let [left, right, top, bottom] = self.x.extents(window);
        let width = (frame.w - left - right).round().max(1.0) as u32;
        let height = (frame.h - top - bottom).round().max(1.0) as u32;
        if self.x.supports(a._NET_MOVERESIZE_WINDOW) {
            // NorthWest gravity: x/y is the top-left of the decorated frame.
            const NORTH_WEST: u32 = 1;
            const XYWH: u32 = 0b1111 << 8;
            let flags = NORTH_WEST | XYWH | (SOURCE_PAGER << 12);
            self.x.client_message(
                window,
                a._NET_MOVERESIZE_WINDOW,
                [
                    flags,
                    frame.x.round() as i32 as u32,
                    frame.y.round() as i32 as u32,
                    width,
                    height,
                ],
            )?;
        } else {
            let aux = ConfigureWindowAux::new()
                .x((frame.x + left).round() as i32)
                .y((frame.y + top).round() as i32)
                .width(width)
                .height(height);
            self.x
                .conn
                .configure_window(window, &aux)
                .map_err(|e| x_error("configure a window")(Box::new(e)))?;
        }
        self.x.flush()
    }

    fn focus(&self, id: WindowId) -> Result<()> {
        // Also restores a minimized window in EWMH window managers.
        self.x.client_message(
            window_of(id),
            self.x.atoms._NET_ACTIVE_WINDOW,
            [SOURCE_PAGER, 0, 0, 0, 0],
        )?;
        self.x.flush()
    }

    fn start_events(&self, sink: EventSink) -> Result<()> {
        let mut slot = self.events.lock().unwrap_or_else(|e| e.into_inner());
        slot.take();
        *slot = Some(events::EventThread::start(sink)?);
        Ok(())
    }

    fn stop_events(&self) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn permissions(&self, _prompt: bool) -> PermissionStatus {
        PermissionStatus::Granted
    }

    fn set_launch_at_login(&self, enabled: bool) -> Result<()> {
        login::set(enabled)
    }
}

/// The active displays: one per RandR CRTC in use (mirrored outputs share one), or the
/// whole screen when RandR has nothing to say (e.g. a virtual X server).
fn displays(x: &X11) -> Vec<DisplayInfo> {
    let workarea = {
        let w = x.prop32(x.root, x.atoms._NET_WORKAREA);
        (w.len() >= 4).then(|| {
            Rect::new(
                f64::from(w[0] as i32),
                f64::from(w[1] as i32),
                f64::from(w[2]),
                f64::from(w[3]),
            )
        })
    };
    let work_area = |frame: Rect| {
        workarea
            .and_then(|wa| wa.intersection(&frame))
            .unwrap_or(frame)
    };

    let mut out = Vec::new();
    if let Some(resources) = x
        .conn
        .randr_get_screen_resources_current(x.root)
        .ok()
        .and_then(|c| c.reply().ok())
    {
        for crtc in &resources.crtcs {
            let Some(info) = x
                .conn
                .randr_get_crtc_info(*crtc, resources.config_timestamp)
                .ok()
                .and_then(|c| c.reply().ok())
            else {
                continue;
            };
            let Some(&output) = info.outputs.first() else {
                continue;
            };
            if info.mode == 0 || info.width == 0 || info.height == 0 {
                continue;
            }
            let output_name = x
                .conn
                .randr_get_output_info(output, resources.config_timestamp)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|o| String::from_utf8_lossy(&o.name).into_owned())
                .unwrap_or_else(|| format!("output-{output}"));
            let edid = x
                .conn
                .randr_get_output_property(output, x.atoms.EDID, AtomEnum::ANY, 0, 64, false, false)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| parse_edid(&r.data))
                .unwrap_or_default();
            let frame = Rect::new(
                f64::from(info.x),
                f64::from(info.y),
                f64::from(info.width),
                f64::from(info.height),
            );
            let builtin = ["eDP", "LVDS", "DSI"]
                .iter()
                .any(|p| output_name.starts_with(p));
            out.push(DisplayInfo {
                handle: output_name.clone(),
                name: edid.name.unwrap_or(output_name),
                builtin,
                vendor: edid.vendor,
                model: edid.model,
                serial: edid.serial,
                frame,
                work_area: work_area(frame),
                scale: 1.0,
            });
        }
    }
    if out.is_empty() {
        let screen = &x.conn.setup().roots[0];
        let frame = Rect::new(
            0.0,
            0.0,
            f64::from(screen.width_in_pixels),
            f64::from(screen.height_in_pixels),
        );
        out.push(DisplayInfo {
            handle: "screen".into(),
            name: "Screen".into(),
            builtin: false,
            vendor: None,
            model: None,
            serial: None,
            frame,
            work_area: work_area(frame),
            scale: 1.0,
        });
    }
    out
}

/// What an EDID block says about a monitor.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Edid {
    pub(crate) name: Option<String>,
    pub(crate) vendor: Option<u32>,
    pub(crate) model: Option<u32>,
    pub(crate) serial: Option<u32>,
}

/// Parses the base EDID block: manufacturer and product ids, serial number, and the
/// monitor name descriptor (`0xFC`).
pub(crate) fn parse_edid(edid: &[u8]) -> Edid {
    const HEADER: [u8; 8] = [0, 255, 255, 255, 255, 255, 255, 0];
    if edid.len() < 128 || edid[..8] != HEADER {
        return Edid::default();
    }
    let nonzero = |v: u32| (v != 0).then_some(v);
    let name = (0..4).find_map(|i| {
        let d = &edid[54 + 18 * i..72 + 18 * i];
        (d[..3] == [0, 0, 0] && d[3] == 0xFC).then(|| {
            let text: Vec<u8> = d[5..].iter().copied().take_while(|&b| b != 0x0A).collect();
            String::from_utf8_lossy(&text).trim().to_string()
        })
    });
    Edid {
        name: name.filter(|n| !n.is_empty()),
        vendor: nonzero(u32::from(u16::from_be_bytes([edid[8], edid[9]]))),
        model: nonzero(u32::from(u16::from_le_bytes([edid[10], edid[11]]))),
        serial: nonzero(u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up EDID for "DELL U2723QE": manufacturer DEL, product 0x4321, serial 1234.
    fn sample_edid() -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
        e[8..10].copy_from_slice(&0x10ACu16.to_be_bytes()); // "DEL"
        e[10..12].copy_from_slice(&0x4321u16.to_le_bytes());
        e[12..16].copy_from_slice(&1234u32.to_le_bytes());
        // Descriptor 2 (offset 72): monitor name.
        e[72..77].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        let name = b"DELL U2723QE\n";
        e[77..77 + name.len()].copy_from_slice(name);
        e
    }

    #[test]
    fn edid_names_and_ids() {
        let edid = parse_edid(&sample_edid());
        assert_eq!(edid.name.as_deref(), Some("DELL U2723QE"));
        assert_eq!(edid.vendor, Some(0x10AC));
        assert_eq!(edid.model, Some(0x4321));
        assert_eq!(edid.serial, Some(1234));
    }

    #[test]
    fn bad_edid_is_ignored() {
        assert_eq!(parse_edid(&[]), Edid::default());
        assert_eq!(parse_edid(&[0u8; 128]), Edid::default());
    }
}
