//! Choosing which profile and layout apply to the connected displays.
//!
//! A layout *fits* when every display it lists is connected. Among the fitting layouts of
//! a profile the one that uses the most displays wins, so "Laptop open" (43" + laptop) is
//! preferred over "Laptop closed" (43" only) as soon as the laptop screen is on.
//!
//! The profile is the one the user chose by hand (if it has a fitting layout) or else the
//! first profile in the configuration that has one.

use std::collections::HashSet;

use crate::config::{Layout, Profile};

/// Identifies a layout inside the configuration by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutRef {
    /// Index into `Config::profiles`.
    pub profile: usize,
    /// Index into `Profile::layouts`.
    pub layout: usize,
}

impl LayoutRef {
    /// Looks up the profile and layout. Returns `None` if the indices are stale.
    pub fn get<'a>(&self, profiles: &'a [Profile]) -> Option<(&'a Profile, &'a Layout)> {
        let profile = profiles.get(self.profile)?;
        Some((profile, profile.layouts.get(self.layout)?))
    }
}

/// The best fitting layout of one profile, if any.
pub fn best_layout(profile: &Profile, connected: &HashSet<&str>) -> Option<usize> {
    profile
        .layouts
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            !l.displays.is_empty() && l.displays.iter().all(|d| connected.contains(d.as_str()))
        })
        // `max_by_key` returns the *last* maximum; reverse so the first one in config order wins ties.
        .rev()
        .max_by_key(|(_, l)| l.displays.len())
        .map(|(i, _)| i)
}

/// Indices of every profile that has a fitting layout, in configuration order.
pub fn fitting_profiles(profiles: &[Profile], connected: &HashSet<&str>) -> Vec<usize> {
    profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| best_layout(p, connected).is_some())
        .map(|(i, _)| i)
        .collect()
}

/// Selects the layout to use.
///
/// * `connected`: ids of the display definitions that are currently connected (see
///   [`crate::display::resolve_displays`]).
/// * `preferred`: the profile chosen by hand, if any. Ignored if it doesn't fit.
pub fn select_layout(
    profiles: &[Profile],
    connected: &HashSet<&str>,
    preferred: Option<&str>,
) -> Option<LayoutRef> {
    let preferred_idx = preferred.and_then(|name| {
        profiles
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
    });
    preferred_idx
        .into_iter()
        .chain(0..profiles.len())
        .find_map(|pi| {
            best_layout(&profiles[pi], connected).map(|li| LayoutRef {
                profile: pi,
                layout: li,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn example() -> Config {
        Config::from_toml_str(include_str!("../../../examples/config.toml")).unwrap()
    }

    fn names(config: &Config, r: Option<LayoutRef>) -> Option<(String, String)> {
        let (p, l) = r?.get(&config.profiles)?;
        Some((p.name.clone(), l.name.clone()))
    }

    fn set<'a>(ids: &[&'a str]) -> HashSet<&'a str> {
        ids.iter().copied().collect()
    }

    fn pair(p: &str, l: &str) -> Option<(String, String)> {
        Some((p.into(), l.into()))
    }

    #[test]
    fn home_with_laptop_open_and_closed() {
        let c = example();
        let open = select_layout(&c.profiles, &set(&["lg43", "laptop"]), None);
        assert_eq!(names(&c, open), pair("Casa", "Notebook abierta"));
        let closed = select_layout(&c.profiles, &set(&["lg43"]), None);
        assert_eq!(names(&c, closed), pair("Casa", "Notebook cerrada"));
    }

    #[test]
    fn work_with_laptop_open_and_closed() {
        let c = example();
        let open = select_layout(
            &c.profiles,
            &set(&["work_left", "work_right", "laptop"]),
            None,
        );
        assert_eq!(names(&c, open), pair("Trabajo", "Notebook abierta"));
        let closed = select_layout(&c.profiles, &set(&["work_left", "work_right"]), None);
        assert_eq!(names(&c, closed), pair("Trabajo", "Notebook cerrada"));
    }

    #[test]
    fn laptop_alone_uses_travel_profile() {
        let c = example();
        let r = select_layout(&c.profiles, &set(&["laptop"]), None);
        assert_eq!(names(&c, r), pair("Viaje", "Solo notebook"));
    }

    #[test]
    fn preferred_profile_wins_when_it_fits() {
        let c = example();
        let connected = set(&["lg43", "laptop"]);
        // "Viaje" also fits (it only needs the laptop) and is chosen by hand.
        let r = select_layout(&c.profiles, &connected, Some("viaje"));
        assert_eq!(names(&c, r), pair("Viaje", "Solo notebook"));
        // "Trabajo" doesn't fit, so the automatic choice is used.
        let r = select_layout(&c.profiles, &connected, Some("Trabajo"));
        assert_eq!(names(&c, r), pair("Casa", "Notebook abierta"));
    }

    #[test]
    fn nothing_fits() {
        let c = example();
        assert_eq!(select_layout(&c.profiles, &set(&[]), None), None);
        assert_eq!(select_layout(&[], &set(&["laptop"]), None), None);
    }

    #[test]
    fn fitting_profiles_in_config_order() {
        let c = example();
        let fit = fitting_profiles(&c.profiles, &set(&["lg43", "laptop"]));
        let names: Vec<_> = fit.iter().map(|&i| c.profiles[i].name.as_str()).collect();
        assert_eq!(names, vec!["Casa", "Viaje"]);
    }

    #[test]
    fn ties_go_to_the_first_layout() {
        let c = Config::from_toml_str(
            r#"
            [[displays]]
            id = "a"
            match = { builtin = true }
            [[profiles]]
            name = "P"
            [[profiles.layouts]]
            name = "first"
            displays = ["a"]
            [[profiles.layouts]]
            name = "second"
            displays = ["a"]
            "#,
        )
        .unwrap();
        let r = select_layout(&c.profiles, &set(&["a"]), None);
        assert_eq!(names(&c, r), pair("P", "first"));
    }
}
