macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(in crate::ui) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub(in crate::ui) const PLAYER_BAR: &str = N_!("Player Bar");
pub(in crate::ui) const COLUMNS: &str = N_!("Columns");

// The Layout page names the window's regions, not the switches that hide them:
// the preview and the rows below it have to read as one surface.
pub(in crate::ui) const WINDOW_LAYOUT: &str = N_!("Window Layout");
pub(in crate::ui) const WINDOW_REGIONS: &str = N_!("Window Regions");
pub(in crate::ui) const LAYOUT_PREVIEW_HINT: &str = N_!("Click a region to move or hide it");
pub(in crate::ui) const NAVIGATION_SIDEBAR: &str = N_!("Navigation Sidebar");
pub(in crate::ui) const FILTER_BAR: &str = N_!("Filter Bar");
pub(in crate::ui) const DETAILS_SIDEBAR: &str = N_!("Details Sidebar");
pub(in crate::ui) const STATUS_BAR: &str = N_!("Status Bar");
pub(in crate::ui) const NAVIGATION_SIDEBAR_EDGE: &str = N_!("Left edge");
pub(in crate::ui) const FILTER_BAR_EDGE: &str = N_!("Above the track list");
pub(in crate::ui) const DETAILS_SIDEBAR_EDGE: &str = N_!("Right edge");
pub(in crate::ui) const STATUS_BAR_EDGE: &str = N_!("Below the track list");
pub(in crate::ui) const REGION_NAVIGATION: &str = N_!("Navigation");
pub(in crate::ui) const REGION_DETAILS: &str = N_!("Details");
pub(in crate::ui) const MOVE_PLAYER_BAR: &str = N_!("Move player bar");
pub(in crate::ui) const HIDE_NAVIGATION_SIDEBAR: &str = N_!("Hide the sidebar");
pub(in crate::ui) const SHOW_NAVIGATION_SIDEBAR: &str = N_!("Show the sidebar");
pub(in crate::ui) const HIDE_FILTER_BAR: &str = N_!("Hide the filter bar");
pub(in crate::ui) const SHOW_FILTER_BAR: &str = N_!("Show the filter bar");
pub(in crate::ui) const HIDE_DETAILS_SIDEBAR: &str = N_!("Hide the details sidebar");
pub(in crate::ui) const SHOW_DETAILS_SIDEBAR: &str = N_!("Show the details sidebar");
pub(in crate::ui) const HIDE_STATUS_BAR: &str = N_!("Hide the status bar");
pub(in crate::ui) const SHOW_STATUS_BAR: &str = N_!("Show the status bar");
pub(in crate::ui) const RESTORE_LAYOUT_DEFAULTS: &str = N_!("Restore defaults");
pub(in crate::ui) const PLAYER_BAR_POSITION_SAVE_FAILED: &str =
    N_!("Could not save the player bar position");
pub(in crate::ui) const SIDEBAR_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save sidebar visibility");
pub(in crate::ui) const FILTER_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save filter visibility");
pub(in crate::ui) const INFORMATION_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save info panel visibility");
pub(in crate::ui) const STATUS_VISIBILITY_SAVE_FAILED: &str =
    N_!("Could not save status line visibility");
