//! Translatable copy for the window-decoration preference. Kept separate
//! because the central string catalog is intentionally at its size limit.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(super) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(super) const WINDOW_DECORATIONS: &str = N_!("Window Decorations");
pub(super) const WINDOW_DECORATIONS_SUBTITLE: &str =
    N_!("Use Reprise's flat header, or request a title bar and borders from the desktop");
pub(super) const DECORATION_CLIENT: &str = N_!("Chromium (CSD)");
pub(super) const DECORATION_SYSTEM: &str = N_!("System title bar");
