//! gettext initialization and translated message formatting.

use std::sync::OnceLock;

use gettextrs::{
    bind_textdomain_codeset, bindtextdomain, gettext as gettext_message,
    ngettext as ngettext_message, npgettext as npgettext_message, pgettext as pgettext_message,
    setlocale, textdomain, LocaleCategory,
};

const DEFAULT_PACKAGE: &str = "reprise";
const DEFAULT_LOCALE_DIR: &str = "/usr/share/locale";
const SMOKE_ENV: &str = "REPRISE_SMOKE_I18N";
const TRANSLATED_LOCALES: &str = include_str!("../../../po/LINGUAS");

static ACTIVE_GUI_LANGUAGE: OnceLock<String> = OnceLock::new();

pub fn init() {
    let selected_locale = setlocale(LocaleCategory::LcAll, "");
    let message_locale = setlocale(LocaleCategory::LcMessages, "").or(selected_locale);
    let message_locale = message_locale
        .as_deref()
        .and_then(|locale| std::str::from_utf8(locale).ok());
    let language_preferences = std::env::var("LANGUAGE").ok();
    let _ = ACTIVE_GUI_LANGUAGE.set(preferred_gui_language(
        message_locale,
        language_preferences.as_deref(),
    ));
    let package = option_env!("GETTEXT_PACKAGE").unwrap_or(DEFAULT_PACKAGE);
    let locale_dir = std::env::var("REPRISE_LOCALEDIR")
        .ok()
        .or_else(|| option_env!("LOCALEDIR").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_LOCALE_DIR.to_string());
    if let Err(error) = bindtextdomain(package, &locale_dir) {
        tracing::warn!(%error, locale_dir, "could not bind gettext locale directory");
    }
    if let Err(error) = bind_textdomain_codeset(package, "UTF-8") {
        tracing::warn!(%error, "could not set gettext domain encoding");
    }
    if let Err(error) = textdomain(package) {
        tracing::warn!(%error, "could not select gettext text domain");
    }
}

pub fn active_gui_language() -> Option<&'static str> {
    ACTIVE_GUI_LANGUAGE.get().map(String::as_str)
}

pub fn pgettext(context: &str, message: &str) -> String {
    pgettext_message(context, message)
}

fn preferred_gui_language(
    message_locale: Option<&str>,
    language_preferences: Option<&str>,
) -> String {
    let Some(message_locale) = message_locale else {
        return "en".to_owned();
    };
    if is_source_locale(message_locale) {
        return "en".to_owned();
    }
    if let Some(preferences) = language_preferences.filter(|preferences| !preferences.is_empty()) {
        return preferences
            .split(':')
            .find_map(resolve_gui_language)
            .unwrap_or_else(|| "en".to_owned());
    }
    resolve_gui_language(message_locale).unwrap_or_else(|| "en".to_owned())
}

fn resolve_gui_language(locale: &str) -> Option<String> {
    let locale = locale
        .trim()
        .split_once('@')
        .map_or(locale.trim(), |(locale, _)| locale)
        .split_once('.')
        .map_or_else(|| locale.trim(), |(locale, _)| locale)
        .replace('-', "_");
    let mut parts = locale.split('_');
    let language = parts.next()?.to_ascii_lowercase();
    if language == "en" {
        return Some(language);
    }
    let territory = parts.next().map(str::to_ascii_uppercase);
    let exact = territory.map(|territory| format!("{language}_{territory}"));
    exact
        .filter(|locale| translated_locale_exists(locale))
        .or_else(|| translated_locale_exists(&language).then_some(language))
}

fn translated_locale_exists(locale: &str) -> bool {
    TRANSLATED_LOCALES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .any(|translated| translated == locale)
}

fn is_source_locale(locale: &str) -> bool {
    matches!(
        locale
            .trim()
            .split_once('.')
            .map_or(locale.trim(), |(locale, _)| locale),
        "C" | "POSIX"
    )
}

pub fn gettext(message: &str) -> String {
    gettext_message(message)
}

pub fn ngettext(singular: &str, plural: &str, count: u32) -> String {
    ngettext_message(singular, plural, count)
}

pub fn npgettext(context: &str, singular: &str, plural: &str, count: u32) -> String {
    npgettext_message(context, singular, plural, count)
}

pub fn format_message(message: &str, values: &[(&str, &str)]) -> String {
    let mut formatted = message.to_string();
    for (name, value) in values {
        formatted = formatted.replace(&format!("{{{name}}}"), value);
    }
    formatted
}

pub fn smoke_report() {
    if std::env::var(SMOKE_ENV).is_err() {
        return;
    }
    tracing::info!(
        welcome = %gettext("Welcome to Reprise"),
        music = %gettext("Music"),
        play = %gettext("Play"),
        "i18n smoke report"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_placeholders_replace_repeated_known_values_and_preserve_unknown_ones() {
        assert_eq!(
            format_message(
                "{count} of {count}: {name} / {unknown}",
                &[("count", "2"), ("name", "Mix")],
            ),
            "2 of 2: Mix / {unknown}"
        );
    }

    #[test]
    fn gui_language_follows_gettext_catalog_resolution() {
        assert_eq!(preferred_gui_language(Some("de_CH.UTF-8"), None), "de");
        assert_eq!(
            preferred_gui_language(Some("fr_FR.UTF-8"), Some("it:de:fr")),
            "de"
        );
        assert_eq!(preferred_gui_language(Some("zh_CN.UTF-8"), None), "zh_CN");
    }

    #[test]
    fn gui_language_uses_english_when_gettext_uses_source_strings() {
        assert_eq!(preferred_gui_language(Some("it_IT.UTF-8"), None), "en");
        assert_eq!(
            preferred_gui_language(Some("fr_FR.UTF-8"), Some("it")),
            "en"
        );
        assert_eq!(preferred_gui_language(Some("C.UTF-8"), Some("de")), "en");
        assert_eq!(preferred_gui_language(None, Some("de")), "en");
    }
}
