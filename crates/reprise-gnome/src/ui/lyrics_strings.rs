//! Gettext-backed copy for the Lyrics surface.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(super) const LYRICS: &str = N_!("Lyrics");
pub(super) const PLAY_TO_SEE_LYRICS: &str = N_!("Play a track to see its lyrics");
pub(super) const LOADING_LYRICS: &str = N_!("Loading lyrics…");
pub(super) const INSTRUMENTAL: &str = N_!("Instrumental");
pub(super) const NO_LYRICS_FOUND: &str = N_!("No lyrics found");
pub(super) const LYRICS_UNAVAILABLE: &str = N_!("Could not load lyrics");
pub(super) const RETRY: &str = N_!("Retry");

pub(super) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}
