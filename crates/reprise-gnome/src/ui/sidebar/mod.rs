mod sidebar_activity_slot;
mod sidebar_boundary_navigation;
pub(in crate::ui) mod sidebar_device_card;
mod sidebar_device_card_text;
mod sidebar_device_section;
pub(crate) mod sidebar_dnd;
pub(crate) mod sidebar_export;
pub(in crate::ui) mod sidebar_issue_cleanup;
pub(in crate::ui) mod sidebar_issue_strings;
mod sidebar_issues_section;
mod sidebar_module_menu;
mod sidebar_navigation_scroller;
pub(in crate::ui) mod sidebar_playlist_creation;
mod sidebar_playlist_quick_add;
pub(in crate::ui) mod sidebar_presentation;
pub(in crate::ui) mod sidebar_rebuild;
mod sidebar_root;
mod sidebar_row_wiring;
pub(crate) mod sidebar_session;
mod sidebar_turned_off_row;
#[path = "sidebar.rs"]
mod surface;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use sidebar_session::{show_toast, show_toast_with_action};
pub(in crate::ui) use surface::{
    find_row, has_sidebar_row, rebuild, resolve_select_source, select_row_in_its_listbox,
    OnRemoveMissing, RowEntry, Shared, Sidebar,
};
