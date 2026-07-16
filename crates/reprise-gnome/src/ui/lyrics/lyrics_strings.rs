//! Gettext-backed copy for the Lyrics surface.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(in crate::ui) const LYRICS: &str = N_!("Lyrics");
pub(in crate::ui) const PLAY_TO_SEE_LYRICS: &str = N_!("Play a track to see its lyrics");
pub(in crate::ui) const LOADING_LYRICS: &str = N_!("Loading lyrics…");
pub(in crate::ui) const INSTRUMENTAL: &str = N_!("Instrumental");
pub(in crate::ui) const NO_LYRICS_FOUND: &str = N_!("No lyrics found");
pub(in crate::ui) const LYRICS_UNAVAILABLE: &str = N_!("Could not load lyrics");
pub(in crate::ui) const RETRY: &str = N_!("Retry");

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}
