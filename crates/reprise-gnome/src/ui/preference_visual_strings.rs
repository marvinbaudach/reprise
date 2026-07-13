macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(super) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(super) const PLAYER_BAR: &str = N_!("Player Bar");
pub(super) const LIBRARY_WINDOW: &str = N_!("Library Window");
pub(super) const COLUMNS: &str = N_!("Columns");
pub(super) const COLOR_SCHEME_SAVE_FAILED: &str = N_!("Could not save the color scheme");
pub(super) const PLAYER_BAR_POSITION_SAVE_FAILED: &str =
    N_!("Could not save the player bar position");
