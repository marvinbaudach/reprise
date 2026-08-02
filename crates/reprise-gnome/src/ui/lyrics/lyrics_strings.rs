//! Gettext-backed copy for the Lyrics surface.

pub(in crate::ui) use reprise_view::strings::lyrics::*;

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}
