//! The tray menu as plain data.
//!
//! Building the menu is separated from drawing it so its contents can be unit tested;
//! `tray.rs` turns these entries into real menu items.

use venecita_core::engine::Status;

use crate::i18n::{tr, Lang, Text};

/// Identifiers of menu commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    /// Choose a profile by hand.
    Profile(String),
    /// Back to automatic profile detection.
    AutoProfile,
    /// Re-arrange every window.
    RelayoutAll,
    /// Pause/resume.
    TogglePause,
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
            AutoProfile,
            RelayoutAll,
            TogglePause,
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
    /// Shortcuts that could not be registered (taken by another app).
    pub hotkey_conflicts: Vec<String>,
}

/// Builds the menu.
pub fn build(status: &Status, flags: &AppFlags, lang: Lang) -> Vec<MenuEntry> {
    use MenuEntry::*;
    let t = |k| tr(lang, k).to_string();
    let mut m = Vec::new();

    if flags.permission_missing {
        m.push(Item {
            text: t(Text::PermissionMissing),
            command: MenuCommand::OpenPermissionSettings,
            enabled: true,
        });
        m.push(Separator);
    }
    if flags.config_error {
        m.push(Item {
            text: t(Text::ConfigError),
            command: MenuCommand::ShowLogs,
            enabled: true,
        });
        m.push(Separator);
    }

    if !flags.hotkey_conflicts.is_empty() {
        m.push(Label(format!(
            "{} {}",
            t(Text::HotkeysUnavailable),
            flags.hotkey_conflicts.join(", ")
        )));
        m.push(Separator);
    }

    m.push(Label(match (&status.profile, &status.layout) {
        (Some(p), Some(l)) => {
            let manual = if status.manual {
                format!(" {}", t(Text::ProfileManualSuffix))
            } else {
                String::new()
            };
            format!("{}: {p} – {l}{manual}", t(Text::ProfileActive))
        }
        _ => t(Text::NoProfile),
    }));
    m.push(Separator);

    m.push(Label(t(Text::Profiles)));
    m.push(Check {
        text: t(Text::Automatic),
        command: MenuCommand::AutoProfile,
        checked: !status.manual,
        enabled: true,
    });
    for p in &status.profiles {
        m.push(Check {
            text: format!("    {}", p.name),
            command: MenuCommand::Profile(p.name.clone()),
            checked: p.active,
            enabled: p.fits,
        });
    }
    m.push(Separator);

    m.push(Item {
        text: t(Text::RelayoutAll),
        command: MenuCommand::RelayoutAll,
        enabled: !status.paused,
    });
    m.push(Check {
        text: t(Text::Pause),
        command: MenuCommand::TogglePause,
        checked: status.paused,
        enabled: true,
    });
    m.push(Separator);

    m.push(Check {
        text: t(Text::LaunchAtLogin),
        command: MenuCommand::ToggleLaunchAtLogin,
        checked: flags.launch_at_login,
        enabled: true,
    });
    m.push(Item {
        text: t(Text::Settings),
        command: MenuCommand::OpenSettings,
        enabled: true,
    });
    m.push(Item {
        text: t(Text::OpenConfig),
        command: MenuCommand::OpenConfig,
        enabled: true,
    });
    m.push(Item {
        text: t(Text::ReloadConfig),
        command: MenuCommand::ReloadConfig,
        enabled: true,
    });
    m.push(Item {
        text: t(Text::ShowLogs),
        command: MenuCommand::ShowLogs,
        enabled: true,
    });
    m.push(Separator);
    m.push(Item {
        text: t(Text::Quit),
        command: MenuCommand::Quit,
        enabled: true,
    });
    m
}

#[cfg(test)]
mod tests {
    use super::*;
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
        }
    }

    #[test]
    fn command_ids_round_trip() {
        for c in [
            MenuCommand::Profile("Casa Grande".into()),
            MenuCommand::Quit,
            MenuCommand::ToggleLaunchAtLogin,
            MenuCommand::OpenSettings,
        ] {
            assert_eq!(MenuCommand::from_id(&c.id()), Some(c));
        }
        assert_eq!(MenuCommand::from_id("nope"), None);
    }

    #[test]
    fn shows_active_profile_and_disables_profiles_that_dont_fit() {
        let m = build(&status(), &AppFlags::default(), Lang::Es);
        assert_eq!(
            m[0],
            MenuEntry::Label("Perfil: Casa – Notebook abierta".into())
        );
        let trabajo = m
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
    fn warnings_come_first() {
        let flags = AppFlags {
            permission_missing: true,
            config_error: true,
            launch_at_login: false,
            hotkey_conflicts: vec!["ctrl+alt+f".into()],
        };
        let m = build(&status(), &flags, Lang::En);
        assert!(matches!(
            &m[0],
            MenuEntry::Item {
                command: MenuCommand::OpenPermissionSettings,
                ..
            }
        ));
        assert!(matches!(
            &m[2],
            MenuEntry::Item {
                command: MenuCommand::ShowLogs,
                ..
            }
        ));
        assert_eq!(
            m[4],
            MenuEntry::Label("⚠ Shortcuts in use by another app: ctrl+alt+f".into())
        );
    }

    #[test]
    fn no_profile_and_manual_and_paused() {
        let mut s = status();
        s.profile = None;
        s.layout = None;
        let m = build(&s, &AppFlags::default(), Lang::En);
        assert_eq!(
            m[0],
            MenuEntry::Label("No profile fits these displays".into())
        );

        let mut s = status();
        s.manual = true;
        s.paused = true;
        let m = build(&s, &AppFlags::default(), Lang::En);
        assert_eq!(
            m[0],
            MenuEntry::Label("Profile: Casa – Notebook abierta (manual)".into())
        );
        assert!(m.iter().any(|e| matches!(
            e,
            MenuEntry::Item {
                command: MenuCommand::RelayoutAll,
                enabled: false,
                ..
            }
        )));
        assert!(m.iter().any(|e| matches!(
            e,
            MenuEntry::Check {
                command: MenuCommand::TogglePause,
                checked: true,
                ..
            }
        )));
    }
}
