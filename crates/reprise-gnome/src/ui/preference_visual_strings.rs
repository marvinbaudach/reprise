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
pub(super) const SHOW_FILTERS: &str = N_!("Show Filters");
pub(super) const SHOW_INFORMATION_PANEL: &str = N_!("Show Information Panel");
pub(super) const PLAYER_BAR_POSITION_SAVE_FAILED: &str =
    N_!("Could not save the player bar position");
pub(super) const SIDEBAR_VISIBILITY_SAVE_FAILED: &str = N_!("Could not save sidebar visibility");
pub(super) const FILTER_VISIBILITY_SAVE_FAILED: &str = N_!("Could not save filter visibility");
pub(super) const INFORMATION_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save information panel visibility");
pub(super) const STATUS_VISIBILITY_SAVE_FAILED: &str = N_!("Could not save status line visibility");
pub(super) const DENSITY_SAVE_FAILED: &str = N_!("Could not save list density");
