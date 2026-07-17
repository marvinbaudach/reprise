pub(in crate::ui) mod column_header_dnd;
pub(crate) mod column_layout;
pub(in crate::ui) mod column_layout_editor;
pub(in crate::ui) mod column_widths;
pub(in crate::ui) mod current_track_selection;
pub(in crate::ui) mod end_of_results;
pub(in crate::ui) mod list_density;
pub(in crate::ui) mod match_highlight;
pub(crate) mod queue_row_mapping;
pub(crate) mod queue_sections;
pub(crate) mod rating;
pub(in crate::ui) mod reload_restore;
#[path = "track_list.rs"]
mod surface;
pub(crate) mod track_actions;
pub(in crate::ui) mod track_content;
pub(in crate::ui) mod track_cover;
pub(crate) mod track_list_activation;
pub(in crate::ui) mod track_list_builder;
pub(in crate::ui) mod track_list_callbacks;
pub(crate) mod track_list_columns;
pub(in crate::ui) mod track_list_context_keys;
pub(crate) mod track_list_context_menu;
pub(crate) mod track_list_dnd;
pub(crate) mod track_list_dnd_smoke;
pub(crate) mod track_list_empty_state;
mod track_list_filter_actions;
pub(in crate::ui) mod track_list_header_style;
pub(in crate::ui) mod track_list_layout;
pub(in crate::ui) mod track_list_menu_seams;
pub(crate) mod track_list_menu_smoke;
pub(in crate::ui) mod track_list_missing;
pub(crate) mod track_list_model;
pub(in crate::ui) mod track_list_queue_menu;
pub(in crate::ui) mod track_list_reload;
pub(in crate::ui) mod track_list_rescan;
pub(in crate::ui) mod track_list_row_interaction;
pub(in crate::ui) mod track_list_selection;
pub(crate) mod track_list_smoke;
pub(crate) mod track_list_sort;
mod track_list_toast;
pub(in crate::ui) mod track_menu;
pub(crate) mod track_playback_selection;
pub(in crate::ui) mod view_state_memory;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::{
    notify_import_errors_mutated_and_reload, playlist_reorder_allowed, reload,
    set_filter_and_reload, set_source_and_reload, show_toast, OnActivate, Shared, TrackList,
};
pub(in crate::ui) use track_list_layout::{
    STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST, STACK_PAGE_MISSING,
};
