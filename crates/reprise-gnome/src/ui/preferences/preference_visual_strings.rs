macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(in crate::ui) const PLAYER_BAR: &str = N_!("Player Bar");
pub(in crate::ui) const LIBRARY_WINDOW: &str = N_!("Library Window");
pub(in crate::ui) const COLUMNS: &str = N_!("Columns");
pub(in crate::ui) const SHOW_FILTERS: &str = N_!("Show Filters");
pub(in crate::ui) const SHOW_INFORMATION_PANEL: &str = N_!("Show Information Panel");
pub(in crate::ui) const PLAYER_BAR_POSITION_SAVE_FAILED: &str =
    N_!("Could not save the player bar position");
pub(in crate::ui) const SIDEBAR_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save sidebar visibility");
pub(in crate::ui) const FILTER_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save filter visibility");
pub(in crate::ui) const INFORMATION_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save information panel visibility");
pub(in crate::ui) const STATUS_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save status line visibility");
pub(in crate::ui) const DENSITY_SAVE_FAILED: &str = N_!("Could not save list density");
