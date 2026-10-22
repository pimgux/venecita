//! Displays (monitors) and how the user's display definitions are matched to them.
//!
//! The configuration names displays with short ids (`"lg43"`, `"laptop"`) and describes
//! how to recognise each one with a [`DisplayMatcher`]. At runtime the platform backend
//! reports the connected displays as [`DisplayInfo`] and [`resolve_displays`] decides
//! which definition each connected display corresponds to.

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::grid::GridSize;

/// A connected display, as reported by the platform backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Opaque, OS-specific identifier, stable while the display stays connected.
    pub handle: String,
    /// Human readable name (e.g. `"LG HDR 4K"`, `"Built-in Retina Display"`).
    pub name: String,
    /// `true` for the laptop's internal panel.
    pub builtin: bool,
    /// EDID vendor number, if known.
    pub vendor: Option<u32>,
    /// EDID model number, if known.
    pub model: Option<u32>,
    /// EDID serial number, if known (often 0 or missing on cheap monitors).
    pub serial: Option<u32>,
    /// Full frame in global logical coordinates.
    pub frame: Rect,
    /// Usable area: the frame minus menu bar, dock or taskbar.
    pub work_area: Rect,
    /// Backing scale factor (2.0 on Retina, 1.5 for 150 % on Windows…).
    pub scale: f64,
}

impl DisplayInfo {
    /// Resolution in physical pixels, rounded.
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            (self.frame.w * self.scale).round() as u32,
            (self.frame.h * self.scale).round() as u32,
        )
    }
}

/// Describes how to recognise a display. Every field that is set must match; fields left
/// out are ignored. An empty matcher matches nothing (and is rejected by validation).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayMatcher {
    /// Match the laptop's internal panel (`true`) or only external displays (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<bool>,
    /// Case-insensitive substring of the display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// EDID vendor number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<u32>,
    /// EDID model number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<u32>,
    /// EDID serial number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<u32>,
    /// Horizontal resolution in physical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Vertical resolution in physical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl DisplayMatcher {
    /// Returns `true` if no criteria are set.
    pub fn is_empty(&self) -> bool {
        *self == DisplayMatcher::default()
    }

    /// Returns `true` if the matcher singles out a model of monitor (name, EDID ids or
    /// resolution) and not just a kind of display (`builtin`), so it is more specific than
    /// "any external display".
    pub fn is_specific(&self) -> bool {
        self.name.is_some()
            || self.vendor.is_some()
            || self.model.is_some()
            || self.serial.is_some()
            || self.width.is_some()
            || self.height.is_some()
    }

    /// Returns `true` if every criterion that is set matches `display`.
    pub fn matches(&self, display: &DisplayInfo) -> bool {
        if self.is_empty() {
            return false;
        }
        let (px_w, px_h) = display.pixel_size();
        let name_ok = self.name.as_ref().map_or(true, |n| {
            display.name.to_lowercase().contains(&n.to_lowercase())
        });
        name_ok
            && self.builtin.map_or(true, |b| b == display.builtin)
            && self.vendor.map_or(true, |v| Some(v) == display.vendor)
            && self.model.map_or(true, |m| Some(m) == display.model)
            && self.serial.map_or(true, |s| Some(s) == display.serial)
            && self.width.map_or(true, |w| w == px_w)
            && self.height.map_or(true, |h| h == px_h)
    }
}

/// A display definition from the configuration: a user-chosen id plus a matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayDef {
    /// Short id used by layouts and rules (e.g. `"lg43"`).
    pub id: String,
    /// How to recognise the display.
    #[serde(rename = "match")]
    pub matcher: DisplayMatcher,
    /// Grid for this display. Defaults to `general.grid`. Big monitors often deserve a
    /// finer grid (e.g. 8 × 8 on a 43" screen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridSize>,
}

/// A display definition matched to a connected display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDisplay {
    /// Id of the [`DisplayDef`].
    pub def_id: String,
    /// [`DisplayInfo::handle`] of the connected display.
    pub handle: String,
}

