//! gettext initialization and translated message formatting.

use gettextrs::{
    bind_textdomain_codeset, bindtextdomain, gettext as gettext_message,
    ngettext as ngettext_message, setlocale, textdomain, LocaleCategory,
};

const DEFAULT_PACKAGE: &str = "reprise";
const DEFAULT_LOCALE_DIR: &str = "/usr/share/locale";
const SMOKE_ENV: &str = "REPRISE_SMOKE_I18N";

pub fn init() {
    setlocale(LocaleCategory::LcAll, "");
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

pub fn gettext(message: &str) -> String {
    gettext_message(message)
}

pub fn ngettext(singular: &str, plural: &str, count: u32) -> String {
    ngettext_message(singular, plural, count)
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
}
