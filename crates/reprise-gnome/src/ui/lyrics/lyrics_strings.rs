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
pub(in crate::ui) const SYNCED_LRCLIB: &str = N_!("synced · LRCLIB");
pub(in crate::ui) const LYRICS_TAGS: &str = N_!("lyrics · tags");
pub(in crate::ui) const ONLINE_LYRICS_DISABLED: &str = N_!("Online lyrics are disabled");
pub(in crate::ui) const ENABLE_LYRICS_DESCRIPTION: &str =
    N_!("Enable them to load missing lyrics automatically");
pub(in crate::ui) const ENABLE_IN_SETTINGS: &str = N_!("Enable in Settings");

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}