/// Matches display definitions to connected displays.
///
/// Definitions that name a monitor (see [`DisplayMatcher::is_specific`]) are processed
/// first, then the generic ones (`builtin = false`, "any external display"), each group in
/// configuration order. A generic definition therefore never takes a monitor that a
/// specific one is waiting for, wherever it sits in the file.
///
/// Each definition claims the **left-most** (then top-most) connected display that it
/// matches and that has not been claimed yet. This lets two identical monitors (same name,
/// no serial) be told apart: define `work_left` before `work_right` with the same matcher
/// and they are assigned by position.
///
/// Connected displays that match no definition are simply not part of the result.
pub fn resolve_displays(defs: &[DisplayDef], connected: &[DisplayInfo]) -> Vec<ResolvedDisplay> {
    let mut order: Vec<&DisplayInfo> = connected.iter().collect();
    order.sort_by(|a, b| {
        a.frame
            .x
            .total_cmp(&b.frame.x)
            .then(a.frame.y.total_cmp(&b.frame.y))
    });

    let specific = defs.iter().filter(|d| d.matcher.is_specific());
    let generic = defs.iter().filter(|d| !d.matcher.is_specific());
    let mut claimed = vec![false; order.len()];
    let mut out = Vec::new();
    for def in specific.chain(generic) {
        let found = order
            .iter()
            .enumerate()
            .find(|(i, d)| !claimed[*i] && def.matcher.matches(d));
        if let Some((i, d)) = found {
            claimed[i] = true;
            out.push(ResolvedDisplay {
                def_id: def.id.clone(),
                handle: d.handle.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a display for tests. `w`/`h` are logical points, scale 1.0.
    pub(crate) fn display(handle: &str, name: &str, builtin: bool, frame: Rect) -> DisplayInfo {
        DisplayInfo {
            handle: handle.into(),
            name: name.into(),
            builtin,
            vendor: None,
            model: None,
            serial: None,
            frame,
            work_area: frame,
            scale: 1.0,
        }
    }

    fn def(id: &str, matcher: DisplayMatcher) -> DisplayDef {
        DisplayDef {
            id: id.into(),
            matcher,
            grid: None,
        }
    }

    fn by_name(n: &str) -> DisplayMatcher {
        DisplayMatcher {
            name: Some(n.into()),
            ..Default::default()
        }
    }

    #[test]
    fn empty_matcher_matches_nothing() {
        let d = display("1", "Any", false, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(!DisplayMatcher::default().matches(&d));
    }

    #[test]
    fn name_is_case_insensitive_substring() {
        let d = display("1", "LG HDR 4K", false, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(by_name("lg hdr").matches(&d));
        assert!(!by_name("dell").matches(&d));
    }

    #[test]
    fn all_set_fields_must_match() {
        let mut d = display(
            "1",
            "DELL U2723QE",
            false,
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
        );
        d.scale = 2.0;
        d.serial = Some(42);
        let m = DisplayMatcher {
            name: Some("dell".into()),
            serial: Some(42),
            width: Some(3840),
            height: Some(2160),
            builtin: Some(false),
            ..Default::default()
        };
        assert!(m.matches(&d));
        let wrong_serial = DisplayMatcher {
            serial: Some(7),
            ..m.clone()
        };
        assert!(!wrong_serial.matches(&d));
        let wants_builtin = DisplayMatcher {
            builtin: Some(true),
            ..m
        };
        assert!(!wants_builtin.matches(&d));
    }

    #[test]
    fn identical_monitors_are_assigned_left_to_right() {
        // Connected in "wrong" order on purpose: the right monitor is reported first.
        let connected = vec![
            display(
                "B",
                "DELL U2723QE",
                false,
                Rect::new(2560.0, 0.0, 2560.0, 1440.0),
            ),
            display(
                "L",
                "Built-in Retina Display",
                true,
                Rect::new(-1512.0, 300.0, 1512.0, 982.0),
            ),
            display(
                "A",
                "DELL U2723QE",
                false,
                Rect::new(0.0, 0.0, 2560.0, 1440.0),
            ),
        ];
        let defs = vec![
            def("left27", by_name("DELL")),
            def("right27", by_name("DELL")),
            def(
                "laptop",
                DisplayMatcher {
                    builtin: Some(true),
                    ..Default::default()
                },
            ),
        ];
        let resolved = resolve_displays(&defs, &connected);
        let pairs: Vec<_> = resolved
            .iter()
            .map(|r| (r.def_id.as_str(), r.handle.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![("left27", "A"), ("right27", "B"), ("laptop", "L")]
        );
    }

    #[test]
    fn specific_definitions_win_over_a_generic_one_listed_first() {
        // The default config has a generic `external`; later, each Dell gets its own
        // definition. `external` must not grab a Dell the specific ones are waiting for.
        let connected = vec![
            display("L", "Built-in", true, Rect::new(0.0, 0.0, 1920.0, 1080.0)),
            display(
                "A",
                "DELL P2422HE",
                false,
                Rect::new(-1920.0, 0.0, 1920.0, 1080.0),
            ),
            display(
                "B",
                "DELL P2425HE",
                false,
                Rect::new(1920.0, 0.0, 1920.0, 1080.0),
            ),
        ];
        let external = DisplayMatcher {
            builtin: Some(false),
            ..Default::default()
        };
        let defs = vec![
            def("external", external),
            def("dell_a", by_name("P2422HE")),
            def("dell_b", by_name("P2425HE")),
        ];
        let resolved = resolve_displays(&defs, &connected);
        let mut pairs: Vec<_> = resolved
            .iter()
            .map(|r| (r.def_id.as_str(), r.handle.as_str()))
            .collect();
        pairs.sort();
        // Both Dells are found by name; with nothing left, `external` stays unresolved.
        assert_eq!(pairs, vec![("dell_a", "A"), ("dell_b", "B")]);
    }

    #[test]
    fn a_generic_external_takes_what_is_left() {
        let connected = vec![
            display("A", "DELL P2422HE", false, Rect::new(0.0, 0.0, 10.0, 10.0)),
            display("B", "Other", false, Rect::new(10.0, 0.0, 10.0, 10.0)),
        ];
        let external = DisplayMatcher {
            builtin: Some(false),
            ..Default::default()
        };
        let defs = vec![def("external", external), def("dell", by_name("DELL"))];
        let pairs: Vec<_> = resolve_displays(&defs, &connected)
            .into_iter()
            .map(|r| (r.def_id, r.handle))
            .collect();
        assert!(pairs.contains(&("dell".to_string(), "A".to_string())));
        assert!(pairs.contains(&("external".to_string(), "B".to_string())));
    }

    #[test]
    fn missing_displays_are_skipped() {
        // Laptop lid closed: only the 43" is connected.
        let connected = vec![display(
            "X",
            "LG HDR 4K",
            false,
            Rect::new(0.0, 0.0, 3840.0, 2160.0),
        )];
        let defs = vec![
            def("lg43", by_name("LG HDR 4K")),
            def(
                "laptop",
                DisplayMatcher {
                    builtin: Some(true),
                    ..Default::default()
                },
            ),
        ];
        let resolved = resolve_displays(&defs, &connected);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].def_id, "lg43");
    }
}
