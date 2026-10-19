//! Fluent-based translations for the settings window.
//!
//! The strings live in `assets/i18n/{en,es}/settings.ftl` and are embedded at build time.
//! [`I18n`] loads the bundle for one resolved [`Lang`] and looks messages up by id; a test
//! checks that both languages define exactly the same ids.
//!
//! The tray menu still uses the small hand-written table in [`crate::i18n`]; it is already
//! bilingual and tested. The two can be unified later by moving those strings here.

use fluent::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use crate::i18n::Lang;

const EN_FTL: &str = include_str!("../assets/i18n/en/settings.ftl");
const ES_FTL: &str = include_str!("../assets/i18n/es/settings.ftl");

/// The translations for one language, ready to look up by message id.
pub struct I18n {
    lang: Lang,
    bundle: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n").field("lang", &self.lang).finish()
    }
}

impl I18n {
    /// Loads the translations for `lang`.
    pub fn new(lang: Lang) -> Self {
        let (src, tag) = match lang {
            Lang::Es => (ES_FTL, "es"),
            Lang::En => (EN_FTL, "en"),
        };
        let id: LanguageIdentifier = tag.parse().expect("valid language tag");
        let resource =
            FluentResource::try_new(src.to_string()).expect("embedded .ftl files are valid");
        let mut bundle = FluentBundle::new(vec![id]);
        // UI strings are inserted into native widgets, so suppress the Unicode isolation
        // marks Fluent would otherwise wrap arguments in.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .expect("embedded .ftl files have no duplicate ids");
        Self { lang, bundle }
    }

    /// The language these translations are for.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// The text for `key`. Unknown keys return the key itself, so a missing translation is
    /// visible but never panics.
    pub fn get(&self, key: &str) -> String {
        self.format(key, None)
    }

    /// The text for `key`, filling the `{ $name }` placeholders from `args`.
    pub fn get_args(&self, key: &str, args: &FluentArgs) -> String {
        self.format(key, Some(args))
    }

    fn format(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let Some(message) = self.bundle.get_message(key) else {
            return key.to_string();
        };
        let Some(pattern) = message.value() else {
            return key.to_string();
        };
        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            tracing::warn!(key, ?errors, "fluent formatting problem");
        }
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Collects the top-level message ids of a Fluent file (lines like `id = value`,
    /// ignoring comments, blank lines and indented continuations).
    fn message_ids(ftl: &str) -> BTreeSet<String> {
        ftl.lines()
            .filter(|line| {
                let first = line.chars().next();
                // Message lines start in column 0 with a letter; `#` is a comment and a
                // leading space is a continuation of the previous value.
                matches!(first, Some(c) if c.is_ascii_alphabetic())
            })
            .filter_map(|line| line.split_once('=').map(|(id, _)| id.trim().to_string()))
            .collect()
    }

    #[test]
    fn both_languages_define_the_same_keys() {
        let en = message_ids(EN_FTL);
        let es = message_ids(ES_FTL);
        let only_en: Vec<_> = en.difference(&es).collect();
        let only_es: Vec<_> = es.difference(&en).collect();
        assert!(
            only_en.is_empty() && only_es.is_empty(),
            "Fluent keys differ.\n  only in en: {only_en:?}\n  only in es: {only_es:?}"
        );
        assert!(
            en.len() > 20,
            "expected a real set of strings, got {}",
            en.len()
        );
    }

    #[test]
    fn both_bundles_load() {
        for lang in [Lang::En, Lang::Es] {
            let i18n = I18n::new(lang);
            assert!(!i18n.get("save").is_empty());
        }
    }

    #[test]
    fn languages_actually_differ() {
        assert_ne!(
            I18n::new(Lang::En).get("save"),
            I18n::new(Lang::Es).get("save")
        );
    }

    #[test]
    fn unknown_key_returns_itself() {
        assert_eq!(I18n::new(Lang::En).get("no-such-key"), "no-such-key");
    }

    #[test]
    fn arguments_are_substituted() {
        let i18n = I18n::new(Lang::En);
        let mut args = FluentArgs::new();
        args.set("count", 3);
        let text = i18n.get_args("rule-count", &args);
        assert!(text.contains('3'), "got {text}");
    }

    #[test]
    fn every_key_resolves_in_both_languages() {
        // Formatting each message must not fall back to the raw key.
        for (src, lang) in [(EN_FTL, Lang::En), (ES_FTL, Lang::Es)] {
            let i18n = I18n::new(lang);
            for id in message_ids(src) {
                // Messages with placeholders need args; skip the two that take them.
                if matches!(id.as_str(), "rule-count" | "save-failed" | "problems") {
                    continue;
                }
                assert_ne!(i18n.get(&id), id, "`{id}` did not resolve in {lang:?}");
            }
        }
    }
}
