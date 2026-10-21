//! User interface texts in English and Spanish.
//!
//! A deliberately tiny table for the tray menu. The settings window, with many more
//! strings, uses Fluent instead (see [`crate::intl`]).

use venecita_core::config::Language;

/// A resolved UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// English.
    En,
    /// Spanish.
    Es,
}

impl Lang {
    /// Resolves the configured language; `Auto` uses the OS preference (e.g. `"es-AR"`).
    pub fn resolve(configured: Language, os_preference: Option<&str>) -> Lang {
        match configured {
            Language::En => Lang::En,
            Language::Es => Lang::Es,
            Language::Auto => match os_preference {
                Some(l) if l.to_lowercase().starts_with("es") => Lang::Es,
                _ => Lang::En,
            },
        }
    }
}

/// Keys of the translatable texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Text {
    ProfileActive,
    ProfileManualSuffix,
    NoProfile,
    Profiles,
    Automatic,
    RelayoutAll,
    Pause,
    LaunchAtLogin,
    Settings,
    OpenConfig,
    ReloadConfig,
    ShowLogs,
    Quit,
    PermissionMissing,
    ConfigError,
    HotkeysUnavailable,
    Arrange,
    AutoArrange,
    Resume,
}

/// The text for `key` in `lang`.
pub fn tr(lang: Lang, key: Text) -> &'static str {
    use Text::*;
    match (lang, key) {
        (Lang::En, ProfileActive) => "Profile",
        (Lang::Es, ProfileActive) => "Perfil",
        (Lang::En, ProfileManualSuffix) => "(manual)",
        (Lang::Es, ProfileManualSuffix) => "(manual)",
        (Lang::En, NoProfile) => "No profile fits these displays",
        (Lang::Es, NoProfile) => "Ningún perfil coincide con estas pantallas",
        (Lang::En, Profiles) => "Profiles",
        (Lang::Es, Profiles) => "Perfiles",
        (Lang::En, Automatic) => "Automatic (by displays)",
        (Lang::Es, Automatic) => "Automático (según pantallas)",
        (Lang::En, RelayoutAll) => "Re-arrange all windows",
        (Lang::Es, RelayoutAll) => "Reordenar todas las ventanas",
        (Lang::En, Pause) => "Pause",
        (Lang::Es, Pause) => "Pausar",
        (Lang::En, LaunchAtLogin) => "Launch at login",
        (Lang::Es, LaunchAtLogin) => "Iniciar con el sistema",
        (Lang::En, Settings) => "Settings…",
        (Lang::Es, Settings) => "Ajustes…",
        (Lang::En, OpenConfig) => "Open configuration file…",
        (Lang::Es, OpenConfig) => "Abrir archivo de configuración…",
        (Lang::En, ReloadConfig) => "Reload configuration",
        (Lang::Es, ReloadConfig) => "Recargar configuración",
        (Lang::En, ShowLogs) => "Show logs",
        (Lang::Es, ShowLogs) => "Ver registros",
        (Lang::En, Quit) => "Quit",
        (Lang::Es, Quit) => "Salir",
        (Lang::En, Arrange) => "Arrange windows",
        (Lang::Es, Arrange) => "Ordenar ventanas",
        (Lang::En, AutoArrange) => "Arrange automatically",
        (Lang::Es, AutoArrange) => "Ordenar automáticamente",
        (Lang::En, Resume) => "Resume",
        (Lang::Es, Resume) => "Reanudar",
        (Lang::En, PermissionMissing) => "⚠ Grant Accessibility permission…",
        (Lang::Es, PermissionMissing) => "⚠ Dar permiso de Accesibilidad…",
        (Lang::En, ConfigError) => "⚠ Configuration error (see logs)",
        (Lang::Es, ConfigError) => "⚠ Error en la configuración (ver registros)",
        (Lang::En, HotkeysUnavailable) => "⚠ Shortcuts in use by another app:",
        (Lang::Es, HotkeysUnavailable) => "⚠ Atajos usados por otra app:",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_os() {
        assert_eq!(Lang::resolve(Language::Auto, Some("es-AR")), Lang::Es);
        assert_eq!(Lang::resolve(Language::Auto, Some("en-US")), Lang::En);
        assert_eq!(Lang::resolve(Language::Auto, None), Lang::En);
        assert_eq!(Lang::resolve(Language::Es, Some("en-US")), Lang::Es);
    }

    #[test]
    fn spanish_differs_from_english() {
        assert_ne!(tr(Lang::En, Text::Quit), tr(Lang::Es, Text::Quit));
    }
}
