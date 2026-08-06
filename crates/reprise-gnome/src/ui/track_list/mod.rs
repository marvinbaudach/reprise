pub(in crate::ui) mod column_header_dnd;
pub(crate) mod column_layout;
pub(in crate::ui) mod column_layout_editor;
pub(in crate::ui) use reprise_view::column_widths;
pub(in crate::ui) mod current_track_selection;
pub(crate) mod diagnostic_trail;
pub(in crate::ui) mod end_of_results;
pub(in crate::ui) mod list_density;
pub(in crate::ui) mod match_highlight;
pub(in crate::ui) mod now_playing_marker;
mod playlist_reorder_guard;
pub(crate) mod queue_item_menu;
pub(crate) mod queue_item_presentation;
pub(crate) mod queue_row_mapping {
    pub(crate) use reprise_view::queue::rows::{
        classify, is_read_only_episode_projection, reorder_op, reorder_rows, QueueReorderOp,
        QueueRow,
    };
}
pub(crate) mod queue_sections;
pub(crate) mod rating;
pub(in crate::ui) mod rating_cell_refresh;
mod rating_column;
pub(in crate::ui) mod reload_restore;
mod responsive_columns;
pub(crate) mod row_loss_watchdog;
pub(crate) mod row_loss_watchdog_state;
#[path = "track_list.rs"]
mod surface;
pub(in crate::ui) mod tag_mutation_refresh;
pub(crate) mod track_actions;
pub(in crate::ui) mod track_content;
pub(in crate::ui) mod track_cover;
pub(crate) mod track_list_activation;
pub(in crate::ui) mod track_list_builder;
pub(in crate::ui) mod track_list_callbacks;
pub(crate) mod track_list_columns;
mod track_list_context_action_states;
pub(in crate::ui) mod track_list_context_keys;
pub(crate) mod track_list_context_menu;
pub(crate) mod track_list_dnd;
pub(crate) mod track_list_dnd_smoke;
pub(crate) mod track_list_empty_state;
mod track_list_filter_actions;
mod track_list_focus;
mod track_list_geometry;
pub(in crate::ui) mod track_list_header_style;
pub(in crate::ui) mod track_list_keyboard_reorder;
pub(in crate::ui) mod track_list_layout;
pub(in crate::ui) mod track_list_menu_seams;
pub(crate) mod track_list_menu_smoke;
pub(in crate::ui) mod track_list_missing;
pub(crate) mod track_list_model;
mod track_list_model_change;
pub(in crate::ui) mod track_list_queue_menu;
pub(in crate::ui) mod track_list_reload;
pub(in crate::ui) mod track_list_rescan;
pub(in crate::ui) mod track_list_row_interaction;
pub(crate) mod track_list_smoke;
pub(crate) mod track_list_sort;
mod track_list_sound_similarity;
pub(crate) mod track_list_title_column;
mod track_list_toast;
mod track_list_wiring;
pub(in crate::ui) mod track_menu;
pub(crate) mod track_playback_selection;
mod track_reveal;
pub(in crate::ui) mod view_state_memory;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use playlist_reorder_guard::playlist_reorder_allowed;
pub(in crate::ui) use surface::{
    reload, set_filter_and_reload, set_source_and_reload, show_toast, OnActivate, Shared, TrackList,
};
pub(in crate::ui) use track_list_layout::{
    STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST, STACK_PAGE_MISSING,
};
