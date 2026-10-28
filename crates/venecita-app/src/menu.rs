//! The tray menu as plain data.
//!
//! Building the menu is separated from drawing it so its contents can be unit tested;
//! `tray.rs` turns these entries into real menu items.

use std::collections::BTreeMap;

use venecita_core::action::ActionSpec;
use venecita_core::engine::Status;
use venecita_core::Action;

use crate::icons::IconKind;

use crate::i18n::{tr, Lang, Text};

/// Identifiers of menu commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    /// Choose a profile by hand.
    Profile(String),
    /// Turn automatic profile detection on or off (see [`toggle_auto_profile`]).
    ToggleAutoProfile,
    /// Re-arrange every window.
    RelayoutAll,
    /// Pause/resume.
    TogglePause,
    /// Turn automatic arranging on or off (`general.auto_arrange`).
    ToggleAutoArrange,
    /// Toggle launch at login.
    ToggleLaunchAtLogin,
    /// Open the settings window.
    OpenSettings,
    /// Open the configuration file in a text editor.
    OpenConfig,
    /// Reload the configuration file.
    ReloadConfig,
    /// Open the logs folder.
    ShowLogs,
    /// Open the Accessibility settings.
    OpenPermissionSettings,
    /// Quit.
    Quit,
}

impl MenuCommand {
    /// Stable string id used by the native menu.
    pub fn id(&self) -> String {
        match self {
            MenuCommand::Profile(name) => format!("profile:{name}"),
            other => format!("{other:?}"),
        }
    }

    /// Inverse of [`MenuCommand::id`].
    pub fn from_id(id: &str) -> Option<MenuCommand> {
        if let Some(name) = id.strip_prefix("profile:") {
            return Some(MenuCommand::Profile(name.to_string()));
        }
        use MenuCommand::*;
        [
            ToggleAutoProfile,
            RelayoutAll,
            TogglePause,
            ToggleAutoArrange,
            ToggleLaunchAtLogin,
            OpenSettings,
            OpenConfig,
            ReloadConfig,
            ShowLogs,
            OpenPermissionSettings,
            Quit,
        ]
        .into_iter()
        .find(|c| c.id() == id)
    }
}

/// What the "Automatic (by displays)" check item does when clicked.
///
/// * On → off: the current profile is kept from now on (chosen by hand), so connecting
///   or disconnecting displays no longer switches profiles. Its layouts still follow the
///   displays (lid open / closed), and if it doesn't fit at all another one is used until
///   it fits again.
/// * Off → on: profiles follow the connected displays again.
///
/// Returns `None` when automatic mode is on but no profile fits the displays: there is
/// nothing to keep.
pub fn toggle_auto_profile(status: &Status) -> Option<Action> {
    if status.manual {
        Some(Action::AutoProfile)
    } else {
        status
            .profile
            .clone()
            .map(|name| Action::ActivateProfile { name })
    }
}

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// Informational text (disabled).
    Label(String),
    /// A clickable item.
    Item {
        /// Text.
        text: String,
        /// What it does.
        command: MenuCommand,
        /// Whether it can be clicked.
        enabled: bool,
        /// Colored icon shown before the text.
        icon: Option<IconKind>,
        /// The keyboard shortcut for the same action (shown greyed on the right), in the
        /// config syntax (`ctrl+alt+r`).
        shortcut: Option<String>,
    },
    /// An item with a check mark.
    Check {
        /// Text.
        text: String,
        /// What it does.
        command: MenuCommand,
        /// Check mark state.
        checked: bool,
        /// Whether it can be clicked.
        enabled: bool,
        /// The keyboard shortcut for the same action, if any.
        shortcut: Option<String>,
    },
    /// A submenu.
    Submenu {
        /// Text.
        text: String,
        /// Colored icon shown before the text.
        icon: Option<IconKind>,
        /// Its entries.
        entries: Vec<MenuEntry>,
    },
    /// A separator line.
    Separator,
}

