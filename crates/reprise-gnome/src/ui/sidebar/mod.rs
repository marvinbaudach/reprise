mod sidebar_activity_slot;
pub(in crate::ui) mod sidebar_device_card;
pub(crate) mod sidebar_dnd;
pub(crate) mod sidebar_export;
pub(in crate::ui) mod sidebar_issue_cleanup;
pub(in crate::ui) mod sidebar_issue_strings;
pub(in crate::ui) mod sidebar_playlist_creation;
pub(in crate::ui) mod sidebar_presentation;
pub(in crate::ui) mod sidebar_rebuild;
pub(crate) mod sidebar_session;
#[path = "sidebar.rs"]
mod surface;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use sidebar_session::show_toast;
pub(in crate::ui) use surface::{
    find_row, rebuild, resolve_select_source, select_row_in_its_listbox, OnRemoveMissing, RowEntry,
    Shared, Sidebar,
};