/// Extra state the menu shows besides the engine status.
#[derive(Debug, Clone, Default)]
pub struct AppFlags {
    /// The Accessibility permission is missing.
    pub permission_missing: bool,
    /// The last config load failed.
    pub config_error: bool,
    /// Launch at login is enabled.
    pub launch_at_login: bool,
    /// Windows moved away are put back automatically (`general.auto_arrange`).
    pub auto_arrange: bool,
    /// Shortcuts that could not be registered (taken by another app).
    pub hotkey_conflicts: Vec<String>,
    /// No layout has a rule yet: placement is paused until the user sets up their zones.
    pub setup_pending: bool,
    /// Why the local API, turned on in the config, isn't running.
    pub api_error: Option<String>,
}

/// The first shortcut bound to `action` in the `[hotkeys]` table.
pub fn shortcut_for(hotkeys: &BTreeMap<String, ActionSpec>, action: &Action) -> Option<String> {
    hotkeys
        .iter()
        .find(|(_, spec)| spec.0 == *action)
        .map(|(key, _)| key.clone())
}

/// Builds the menu:
///
/// ```text
/// (warnings, if any)
/// Profile: Casa – Notebook abierta  ▸  ✓ Automatic · Casa · Trabajo …
/// Arrange windows                      ⌃⌥R
/// ✓ Arrange automatically
/// Pause / Resume                       ⌃⌥⇧P
/// ───
/// Settings…
/// Quit
/// ```
pub fn build(
    status: &Status,
    flags: &AppFlags,
    lang: Lang,
    hotkeys: &BTreeMap<String, ActionSpec>,
) -> Vec<MenuEntry> {
    use MenuEntry::*;
    let t = |k| tr(lang, k).to_string();
    let key = |action: Action| shortcut_for(hotkeys, &action);
    let mut m = Vec::new();

    if flags.permission_missing {
        m.push(Item {
            text: t(Text::PermissionMissing),
            command: MenuCommand::OpenPermissionSettings,
            enabled: true,
            icon: Some(IconKind::Warning),
            shortcut: None,
        });
    }
    if flags.setup_pending {
        m.push(Item {
            text: t(Text::SetupPending),
            command: MenuCommand::OpenSettings,
            enabled: true,
            icon: Some(IconKind::Warning),
            shortcut: None,
        });
    }
    if flags.config_error {
        m.push(Item {
            text: t(Text::ConfigError),
            command: MenuCommand::ShowLogs,
            enabled: true,
            icon: Some(IconKind::Warning),
            shortcut: None,
        });
    }
    if let Some(e) = &flags.api_error {
        m.push(Item {
            text: format!("{} {e}", t(Text::ApiError)),
            command: MenuCommand::ShowLogs,
            enabled: true,
            icon: Some(IconKind::Warning),
            shortcut: None,
        });
    }
    if !flags.hotkey_conflicts.is_empty() {
        m.push(Label(format!(
            "{} {}",
            t(Text::HotkeysUnavailable),
            flags.hotkey_conflicts.join(", ")
        )));
    }
    if !m.is_empty() {
        m.push(Separator);
    }

    // Profile: the active one in the title, the choice inside.
    let title = match (&status.profile, &status.layout) {
        (Some(p), Some(l)) => {
            let manual = if status.manual {
                format!(" {}", t(Text::ProfileManualSuffix))
            } else {
                String::new()
            };
            format!("{}: {p} – {l}{manual}", t(Text::ProfileActive))
        }
        _ => t(Text::NoProfile),
    };
    let mut profiles = vec![
        Check {
            text: t(Text::Automatic),
            command: MenuCommand::ToggleAutoProfile,
            checked: !status.manual,
            enabled: true,
            shortcut: key(Action::AutoProfile),
        },
        Separator,
    ];
    for p in &status.profiles {
        profiles.push(Check {
            text: p.name.clone(),
            command: MenuCommand::Profile(p.name.clone()),
            checked: p.active,
            enabled: p.fits,
            shortcut: key(Action::ActivateProfile {
                name: p.name.clone(),
            }),
        });
    }
    m.push(Submenu {
        text: title,
        icon: Some(IconKind::Profile),
        entries: profiles,
    });

    m.push(Item {
        text: t(Text::Arrange),
        command: MenuCommand::RelayoutAll,
        enabled: !status.paused,
        icon: Some(IconKind::Arrange),
        shortcut: key(Action::RelayoutAll),
    });
    m.push(Check {
        text: t(Text::AutoArrange),
        command: MenuCommand::ToggleAutoArrange,
        checked: flags.auto_arrange,
        enabled: true,
        shortcut: None,
    });
    let (pause_text, pause_icon) = if status.paused {
        (Text::Resume, IconKind::Resume)
    } else {
        (Text::Pause, IconKind::Pause)
    };
    m.push(Item {
        text: t(pause_text),
        command: MenuCommand::TogglePause,
        enabled: true,
        icon: Some(pause_icon),
        shortcut: key(Action::TogglePause),
    });
    m.push(Separator);
    m.push(Item {
        text: t(Text::Settings),
        command: MenuCommand::OpenSettings,
        enabled: true,
        icon: Some(IconKind::Settings),
        shortcut: key(Action::OpenSettings),
    });
    m.push(Item {
        text: t(Text::Quit),
        command: MenuCommand::Quit,
        enabled: true,
        icon: Some(IconKind::Quit),
        shortcut: None,
    });
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use venecita_core::config::default_hotkeys;
    use venecita_core::engine::ProfileStatus;

    fn status() -> Status {
        Status {
            profile: Some("Casa".into()),
            layout: Some("Notebook abierta".into()),
            manual: false,
            paused: false,
            profiles: vec![
                ProfileStatus {
                    name: "Casa".into(),
                    fits: true,
                    active: true,
                },
                ProfileStatus {
                    name: "Trabajo".into(),
                    fits: false,
                    active: false,
                },
            ],
            windows: 3,
            unused_displays: Vec::new(),
        }
    }

    fn menu(s: &Status, flags: &AppFlags, lang: Lang) -> Vec<MenuEntry> {
        build(s, flags, lang, &default_hotkeys())
    }

    fn commands(entries: &[MenuEntry]) -> Vec<MenuCommand> {
        entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item { command, .. } | MenuEntry::Check { command, .. } => {
                    Some(command.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn profile_submenu(entries: &[MenuEntry]) -> (&String, &Vec<MenuEntry>) {
        entries
            .iter()
            .find_map(|e| match e {
                MenuEntry::Submenu { text, entries, .. } => Some((text, entries)),
                _ => None,
            })
            .expect("a profile submenu")
    }

    #[test]
    fn command_ids_round_trip() {
        for c in [
            MenuCommand::Profile("Casa Grande".into()),
            MenuCommand::Quit,
            MenuCommand::ToggleLaunchAtLogin,
            MenuCommand::OpenSettings,
            MenuCommand::ToggleAutoArrange,
        ] {
            assert_eq!(MenuCommand::from_id(&c.id()), Some(c));
        }
        assert_eq!(MenuCommand::from_id("nope"), None);
    }

    #[test]
    fn the_menu_has_only_the_essentials() {
        let m = menu(&status(), &AppFlags::default(), Lang::Es);
        assert_eq!(
            commands(&m),
            vec![
                MenuCommand::RelayoutAll,
                MenuCommand::ToggleAutoArrange,
                MenuCommand::TogglePause,
                MenuCommand::OpenSettings,
                MenuCommand::Quit,
            ]
        );
        assert!(matches!(
            &m[0],
            MenuEntry::Submenu {
                icon: Some(IconKind::Profile),
                ..
            }
        ));
    }

    #[test]
    fn profiles_are_chosen_in_a_submenu() {
        let m = menu(&status(), &AppFlags::default(), Lang::Es);
        let (title, entries) = profile_submenu(&m);
        assert_eq!(title, "Perfil: Casa – Notebook abierta");
        assert!(matches!(
            &entries[0],
            MenuEntry::Check {
                command: MenuCommand::ToggleAutoProfile,
                checked: true,
                ..
            }
        ));
        let trabajo = entries
            .iter()
            .find(|e| matches!(e, MenuEntry::Check { command: MenuCommand::Profile(n), .. } if n == "Trabajo"))
            .unwrap();
        assert!(matches!(
            trabajo,
            MenuEntry::Check {
                enabled: false,
                checked: false,
                ..
            }
        ));
    }

    #[test]
    fn items_show_their_shortcuts() {
        let m = menu(&status(), &AppFlags::default(), Lang::En);
        let arrange = m
            .iter()
            .find(|e| {
                matches!(
                    e,
                    MenuEntry::Item {
                        command: MenuCommand::RelayoutAll,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(matches!(arrange, MenuEntry::Item { shortcut: Some(k), .. } if k == "ctrl+alt+r"));
        assert_eq!(
            shortcut_for(&default_hotkeys(), &Action::FocusModeToggle).as_deref(),
            Some("ctrl+alt+f")
        );
        assert_eq!(shortcut_for(&BTreeMap::new(), &Action::RelayoutAll), None);
    }

    #[test]
    fn pause_turns_into_resume() {
        let mut s = status();
        s.paused = true;
        let m = menu(&s, &AppFlags::default(), Lang::En);
        let pause = m
            .iter()
            .find(|e| {
                matches!(
                    e,
                    MenuEntry::Item {
                        command: MenuCommand::TogglePause,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(matches!(
            pause,
            MenuEntry::Item { text, icon: Some(IconKind::Resume), .. } if text == "Resume"
        ));
        assert!(m.iter().any(|e| matches!(
            e,
            MenuEntry::Item {
                command: MenuCommand::RelayoutAll,
                enabled: false,
                ..
            }
        )));
    }

    #[test]
    fn auto_arrange_is_a_check() {
        let flags = AppFlags {
            auto_arrange: true,
            ..Default::default()
        };
        let m = menu(&status(), &flags, Lang::En);
        assert!(m.iter().any(|e| matches!(
            e,
            MenuEntry::Check {
                command: MenuCommand::ToggleAutoArrange,
                checked: true,
                ..
            }
        )));
    }

    #[test]
    fn pending_setup_opens_the_settings() {
        let flags = AppFlags {
            setup_pending: true,
            ..Default::default()
        };
        let m = menu(&status(), &flags, Lang::Es);
        assert_eq!(
            m[0],
            MenuEntry::Item {
                text: "⚠ Configurá tus zonas para empezar…".into(),
                command: MenuCommand::OpenSettings,
                enabled: true,
                icon: Some(IconKind::Warning),
                shortcut: None,
            }
        );
    }

    #[test]
    fn api_error_is_shown() {
        let flags = AppFlags {
            api_error: Some("127.0.0.1:47800 is in use by another program".into()),
            ..Default::default()
        };
        let m = menu(&status(), &flags, Lang::En);
        assert!(matches!(
            &m[0],
            MenuEntry::Item { text, command: MenuCommand::ShowLogs, .. }
                if text == "⚠ Local API off: 127.0.0.1:47800 is in use by another program"
        ));
    }

    #[test]
    fn warnings_come_first() {
        let flags = AppFlags {
            permission_missing: true,
            config_error: true,
            hotkey_conflicts: vec!["ctrl+alt+f".into()],
            ..Default::default()
        };
        let m = menu(&status(), &flags, Lang::En);
        assert!(matches!(
            &m[0],
            MenuEntry::Item {
                command: MenuCommand::OpenPermissionSettings,
                ..
            }
        ));
        assert!(matches!(
            &m[1],
            MenuEntry::Item {
                command: MenuCommand::ShowLogs,
                ..
            }
        ));
        assert_eq!(
            m[2],
            MenuEntry::Label("⚠ Shortcuts in use by another app: ctrl+alt+f".into())
        );
        assert_eq!(m[3], MenuEntry::Separator);
    }

    #[test]
    fn no_profile_and_manual() {
        let mut s = status();
        s.profile = None;
        s.layout = None;
        let m = menu(&s, &AppFlags::default(), Lang::En);
        assert_eq!(profile_submenu(&m).0, "No profile fits these displays");

        let mut s = status();
        s.manual = true;
        let m = menu(&s, &AppFlags::default(), Lang::En);
        assert_eq!(
            profile_submenu(&m).0,
            "Profile: Casa – Notebook abierta (manual)"
        );
    }

    #[test]
    fn automatic_check_item_toggles() {
        // On (no profile chosen by hand) → keep the current profile.
        let mut s = status();
        assert_eq!(
            toggle_auto_profile(&s),
            Some(Action::ActivateProfile {
                name: "Casa".into()
            })
        );
        // Off → back to automatic.
        s.manual = true;
        assert_eq!(toggle_auto_profile(&s), Some(Action::AutoProfile));
        // On, but nothing fits: nothing to keep.
        let mut s = status();
        s.profile = None;
        assert_eq!(toggle_auto_profile(&s), None);
    }
}
